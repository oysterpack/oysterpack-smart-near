use crate::{
    components::contract_metrics::ContractMetricsComponent, interface::ContractMetrics,
    ContractBuyerBid, ContractOwner, ContractOwnerObject, ContractOwnershipAccountIdsObject,
    ERR_CONTRACT_BID_CANCEL_ACCESS_DENIED, ERR_CONTRACT_BID_NOT_ATTACHED, ERR_CONTRACT_BID_TOO_LOW,
    ERR_CONTRACT_SALE_NOT_ALLOWED, ERR_CONTRACT_SALE_PRICE_MUST_NOT_BE_ZERO,
    LOG_EVENT_CONTRACT_BID_EXPIRED, LOG_EVENT_CONTRACT_BID_LOST, LOG_EVENT_CONTRACT_FOR_SALE,
    LOG_EVENT_CONTRACT_SOLD,
};
use crate::{ContractBid, ContractSale};
use near_sdk::{env, Promise};
use oysterpack_smart_near::{
    asserts::assert_yocto_near_attached,
    domain::{Expiration, YoctoNear, ZERO_NEAR},
};

pub struct ContractSaleComponent {}

impl ContractSale for ContractSaleComponent {
    fn contract_sale_price(&self) -> Option<YoctoNear> {
        ContractOwnerObject::load().contract_sale_price()
    }

    fn contract_bid(&self) -> Option<ContractBuyerBid> {
        ContractOwnerObject::load()
            .bid()
            .map(|bid| bid.1)
            .map(|bid| {
                let account_ids = ContractOwnershipAccountIdsObject::load();
                ContractBuyerBid {
                    buyer: account_ids.buyer.as_ref().cloned().unwrap(),
                    bid,
                }
            })
    }

    fn sell_contract(&mut self, price: YoctoNear) {
        let mut contract_owner = Self::validate_sell_contract_request(price);
        match contract_owner.bid() {
            None => {
                contract_owner.sale_price = Some(price);
                LOG_EVENT_CONTRACT_FOR_SALE.log(price);
            }
            Some((_buyer, bid)) => {
                if bid.expired() {
                    let mut account_ids = ContractOwnershipAccountIdsObject::load();
                    Self::cancel_losing_bid(&mut contract_owner, &mut account_ids);
                    account_ids.save();

                    contract_owner.sale_price = Some(price);
                    LOG_EVENT_CONTRACT_FOR_SALE.log(price);
                } else if bid.amount >= price {
                    let mut account_ids = ContractOwnershipAccountIdsObject::load();
                    Self::execute_contract_sale(&mut contract_owner, &mut account_ids);
                    account_ids.save();
                } else {
                    contract_owner.sale_price = Some(price);
                    LOG_EVENT_CONTRACT_FOR_SALE.log(price);
                }
            }
        }
        contract_owner.save();
    }

    fn cancel_contract_sell_order(&mut self) {
        assert_yocto_near_attached();
        let mut contract_owner = ContractOwnerObject::assert_owner_access();
        if contract_owner.sale_price.take().is_some() {
            contract_owner.save();
        }
    }

    fn buy_contract(&mut self, expiration: Option<Expiration>) {
        let bid = YoctoNear(env::attached_deposit());
        ERR_CONTRACT_BID_NOT_ATTACHED.assert(|| bid > ZERO_NEAR);

        let mut owner = ContractOwnerObject::load();
        let mut account_ids = ContractOwnershipAccountIdsObject::load();

        match owner.bid.map(|(_, bid)| bid) {
            None => Self::place_bid(&mut owner, &mut account_ids, bid, expiration),
            Some(current_bid) => {
                ERR_CONTRACT_BID_TOO_LOW
                    .assert(|| bid > current_bid.amount || current_bid.expired());
                Self::cancel_losing_bid(&mut owner, &mut account_ids);
                Self::place_bid(&mut owner, &mut account_ids, bid, expiration);
            }
        }

        owner.save();
        account_ids.save();
    }

    fn cancel_contract_buy_order(&mut self) {
        assert_yocto_near_attached();

        let mut owner = ContractOwnerObject::load();
        if owner.bid.is_none() {
            return;
        }

        let mut account_ids = ContractOwnershipAccountIdsObject::load();
        ERR_CONTRACT_BID_CANCEL_ACCESS_DENIED
            .assert(|| account_ids.buyer == Some(env::predecessor_account_id()));

        Self::cancel_bid(&mut owner, &mut account_ids);

        owner.save();
        account_ids.save();
    }
}

impl ContractSaleComponent {
    /// 1. clears the current bid
    /// 2. refunds the bid amount back to the buyer
    fn cancel_bid(
        owner: &mut ContractOwnerObject,
        account_ids: &mut ContractOwnershipAccountIdsObject,
    ) -> ContractBid {
        ContractBid::clear_near_balance();
        let (_, bid) = owner.bid.take().expect("BUG: cancel_bid(): expected bid");
        let buyer = account_ids
            .buyer
            .take()
            .expect("BUG: cancel_bid(): expected buyer");
        Promise::new(buyer).transfer(bid.amount.value());
        bid
    }

    fn cancel_losing_bid(
        owner: &mut ContractOwnerObject,
        account_ids: &mut ContractOwnershipAccountIdsObject,
    ) {
        let bid = Self::cancel_bid(owner, account_ids);
        if bid.expired() {
            LOG_EVENT_CONTRACT_BID_EXPIRED.log("expired bid was cancelled");
        } else {
            LOG_EVENT_CONTRACT_BID_LOST.log("higher bid was placed");
        }
    }

    fn place_bid(
        owner: &mut ContractOwnerObject,
        account_ids: &mut ContractOwnershipAccountIdsObject,
        amount: YoctoNear,
        expiration: Option<Expiration>,
    ) {
        account_ids.buyer = Some(env::predecessor_account_id());
        owner.bid = Some((
            env::predecessor_account_id().into(),
            ContractBid { amount, expiration },
        ));
        ContractBid::set_near_balance(amount);

        if let Some(sale_price) = owner.sale_price {
            if amount >= sale_price {
                Self::execute_contract_sale(owner, account_ids);
            }
        }
    }

    fn validate_sell_contract_request(price: YoctoNear) -> ContractOwnerObject {
        assert_yocto_near_attached();
        let contract_owner = ContractOwnerObject::assert_owner_access();
        ERR_CONTRACT_SALE_NOT_ALLOWED.assert(
            || !contract_owner.transfer_initiated(),
            || "contract sale is not allowed because contract ownership transfer has been initiated",
        );
        ERR_CONTRACT_SALE_PRICE_MUST_NOT_BE_ZERO.assert(|| price == ZERO_NEAR);
        contract_owner
    }

    /// 1. clear the NEAR bid balance, which effectively transfers the bid balance to the owner balance
    /// 2. transfer the owner's NEAR funds out to the owner's account
    /// 3. update the `contract_owner` object
    ///    - set the new owner
    ///    - clear the bid
    ///    - clear the sale price
    /// 4. update the ['ContractOwnershipAccountIds`] object
    ///    - set the new owner account ID
    ///    - clear the buyer account ID
    /// 5. log event: LOG_EVENT_CONTRACT_SOLD
    fn execute_contract_sale(
        owner: &mut ContractOwner,
        account_ids: &mut ContractOwnershipAccountIdsObject,
    ) {
        ContractBid::clear_near_balance();

        // transfer the owner's NEAR funds out to the owner's account
        let near_balances = ContractMetricsComponent.near_balances();
        Promise::new(account_ids.owner.clone()).transfer(near_balances.owner().value());

        // update the contract owner
        let (buyer_account_id_hash, bid) = owner
            .bid
            .take()
            .expect("BUG: execute_contract_sale(): expected bid");
        owner.account_id_hash = buyer_account_id_hash;
        owner.sale_price.take();
        account_ids.owner = account_ids
            .buyer
            .take()
            .expect("BUG: execute_contract_sale(): expected buyer");

        LOG_EVENT_CONTRACT_SOLD.log(format!(
            "buyer: {}, sale price: {}",
            &account_ids.owner, bid.amount
        ));
    }
}

use crate::{
    components::contract_metrics::ContractMetricsComponent, interface::ContractMetrics,
    ContractOwner, ContractOwnerObject, ERR_CONTRACT_SALE_NOT_ALLOWED,
    ERR_CONTRACT_SALE_PRICE_MUST_NOT_BE_ZERO, LOG_EVENT_CONTRACT_BID_EXPIRED,
    LOG_EVENT_CONTRACT_FOR_SALE,
};
use crate::{
    BalanceId, ContractBid, ContractNearBalances, ContractSale, ERR_CONTRACT_BID_BALANCE_MISMATCH,
};
use oysterpack_smart_account_management::{AccountNearDataObject, ERR_ACCOUNT_NOT_REGISTERED};
use oysterpack_smart_near::{
    asserts::assert_yocto_near_attached,
    domain::{AccountIdHash, Expiration, YoctoNear, ZERO_NEAR},
};

/// Used to track the contract bid
pub const CONTRACT_BID_BALANCE_ID: BalanceId = BalanceId(255);

pub struct ContractSaleComponent {}

impl ContractSale for ContractSaleComponent {
    fn sell_contract(&mut self, price: YoctoNear) {
        let mut contract_owner = self.validate_sell_contract_request(price);
        match contract_owner.bid() {
            None => {
                contract_owner.sale_price = Some(price);
                LOG_EVENT_CONTRACT_FOR_SALE.log(price);
            }
            Some((buyer, bid)) => {
                if bid.expired() {
                    self.cancel_expired_bid(&mut contract_owner);
                    contract_owner.sale_price = Some(price);
                    LOG_EVENT_CONTRACT_FOR_SALE.log(price);
                } else if bid.amount >= price {
                    self.execute_contract_sale(&mut contract_owner);
                } else {
                    contract_owner.sale_price = Some(price);
                    LOG_EVENT_CONTRACT_FOR_SALE.log(price);
                }
            }
        }
        contract_owner.save();
    }

    fn cancel_contract_sell_order(&mut self) {
        unimplemented!()
    }

    fn buy_contract(
        &mut self,
        expiration: Option<Expiration>,
        from_contract_balance: Option<YoctoNear>,
    ) {
        unimplemented!()
    }

    fn cancel_contract_buy_order(&mut self) {
        unimplemented!()
    }
}

impl ContractSaleComponent {
    fn validate_sell_contract_request(&mut self, price: YoctoNear) -> ContractOwnerObject {
        assert_yocto_near_attached();
        let contract_owner = ContractOwnerObject::assert_owner_access();
        ERR_ACCOUNT_NOT_REGISTERED
            .assert(|| AccountNearDataObject::exists(contract_owner.account_id_hash()));
        ERR_CONTRACT_SALE_NOT_ALLOWED.assert(
            || !contract_owner.transfer_initiated(),
            || "contract sale is not allowed because contract ownership transfer has been initiated",
        );
        ERR_CONTRACT_SALE_PRICE_MUST_NOT_BE_ZERO.assert(|| price == ZERO_NEAR);
        contract_owner
    }

    /// refund bid by clearing the contract bid balance and applying the NEAR credit back to the
    /// bidder's account
    fn cancel_expired_bid(&mut self, owner: &mut ContractOwner) {
        ContractNearBalances::clear_balance(CONTRACT_BID_BALANCE_ID);

        let (buyer, bid) = owner.bid.unwrap();
        let mut buyer = AccountNearDataObject::registered_account(buyer);
        buyer.incr_near_balance(bid.amount);
        buyer.save();

        LOG_EVENT_CONTRACT_BID_EXPIRED.log("expired bid was cancelled");
    }

    // TODO
    fn execute_contract_sale(&mut self, contract_owner: &mut ContractOwner) {
        unimplemented!()
    }
}

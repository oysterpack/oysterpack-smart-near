use crate::components::contract_ownership::ContractOwnershipComponent;
use crate::{ContractBid, ContractSale, ERR_BID_IS_EXPIRED};
use crate::{
    ContractBuyerBid, ContractOwner, ContractOwnerObject, ContractOwnership,
    ContractOwnershipAccountIdsObject, ERR_ACCESS_DENIED_MUST_BE_BUYER, ERR_CONTRACT_BID_TOO_LOW,
    ERR_CONTRACT_SALE_NOT_ALLOWED, ERR_CONTRACT_SALE_PRICE_MUST_NOT_BE_ZERO, ERR_NO_ACTIVE_BID,
    ERR_OWNER_CANNOT_BUY_CONTRACT, LOG_EVENT_CONTRACT_BID_CANCELLED,
    LOG_EVENT_CONTRACT_BID_EXPIRATION_CHANGE, LOG_EVENT_CONTRACT_BID_LOWERED,
    LOG_EVENT_CONTRACT_BID_PLACED, LOG_EVENT_CONTRACT_BID_RAISED, LOG_EVENT_CONTRACT_FOR_SALE,
    LOG_EVENT_CONTRACT_SALE_CANCELLED, LOG_EVENT_CONTRACT_SOLD,
};
use near_sdk::{env, Promise};
use oysterpack_smart_near::asserts::assert_near_attached;
use oysterpack_smart_near::domain::ExpirationSetting;
use oysterpack_smart_near::{
    asserts::assert_yocto_near_attached,
    domain::{Expiration, YoctoNear, ZERO_NEAR},
    LogEvent,
};

pub struct ContractSaleComponent;

impl ContractSale for ContractSaleComponent {
    fn contract_sale_price() -> Option<YoctoNear> {
        ContractOwnerObject::load().contract_sale_price()
    }

    fn contract_bid() -> Option<ContractBuyerBid> {
        ContractOwnerObject::load()
            .bid()
            .map(|bid| bid.1)
            .map(|bid| {
                let account_ids = ContractOwnershipAccountIdsObject::load();
                ContractBuyerBid {
                    buyer: account_ids
                        .buyer
                        .as_ref()
                        .cloned()
                        .expect("BUG: contract_bid(): expected buyer"),
                    bid,
                }
            })
    }

    fn sell_contract(&mut self, price: YoctoNear) {
        let mut contract_owner = Self::validate_sell_contract_request(price);
        match contract_owner.bid() {
            None => match contract_owner.sale_price {
                Some(current_price) if price == current_price => return,
                _ => {
                    contract_owner.sale_price = Some(price);
                    LOG_EVENT_CONTRACT_FOR_SALE.log(price);
                }
            },
            Some((_buyer, bid)) => {
                if bid.expired() {
                    let mut account_ids = ContractOwnershipAccountIdsObject::load();
                    Self::cancel_bid(&mut contract_owner, &mut account_ids, "bid expired");
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

    fn cancel_contract_sale(&mut self) {
        assert_yocto_near_attached();
        let mut contract_owner = ContractOwnerObject::assert_owner_access();
        if contract_owner.sale_price.take().is_some() {
            contract_owner.save();
            LOG_EVENT_CONTRACT_SALE_CANCELLED.log("");
        }
    }

    fn buy_contract(&mut self, expiration: Option<ExpirationSetting>) {
        assert_near_attached("contract bid cannot be zero");
        let expiration = expiration.map(|expiration| {
            let expiration: Expiration = expiration.into();
            ERR_BID_IS_EXPIRED.assert(|| !expiration.expired());
            expiration
        });

        let mut account_ids = ContractOwnershipAccountIdsObject::load();
        ERR_OWNER_CANNOT_BUY_CONTRACT.assert(|| env::predecessor_account_id() != account_ids.owner);
        let mut owner = ContractOwnerObject::load();
        ERR_CONTRACT_SALE_NOT_ALLOWED.assert(
            || !owner.transfer_initiated(),
            || "bid cannot be placed while contract ownership is being transferred",
        );

        let bid = YoctoNear(env::attached_deposit());
        match owner.bid.map(|(_, bid)| bid) {
            None => Self::place_bid(&mut owner, &mut account_ids, bid, expiration),
            Some(current_bid) => {
                ERR_CONTRACT_BID_TOO_LOW
                    .assert(|| bid > current_bid.amount || current_bid.expired());
                Self::cancel_bid(&mut owner, &mut account_ids, "higher bid has been placed");
                Self::place_bid(&mut owner, &mut account_ids, bid, expiration);
            }
        }

        owner.save();
        account_ids.save();
    }

    fn raise_contract_bid(&mut self, expiration: Option<ExpirationSetting>) {
        assert_near_attached("NEAR attached deposit is required");

        let mut owner = ContractOwnerObject::load();
        match owner.bid {
            None => ERR_NO_ACTIVE_BID.panic(),
            Some((buyer_account_id_hash, mut bid)) => {
                ERR_ACCESS_DENIED_MUST_BE_BUYER
                    .assert(|| buyer_account_id_hash == env::predecessor_account_id().into());

                let amount = env::attached_deposit().into();
                ContractBid::incr_near_balance(amount);

                bid.amount += amount;
                bid.update_expiration(expiration);
                owner.bid = Some((buyer_account_id_hash, bid));

                if let Some(contract_sale_price) = owner.sale_price {
                    if bid.amount >= contract_sale_price {
                        let mut account_ids = ContractOwnershipAccountIdsObject::load();
                        Self::execute_contract_sale(&mut owner, &mut account_ids);
                        account_ids.save();
                    } else {
                        Self::log_bid_raised(bid);
                    }
                } else {
                    Self::log_bid_raised(bid);
                }
            }
        }

        owner.save();
    }

    fn lower_contract_bid(&mut self, amount: YoctoNear, expiration: Option<ExpirationSetting>) {
        assert_yocto_near_attached();

        let mut owner = ContractOwnerObject::load();
        match owner.bid {
            None => ERR_NO_ACTIVE_BID.panic(),
            Some((buyer_account_id_hash, mut bid)) => {
                ERR_ACCESS_DENIED_MUST_BE_BUYER
                    .assert(|| buyer_account_id_hash == env::predecessor_account_id().into());

                ContractBid::decr_near_balance(amount);

                bid.amount -= amount;
                bid.update_expiration(expiration);
                Self::log_bid_lowered(bid);

                owner.bid = Some((buyer_account_id_hash, bid));
            }
        }

        owner.save();
        Promise::new(env::predecessor_account_id()).transfer(amount.value() + 1);
    }

    fn update_contract_bid_expiration(&mut self, expiration: Option<ExpirationSetting>) {
        assert_yocto_near_attached();

        let mut owner = ContractOwnerObject::load();
        match owner.bid {
            None => ERR_NO_ACTIVE_BID.panic(),
            Some((buyer_account_id_hash, mut bid)) => {
                ERR_ACCESS_DENIED_MUST_BE_BUYER
                    .assert(|| buyer_account_id_hash == env::predecessor_account_id().into());

                bid.expiration = expiration.map(Into::into);
                owner.bid = Some((buyer_account_id_hash, bid));
                Self::log_bid_event(LOG_EVENT_CONTRACT_BID_EXPIRATION_CHANGE, bid);
            }
        }

        owner.save();
    }

    fn cancel_contract_bid(&mut self) {
        assert_yocto_near_attached();

        let mut owner = ContractOwnerObject::load();
        if owner.bid.is_none() {
            return;
        }

        let mut account_ids = ContractOwnershipAccountIdsObject::load();
        ERR_ACCESS_DENIED_MUST_BE_BUYER
            .assert(|| account_ids.buyer == Some(env::predecessor_account_id()));

        Self::cancel_bid(&mut owner, &mut account_ids, "");

        owner.save();
        account_ids.save();
    }
}

impl ContractSaleComponent {
    fn log_bid_event(event: LogEvent, bid: ContractBid) {
        match bid.expiration {
            None => event.log(format!("bid: {}", bid.amount)),
            Some(expiration) => LOG_EVENT_CONTRACT_BID_PLACED
                .log(format!("bid: {} | expiration: {}", bid.amount, expiration)),
        }
    }

    fn log_bid_placed(bid: ContractBid) {
        Self::log_bid_event(LOG_EVENT_CONTRACT_BID_PLACED, bid);
    }

    fn log_bid_raised(bid: ContractBid) {
        Self::log_bid_event(LOG_EVENT_CONTRACT_BID_RAISED, bid);
    }

    fn log_bid_lowered(bid: ContractBid) {
        Self::log_bid_event(LOG_EVENT_CONTRACT_BID_LOWERED, bid);
    }

    /// 1. clears the current bid
    /// 2. refunds the bid amount back to the buyer
    pub(crate) fn cancel_bid(
        owner: &mut ContractOwnerObject,
        account_ids: &mut ContractOwnershipAccountIdsObject,
        msg: &str,
    ) -> ContractBid {
        ContractBid::clear_near_balance();
        let (_, bid) = owner.bid.take().expect("BUG: cancel_bid(): expected bid");
        let buyer = account_ids
            .buyer
            .take()
            .expect("BUG: cancel_bid(): expected buyer");
        Promise::new(buyer).transfer(bid.amount.value());
        LOG_EVENT_CONTRACT_BID_CANCELLED.log(msg);
        bid
    }

    // fn cancel_losing_bid(
    //     owner: &mut ContractOwnerObject,
    //     account_ids: &mut ContractOwnershipAccountIdsObject,
    // ) {
    //     let bid = Self::cancel_bid(owner, account_ids);
    //     if bid.expired() {
    //         LOG_EVENT_CONTRACT_BID_EXPIRED.log("bid expired");
    //     } else {
    //         LOG_EVENT_CONTRACT_BID_LOST.log("higher bid was placed");
    //     }
    // }

    fn place_bid(
        owner: &mut ContractOwnerObject,
        account_ids: &mut ContractOwnershipAccountIdsObject,
        amount: YoctoNear,
        expiration: Option<Expiration>,
    ) {
        account_ids.buyer = Some(env::predecessor_account_id());
        let bid = ContractBid { amount, expiration };
        owner.bid = Some((env::predecessor_account_id().into(), bid));
        ContractBid::set_near_balance(amount);

        if let Some(sale_price) = owner.sale_price {
            if amount >= sale_price {
                Self::execute_contract_sale(owner, account_ids);
                return;
            }
        }

        Self::log_bid_placed(bid);
    }

    fn validate_sell_contract_request(price: YoctoNear) -> ContractOwnerObject {
        assert_yocto_near_attached();
        let contract_owner = ContractOwnerObject::assert_owner_access();
        ERR_CONTRACT_SALE_NOT_ALLOWED.assert(
            || !contract_owner.transfer_initiated(),
            || "contract cannot be sold after transfer process has been started",
        );
        ERR_CONTRACT_SALE_PRICE_MUST_NOT_BE_ZERO.assert(|| price > ZERO_NEAR);
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
        let owner_balance = ContractOwnershipComponent::owner_balance();
        Promise::new(account_ids.owner.clone()).transfer(owner_balance.available.value());

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
            "buyer={}, price={}",
            &account_ids.owner, bid.amount
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::contract_ownership::ContractOwnershipComponent;
    use crate::ContractOwnership;
    use near_sdk::test_utils;
    use oysterpack_smart_near::component::*;
    use oysterpack_smart_near::domain::ExpirationDuration;
    use oysterpack_smart_near::YOCTO;
    use oysterpack_smart_near_test::*;

    #[test]
    fn contract_sale_basic_workflow() {
        // Arrange
        let alfio = "alfio";
        let bob = "bob";

        let mut ctx = new_context(alfio);
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        let mut service = ContractSaleComponent;
        assert!(ContractSaleComponent::contract_sale_price().is_none());
        // should be harmless to call by the owner - should have no effect
        service.cancel_contract_sale();
        assert!(ContractSaleComponent::contract_bid().is_none());
        // should have no effect and should be harmless to call when there is no bid
        service.cancel_contract_bid();

        // Act - Bob will submit a bid to buy the contract
        ctx.predecessor_account_id = bob.to_string();
        ctx.attached_deposit = 1000;
        testing_env!(ctx.clone());
        service.buy_contract(None);
        // Assert
        let logs = test_utils::get_logs();
        println!("{:#?}", logs);
        let bid = ContractSaleComponent::contract_bid().unwrap();
        assert_eq!(bid.buyer.as_str(), bob);
        assert_eq!(bid.bid.amount.value(), 1000);
        assert_eq!(ContractBid::near_balance(), bid.bid.amount);
        assert!(bid.bid.expiration.is_none());

        // Act - Bob raises the bid
        testing_env!(ctx.clone());
        service.raise_contract_bid(None);
        // Assert
        let logs = test_utils::get_logs();
        println!("{:#?}", logs);
        let bid = ContractSaleComponent::contract_bid().unwrap();
        assert_eq!(bid.buyer.as_str(), bob);
        assert_eq!(bid.bid.amount.value(), 2000);
        assert_eq!(ContractBid::near_balance(), bid.bid.amount);
        assert!(bid.bid.expiration.is_none());

        // Act - Bob raises the bid and updates expiration
        testing_env!(ctx.clone());
        service.raise_contract_bid(None);
        // Assert
        let logs = test_utils::get_logs();
        println!("{:#?}", logs);
        let bid = ContractSaleComponent::contract_bid().unwrap();
        assert_eq!(bid.buyer.as_str(), bob);
        assert_eq!(bid.bid.amount.value(), 3000);
        assert_eq!(ContractBid::near_balance(), bid.bid.amount);
        assert!(bid.bid.expiration.is_none());

        // Act - Bob sets an expiration
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        service.update_contract_bid_expiration(Some(ExpirationSetting::Relative(
            ExpirationDuration::Epochs(10),
        )));
        // Assert
        let logs = test_utils::get_logs();
        println!("{:#?}", logs);
        let bid = ContractSaleComponent::contract_bid().unwrap();
        assert_eq!(bid.buyer.as_str(), bob);
        assert_eq!(bid.bid.amount.value(), 3000);
        assert_eq!(ContractBid::near_balance(), bid.bid.amount);
        assert_eq!(
            bid.bid.expiration,
            Some(ExpirationSetting::Relative(ExpirationDuration::Epochs(10),).into())
        );

        // Act - Bob clears the expiration
        testing_env!(ctx.clone());
        service.update_contract_bid_expiration(None);
        // Assert
        let logs = test_utils::get_logs();
        println!("{:#?}", logs);
        let bid = ContractSaleComponent::contract_bid().unwrap();
        assert_eq!(bid.buyer.as_str(), bob);
        assert_eq!(bid.bid.amount.value(), 3000);
        assert_eq!(ContractBid::near_balance(), bid.bid.amount);
        assert!(bid.bid.expiration.is_none());

        // Act - Bob lowers the bid
        testing_env!(ctx.clone());
        service.lower_contract_bid(1000.into(), None);
        // Assert
        let logs = test_utils::get_logs();
        println!("{:#?}", logs);
        let bid = ContractSaleComponent::contract_bid().unwrap();
        assert_eq!(bid.buyer.as_str(), bob);
        assert_eq!(bid.bid.amount.value(), 2000);
        assert_eq!(ContractBid::near_balance(), bid.bid.amount);
        assert!(bid.bid.expiration.is_none());
        let receipts = deserialize_receipts();
        let action = &receipts[0].actions[0];
        match action {
            Action::Transfer(transfer) => {
                assert_eq!(transfer.deposit, 1001);
            }
            _ => panic!("expected TransferAction"),
        }

        // Act - owner sells contract
        ctx.predecessor_account_id = alfio.to_string();
        testing_env!(ctx.clone());
        service.sell_contract(YOCTO.into());
        // Assert
        let logs = test_utils::get_logs();
        println!("{:#?}", logs);
        assert_eq!(
            ContractSaleComponent::contract_sale_price(),
            Some(YOCTO.into())
        );
        let bid = ContractSaleComponent::contract_bid().unwrap();
        assert_eq!(bid.buyer.as_str(), bob);
        assert_eq!(bid.bid.amount.value(), 2000);
        assert_eq!(ContractBid::near_balance(), bid.bid.amount);
        assert!(bid.bid.expiration.is_none());

        // Act - owner cancels sale
        testing_env!(ctx.clone());
        service.cancel_contract_sale();
        // Assert
        let logs = test_utils::get_logs();
        println!("{:#?}", logs);
        assert!(ContractSaleComponent::contract_sale_price().is_none());
        let bid = ContractSaleComponent::contract_bid().unwrap();
        assert_eq!(bid.buyer.as_str(), bob);
        assert_eq!(bid.bid.amount.value(), 2000);
        assert_eq!(ContractBid::near_balance(), bid.bid.amount);
        assert!(bid.bid.expiration.is_none());

        // Act - buyer cancels bid
        ctx.predecessor_account_id = ContractSaleComponent::contract_bid().unwrap().buyer.clone();
        testing_env!(ctx.clone());
        service.cancel_contract_bid();
        // Assert
        let logs = test_utils::get_logs();
        println!("{:#?}", logs);
        assert!(ContractSaleComponent::contract_sale_price().is_none());
        assert!(ContractSaleComponent::contract_bid().is_none());
        assert_eq!(ContractBid::near_balance(), ZERO_NEAR);

        // Act - owner sells contract
        ctx.predecessor_account_id = alfio.to_string();
        testing_env!(ctx.clone());
        service.sell_contract(YOCTO.into());
        // Assert
        let logs = test_utils::get_logs();
        println!("{:#?}", logs);
        assert_eq!(
            ContractSaleComponent::contract_sale_price(),
            Some(YOCTO.into())
        );

        // Act - Bob will submit a bid high enough to buy the contract
        ctx.predecessor_account_id = bob.to_string();
        ctx.attached_deposit = YOCTO;
        testing_env!(ctx.clone());
        let previous_owner = ContractOwnershipComponent::owner();
        let owner_balance = ContractOwnershipComponent::owner_balance();
        service.buy_contract(None);
        // Assert
        let logs = test_utils::get_logs();
        println!("{:#?}", logs);
        assert_eq!(ContractOwnershipComponent::owner().as_str(), bob);
        assert_eq!(ContractBid::near_balance(), ZERO_NEAR);
        let receipts = deserialize_receipts();
        assert_eq!(&previous_owner, &receipts[0].receiver_id.as_str());
        let action = &receipts[0].actions[0];
        match action {
            Action::Transfer(transfer) => {
                assert_eq!(transfer.deposit, owner_balance.available.value());
            }
            _ => panic!("expected TransferAction"),
        }
    }
}

#[cfg(test)]
mod tests_sell_contract {
    use super::*;
    use crate::components::contract_ownership::ContractOwnershipComponent;
    use near_sdk::test_utils;
    use oysterpack_smart_near::component::*;
    use oysterpack_smart_near::YOCTO;
    use oysterpack_smart_near_test::*;

    #[test]
    fn new_sale_no_bid() {
        // Arrange
        let alfio = "alfio";

        let mut ctx = new_context(alfio);
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        // Act
        ContractSaleComponent.sell_contract(YOCTO.into());
        // Assert
        assert_eq!(
            ContractSaleComponent::contract_sale_price(),
            Some(YOCTO.into())
        );
        let logs = test_utils::get_logs();

        assert_eq!(
            &logs[0],
            LOG_EVENT_CONTRACT_FOR_SALE.message(YOCTO).as_str()
        );
    }

    #[test]
    fn update_sale_no_bid() {
        // Arrange
        let alfio = "alfio";

        let mut ctx = new_context(alfio);
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        // Act
        ContractSaleComponent.sell_contract(YOCTO.into());
        ContractSaleComponent.sell_contract((2 * YOCTO).into());
        // Assert
        assert_eq!(
            ContractSaleComponent::contract_sale_price(),
            Some((2 * YOCTO).into())
        );
        let logs = test_utils::get_logs();

        assert_eq!(
            &logs[0],
            LOG_EVENT_CONTRACT_FOR_SALE.message(YOCTO).as_str()
        );
        assert_eq!(
            &logs[1],
            LOG_EVENT_CONTRACT_FOR_SALE.message(2 * YOCTO).as_str()
        );
    }

    #[test]
    fn update_sale_with_same_price_no_bid() {
        // Arrange
        let alfio = "alfio";

        let mut ctx = new_context(alfio);
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        // Act
        ContractSaleComponent.sell_contract(YOCTO.into());
        ContractSaleComponent.sell_contract(YOCTO.into());
        // Assert
        assert_eq!(
            ContractSaleComponent::contract_sale_price(),
            Some(YOCTO.into())
        );
        let logs = test_utils::get_logs();
        assert_eq!(
            &logs[0],
            LOG_EVENT_CONTRACT_FOR_SALE.message(YOCTO).as_str()
        );
        assert_eq!(logs.len(), 1);
    }

    #[test]
    fn new_sale_lower_bid() {
        // Arrange
        let owner = "alfio";
        let buyer = "bob";

        let mut ctx = new_context(owner);
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(owner)));
        ctx.attached_deposit = 100;
        ctx.predecessor_account_id = buyer.to_string();
        testing_env!(ctx.clone());
        ContractSaleComponent.buy_contract(None);

        // Act
        ctx.attached_deposit = 1;
        ctx.predecessor_account_id = owner.to_string();
        testing_env!(ctx.clone());
        ContractSaleComponent.sell_contract(YOCTO.into());
        // Assert
        assert_eq!(
            ContractSaleComponent::contract_sale_price(),
            Some(YOCTO.into())
        );
        let logs = test_utils::get_logs();

        assert_eq!(
            &logs[0],
            LOG_EVENT_CONTRACT_FOR_SALE.message(YOCTO).as_str()
        );
    }

    #[test]
    fn new_sale_matching_bid() {
        // Arrange
        let owner = "alfio";
        let buyer = "bob";

        let mut ctx = new_context(owner);
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(owner)));
        ctx.attached_deposit = YOCTO;
        ctx.predecessor_account_id = buyer.to_string();
        testing_env!(ctx.clone());
        ContractSaleComponent.buy_contract(None);

        // Act
        ctx.attached_deposit = 1;
        ctx.predecessor_account_id = owner.to_string();
        testing_env!(ctx.clone());
        ContractSaleComponent.sell_contract(YOCTO.into());
        // Assert
        assert!(ContractSaleComponent::contract_sale_price().is_none());
        assert!(ContractSaleComponent::contract_bid().is_none());
        assert!(ContractOwnershipAccountIdsObject::load().buyer.is_none());
        assert_eq!(ContractOwnershipComponent::owner(), buyer.to_string());

        let logs = test_utils::get_logs();

        assert_eq!(
            &logs[0],
            LOG_EVENT_CONTRACT_SOLD
                .message(format!("buyer={}, price={}", buyer, YOCTO))
                .as_str()
        );
    }

    #[test]
    fn new_sale_higher_bid() {
        // Arrange
        let owner = "alfio";
        let buyer = "bob";

        let mut ctx = new_context(owner);
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(owner)));
        ctx.attached_deposit = 2 * YOCTO;
        ctx.predecessor_account_id = buyer.to_string();
        testing_env!(ctx.clone());
        ContractSaleComponent.buy_contract(None);

        // Act
        ctx.attached_deposit = 1;
        ctx.predecessor_account_id = owner.to_string();
        testing_env!(ctx.clone());
        ContractSaleComponent.sell_contract(YOCTO.into());
        // Assert
        assert!(ContractSaleComponent::contract_sale_price().is_none());
        assert!(ContractSaleComponent::contract_bid().is_none());
        assert!(ContractOwnershipAccountIdsObject::load().buyer.is_none());
        assert_eq!(ContractOwnershipComponent::owner(), buyer.to_string());

        let logs = test_utils::get_logs();

        assert_eq!(
            &logs[0],
            LOG_EVENT_CONTRACT_SOLD
                .message(format!("buyer={}, price={}", buyer, 2 * YOCTO))
                .as_str()
        );
    }

    #[test]
    fn new_sale_expired_bid() {
        // Arrange
        let owner = "alfio";
        let buyer = "bob";

        let mut ctx = new_context(owner);
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(owner)));
        ctx.attached_deposit = 2 * YOCTO;
        ctx.predecessor_account_id = buyer.to_string();
        ctx.epoch_height = 100;
        testing_env!(ctx.clone());
        ContractSaleComponent.buy_contract(Some(ExpirationSetting::Absolute(Expiration::Epoch(
            200.into(),
        ))));

        // Act
        ctx.attached_deposit = 1;
        ctx.predecessor_account_id = owner.to_string();
        ctx.epoch_height = 201;
        testing_env!(ctx.clone());
        ContractSaleComponent.sell_contract(YOCTO.into());
        // Assert
        assert_eq!(
            ContractSaleComponent::contract_sale_price(),
            Some(YOCTO.into())
        );
        assert!(ContractSaleComponent::contract_bid().is_none());
        assert!(ContractOwnershipAccountIdsObject::load().buyer.is_none());
        assert_eq!(ContractOwnershipComponent::owner(), owner.to_string());

        let logs = test_utils::get_logs();
        println!("{:#?}", logs);
        assert_eq!(
            &logs[0],
            LOG_EVENT_CONTRACT_BID_CANCELLED
                .message("bid expired")
                .as_str()
        );
        assert_eq!(
            &logs[1],
            LOG_EVENT_CONTRACT_FOR_SALE.message(YOCTO).as_str()
        );
    }

    #[test]
    fn updated_sale_matching_bid() {
        // Arrange
        let owner = "alfio";
        let buyer = "bob";

        let mut ctx = new_context(owner);
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(owner)));
        ctx.attached_deposit = YOCTO;
        ctx.predecessor_account_id = buyer.to_string();
        testing_env!(ctx.clone());
        ContractSaleComponent.buy_contract(None);

        // Act
        ctx.attached_deposit = 1;
        ctx.predecessor_account_id = owner.to_string();
        testing_env!(ctx.clone());
        ContractSaleComponent.sell_contract((5 * YOCTO).into());
        ContractSaleComponent.sell_contract(YOCTO.into());
        // Assert
        assert!(ContractSaleComponent::contract_sale_price().is_none());
        assert!(ContractSaleComponent::contract_bid().is_none());
        assert!(ContractOwnershipAccountIdsObject::load().buyer.is_none());
        assert_eq!(ContractOwnershipComponent::owner(), buyer.to_string());

        let logs = test_utils::get_logs();

        assert_eq!(
            &logs[0],
            LOG_EVENT_CONTRACT_FOR_SALE.message(5 * YOCTO).as_str()
        );
        assert_eq!(
            &logs[1],
            LOG_EVENT_CONTRACT_SOLD
                .message(format!("buyer={}, price={}", buyer, YOCTO))
                .as_str()
        );
    }

    #[test]
    fn update_sale_higher_bid() {
        // Arrange
        let owner = "alfio";
        let buyer = "bob";

        let mut ctx = new_context(owner);
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(owner)));
        ctx.attached_deposit = 2 * YOCTO;
        ctx.predecessor_account_id = buyer.to_string();
        testing_env!(ctx.clone());
        ContractSaleComponent.buy_contract(None);

        // Act
        ctx.attached_deposit = 1;
        ctx.predecessor_account_id = owner.to_string();
        testing_env!(ctx.clone());
        ContractSaleComponent.sell_contract((5 * YOCTO).into());
        ContractSaleComponent.sell_contract(YOCTO.into());
        // Assert
        assert!(ContractSaleComponent::contract_sale_price().is_none());
        assert!(ContractSaleComponent::contract_bid().is_none());
        assert!(ContractOwnershipAccountIdsObject::load().buyer.is_none());
        assert_eq!(ContractOwnershipComponent::owner(), buyer.to_string());

        let logs = test_utils::get_logs();

        assert_eq!(
            &logs[0],
            LOG_EVENT_CONTRACT_FOR_SALE.message(5 * YOCTO).as_str()
        );
        assert_eq!(
            &logs[1],
            LOG_EVENT_CONTRACT_SOLD
                .message(format!("buyer={}, price={}", buyer, 2 * YOCTO))
                .as_str()
        );
    }

    #[test]
    #[should_panic(
        expected = "[ERR] [CONTRACT_SALE_NOT_ALLOWED] contract cannot be sold after transfer process has been started"
    )]
    fn transfer_ownership_initiated() {
        // Arrange
        let owner = "alfio";
        let buyer = "bob";

        let mut ctx = new_context(owner);
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(owner)));
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        ContractOwnershipComponent.transfer_ownership(to_valid_account_id(buyer));

        // Act
        ctx.attached_deposit = 1;
        ctx.predecessor_account_id = owner.to_string();
        testing_env!(ctx.clone());
        ContractSaleComponent.sell_contract(YOCTO.into());
    }

    #[test]
    #[should_panic(expected = "[ERR] [OWNER_ACCESS_REQUIRED]")]
    fn not_owner() {
        // Arrange
        let owner = "alfio";

        let mut ctx = new_context(owner);
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(owner)));

        // Act
        ctx.attached_deposit = 1;
        ctx.predecessor_account_id = "bob".to_string();
        testing_env!(ctx.clone());
        ContractSaleComponent.sell_contract(YOCTO.into());
    }

    #[test]
    #[should_panic(expected = "[ERR] [YOCTONEAR_DEPOSIT_REQUIRED]")]
    fn zero_deposit() {
        // Arrange
        let owner = "alfio";

        let mut ctx = new_context(owner);
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(owner)));

        // Act
        ctx.attached_deposit = 0;
        testing_env!(ctx.clone());
        ContractSaleComponent.sell_contract(YOCTO.into());
    }

    #[test]
    #[should_panic(expected = "[ERR] [YOCTONEAR_DEPOSIT_REQUIRED]")]
    fn two_deposit() {
        // Arrange
        let owner = "alfio";

        let mut ctx = new_context(owner);
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(owner)));

        // Act
        ctx.attached_deposit = 2;
        testing_env!(ctx.clone());
        ContractSaleComponent.sell_contract(YOCTO.into());
    }

    #[test]
    #[should_panic(expected = "[ERR] [CONTRACT_SALE_PRICE_MUST_NOT_BE_ZERO]")]
    fn zero_sale_price() {
        // Arrange
        let owner = "alfio";

        let mut ctx = new_context(owner);
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(owner)));

        // Act
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        ContractSaleComponent.sell_contract(ZERO_NEAR);
    }
}

#[cfg(test)]
mod tests_buy_contract {
    use super::*;
    use crate::components::contract_ownership::ContractOwnershipComponent;
    use near_sdk::{test_utils, VMContext};
    use oysterpack_smart_near::component::*;
    use oysterpack_smart_near::YOCTO;
    use oysterpack_smart_near_test::*;

    const OWNER: &str = "owner";
    const BUYER_1: &str = "buyer1";
    const BUYER_2: &str = "buyer2";

    fn arrange(sale_price: Option<YoctoNear>, bid: Option<ContractBuyerBid>) -> VMContext {
        let ctx = new_context(OWNER);
        {
            let mut ctx = ctx.clone();
            ctx.attached_deposit = 1;
            testing_env!(ctx.clone());

            ContractOwnershipComponent::deploy(Some(to_valid_account_id(OWNER)));
            if let Some(sale_price) = sale_price {
                ContractSaleComponent.sell_contract(sale_price);
            }

            if let Some(bid) = bid {
                ctx.predecessor_account_id = bid.buyer;
                ctx.attached_deposit = bid.bid.amount.value();
                testing_env!(ctx.clone());
                ContractSaleComponent
                    .buy_contract(bid.bid.expiration.as_ref().cloned().map(Into::into));
            }
        }
        assert_eq!(ContractOwnershipComponent::owner(), OWNER.to_string());
        ctx
    }

    #[test]
    #[should_panic(expected = "[ERR] [NEAR_DEPOSIT_REQUIRED]")]
    fn zero_yocto_near_attached() {
        let mut ctx = new_context(OWNER);
        ctx.attached_deposit = 0;
        ctx.predecessor_account_id = BUYER_1.to_string();
        testing_env!(ctx.clone());
        ContractSaleComponent.buy_contract(None);
    }

    #[test]
    fn higher_sale_price_and_lower_prior_bid_() {
        let mut ctx = arrange(
            None,
            Some(ContractBuyerBid {
                buyer: BUYER_2.to_string(),
                bid: ContractBid {
                    amount: 1000.into(),
                    expiration: None,
                },
            }),
        );

        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        ContractSaleComponent.sell_contract(YOCTO.into());

        ctx.predecessor_account_id = BUYER_1.to_string();
        ctx.attached_deposit = 1001;
        testing_env!(ctx.clone());
        ContractSaleComponent.buy_contract(None);
        let bid = ContractSaleComponent::contract_bid().unwrap();
        assert_eq!(bid.buyer, ctx.predecessor_account_id);
        assert_eq!(bid.bid.amount, ctx.attached_deposit.into());

        let logs = test_utils::get_logs();
        println!("{:#?}", logs);
        assert_eq!(
            logs,
            vec![
                LOG_EVENT_CONTRACT_BID_CANCELLED.message("higher bid has been placed"),
                LOG_EVENT_CONTRACT_BID_PLACED.message("bid: 1001")
            ]
        );

        let receipts = deserialize_receipts();
        assert_eq!(&receipts[0].receiver_id, BUYER_2);
        match &receipts[0].actions[0] {
            Action::Transfer(transfer) => assert_eq!(transfer.deposit, 1000),
            _ => panic!("expected TransferAction"),
        }
    }

    #[test]
    #[should_panic(expected = "[ERR] [CONTRACT_SALE_NOT_ALLOWED]")]
    fn with_contract_transfer_initiated() {
        let mut ctx = arrange(None, None);

        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        ContractOwnershipComponent.transfer_ownership(to_valid_account_id(BUYER_2));

        ctx.predecessor_account_id = BUYER_1.to_string();
        ctx.attached_deposit = 10000;
        testing_env!(ctx.clone());
        ContractSaleComponent.buy_contract(None);
    }

    #[cfg(test)]
    mod no_sale_no_bid {
        use super::*;
        use oysterpack_smart_near::domain::ExpirationDuration;

        #[test]
        fn no_expiration() {
            let mut ctx = arrange(None, None);

            ctx.predecessor_account_id = BUYER_1.to_string();
            ctx.attached_deposit = 100;
            testing_env!(ctx.clone());
            ContractSaleComponent.buy_contract(None);

            let bid = ContractSaleComponent::contract_bid().unwrap();
            assert_eq!(bid.buyer, BUYER_1.to_string());
            assert_eq!(bid.bid.amount, 100.into());
            assert!(bid.bid.expiration.is_none());

            let logs = test_utils::get_logs();
            println!("{:#?}", logs);
            assert_eq!(
                &logs[0],
                LOG_EVENT_CONTRACT_BID_PLACED.message("bid: 100").as_str()
            );
        }

        #[test]
        fn with_future_expiration() {
            let mut ctx = arrange(None, None);

            ctx.predecessor_account_id = BUYER_1.to_string();
            ctx.attached_deposit = 100;
            ctx.epoch_height = 10;
            testing_env!(ctx.clone());
            let expiration = Expiration::Epoch(20.into());
            ContractSaleComponent.buy_contract(Some(expiration.into()));

            let bid = ContractSaleComponent::contract_bid().unwrap();
            assert_eq!(bid.buyer, BUYER_1.to_string());
            assert_eq!(bid.bid.amount, 100.into());
            assert_eq!(bid.bid.expiration.unwrap(), expiration);

            let logs = test_utils::get_logs();
            println!("{:#?}", logs);
            assert_eq!(
                &logs[0],
                LOG_EVENT_CONTRACT_BID_PLACED
                    .message("bid: 100 | expiration: EpochHeight(20)")
                    .as_str()
            );
        }

        #[test]
        fn with_future_relative_expiration() {
            let mut ctx = arrange(None, None);

            ctx.predecessor_account_id = BUYER_1.to_string();
            ctx.attached_deposit = 100;
            ctx.epoch_height = 10;
            testing_env!(ctx.clone());
            let expiration = Expiration::Epoch(20.into());
            ContractSaleComponent.buy_contract(Some(ExpirationSetting::Relative(
                ExpirationDuration::Epochs(10),
            )));

            let bid = ContractSaleComponent::contract_bid().unwrap();
            assert_eq!(bid.buyer, BUYER_1.to_string());
            assert_eq!(bid.bid.amount, 100.into());
            assert_eq!(bid.bid.expiration.unwrap(), expiration);

            let logs = test_utils::get_logs();
            println!("{:#?}", logs);
            assert_eq!(
                &logs[0],
                LOG_EVENT_CONTRACT_BID_PLACED
                    .message("bid: 100 | expiration: EpochHeight(20)")
                    .as_str()
            );
        }

        #[test]
        #[should_panic(expected = "[ERR] [BID_IS_EXPIRED]")]
        fn with_expired_bid() {
            let mut ctx = arrange(None, None);

            ctx.predecessor_account_id = BUYER_1.to_string();
            ctx.attached_deposit = 100;
            ctx.epoch_height = 10;
            testing_env!(ctx.clone());
            let expiration = Expiration::Epoch(5.into());
            ContractSaleComponent.buy_contract(Some(expiration.into()));
        }
    }

    #[cfg(test)]
    mod with_sale_no_bid {
        use super::*;
        use oysterpack_smart_near::YOCTO;

        #[test]
        fn higher_sale_price() {
            let mut ctx = arrange(Some(1000.into()), None);

            ctx.predecessor_account_id = BUYER_1.to_string();
            ctx.attached_deposit = 100;
            testing_env!(ctx.clone());
            ContractSaleComponent.buy_contract(None);

            let bid = ContractSaleComponent::contract_bid().unwrap();
            assert_eq!(bid.buyer, BUYER_1.to_string());
            assert_eq!(bid.bid.amount, 100.into());
            assert!(bid.bid.expiration.is_none());

            let logs = test_utils::get_logs();
            println!("{:#?}", logs);
            assert_eq!(
                &logs[0],
                LOG_EVENT_CONTRACT_BID_PLACED.message("bid: 100").as_str()
            );
        }

        #[test]
        fn with_matching_sale_price() {
            let mut ctx = arrange(Some((YOCTO * 1_000_000).into()), None);

            ctx.predecessor_account_id = BUYER_1.to_string();
            ctx.attached_deposit = YOCTO * 1_000_000;
            testing_env!(ctx.clone());
            ContractSaleComponent.buy_contract(None);
            let logs = test_utils::get_logs();
            println!("{:#?}", logs);
            assert_eq!(
                &logs[0],
                LOG_EVENT_CONTRACT_SOLD
                    .message("buyer=buyer1, price=1000000000000000000000000000000")
                    .as_str()
            );
            assert_eq!(ContractOwnershipComponent::owner(), BUYER_1.to_string());
            assert!(ContractSaleComponent::contract_sale_price().is_none());
            assert!(ContractSaleComponent::contract_bid().is_none());

            let receipts = deserialize_receipts();
            assert_eq!(&receipts[0].receiver_id, OWNER);
            match &receipts[0].actions[0] {
                Action::Transfer(transfer) => assert!(transfer.deposit > ctx.attached_deposit),
                _ => panic!("expected TransferAction"),
            }
        }

        #[test]
        fn with_lower_sale_price() {
            let mut ctx = arrange(Some(100.into()), None);

            ctx.predecessor_account_id = BUYER_1.to_string();
            ctx.attached_deposit = YOCTO * 1_000_000;
            testing_env!(ctx.clone());
            ContractSaleComponent.buy_contract(None);
            let logs = test_utils::get_logs();
            println!("{:#?}", logs);
            assert_eq!(
                &logs[0],
                LOG_EVENT_CONTRACT_SOLD
                    .message("buyer=buyer1, price=1000000000000000000000000000000")
                    .as_str()
            );
            assert_eq!(ContractOwnershipComponent::owner(), BUYER_1.to_string());
            assert!(ContractSaleComponent::contract_sale_price().is_none());
            assert!(ContractSaleComponent::contract_bid().is_none());

            let receipts = deserialize_receipts();
            assert_eq!(&receipts[0].receiver_id, OWNER);
            match &receipts[0].actions[0] {
                Action::Transfer(transfer) => assert!(transfer.deposit > ctx.attached_deposit),
                _ => panic!("expected TransferAction"),
            }
        }
    }

    #[cfg(test)]
    mod no_sale_with_bid {
        use super::*;
        use oysterpack_smart_near::domain::ExpirationDuration;

        #[test]
        #[should_panic(expected = "[ERR] [CONTRACT_BID_NOT_ATTACHED]")]
        fn higher_prior_bid() {
            let mut ctx = arrange(
                None,
                Some(ContractBuyerBid {
                    buyer: BUYER_2.to_string(),
                    bid: ContractBid {
                        amount: 1000.into(),
                        expiration: None,
                    },
                }),
            );

            ctx.predecessor_account_id = BUYER_1.to_string();
            ctx.attached_deposit = 999;
            testing_env!(ctx.clone());
            ContractSaleComponent.buy_contract(None);
        }

        #[test]
        fn higher_prior_expired_bid() {
            testing_env!(new_context(OWNER));
            let mut ctx = arrange(
                None,
                Some(ContractBuyerBid {
                    buyer: BUYER_2.to_string(),
                    bid: ContractBid {
                        amount: 1000.into(),
                        expiration: Some(
                            ExpirationSetting::Relative(ExpirationDuration::Epochs(10)).into(),
                        ),
                    },
                }),
            );

            let bid = ContractSaleComponent::contract_bid().unwrap();

            ctx.predecessor_account_id = BUYER_1.to_string();
            ctx.attached_deposit = 999;
            if let Some(Expiration::Epoch(epoch)) = bid.bid.expiration {
                ctx.epoch_height = epoch.value() + 1; // expires the bid
            }
            testing_env!(ctx.clone());
            ContractSaleComponent.buy_contract(None);
            let bid = ContractSaleComponent::contract_bid().unwrap();
            assert_eq!(bid.buyer, ctx.predecessor_account_id);
            assert_eq!(bid.bid.amount, ctx.attached_deposit.into());

            let logs = test_utils::get_logs();
            println!("{:#?}", logs);
            assert_eq!(
                logs,
                vec![
                    LOG_EVENT_CONTRACT_BID_CANCELLED.message("higher bid has been placed"),
                    LOG_EVENT_CONTRACT_BID_PLACED.message("bid: 999")
                ]
            );

            let receipts = deserialize_receipts();
            assert_eq!(&receipts[0].receiver_id, BUYER_2);
            match &receipts[0].actions[0] {
                Action::Transfer(transfer) => assert_eq!(transfer.deposit, 1000),
                _ => panic!("expected TransferAction"),
            }
        }

        #[test]
        #[should_panic(expected = "[ERR] [CONTRACT_BID_NOT_ATTACHED]")]
        fn matching_prior_bid() {
            let mut ctx = arrange(
                None,
                Some(ContractBuyerBid {
                    buyer: BUYER_2.to_string(),
                    bid: ContractBid {
                        amount: 1000.into(),
                        expiration: None,
                    },
                }),
            );

            ctx.predecessor_account_id = BUYER_1.to_string();
            ctx.attached_deposit = 999;
            testing_env!(ctx.clone());
            ContractSaleComponent.buy_contract(None);
        }

        #[test]
        fn lower_prior_bid() {
            let mut ctx = arrange(
                None,
                Some(ContractBuyerBid {
                    buyer: BUYER_2.to_string(),
                    bid: ContractBid {
                        amount: 1000.into(),
                        expiration: None,
                    },
                }),
            );

            ctx.predecessor_account_id = BUYER_1.to_string();
            ctx.attached_deposit = 1001;
            testing_env!(ctx.clone());
            ContractSaleComponent.buy_contract(None);
            let bid = ContractSaleComponent::contract_bid().unwrap();
            assert_eq!(bid.buyer, ctx.predecessor_account_id);
            assert_eq!(bid.bid.amount, ctx.attached_deposit.into());

            let logs = test_utils::get_logs();
            println!("{:#?}", logs);
            assert_eq!(
                logs,
                vec![
                    LOG_EVENT_CONTRACT_BID_CANCELLED.message("higher bid has been placed"),
                    LOG_EVENT_CONTRACT_BID_PLACED.message("bid: 1001")
                ]
            );

            let receipts = deserialize_receipts();
            assert_eq!(&receipts[0].receiver_id, BUYER_2);
            match &receipts[0].actions[0] {
                Action::Transfer(transfer) => assert_eq!(transfer.deposit, 1000),
                _ => panic!("expected TransferAction"),
            }
        }
    }
}

#[cfg(test)]
mod tests_cancel_contract_sale {
    use super::*;
    use near_sdk::test_utils;
    use oysterpack_smart_near::component::*;
    use oysterpack_smart_near::YOCTO;
    use oysterpack_smart_near_test::*;

    const OWNER: &str = "owner";

    #[test]
    fn cancel_prior_sale() {
        // Arrange
        let mut ctx = new_context(OWNER);
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(OWNER)));

        ContractSaleComponent.sell_contract(YOCTO.into());

        // Act
        ContractSaleComponent.cancel_contract_sale();

        // Assert
        assert!(ContractSaleComponent::contract_sale_price().is_none());
        let logs = test_utils::get_logs();
        println!("{:#?}", logs);
        assert_eq!(
            logs,
            vec![
                LOG_EVENT_CONTRACT_FOR_SALE.message(YOCTO),
                LOG_EVENT_CONTRACT_SALE_CANCELLED.message("")
            ]
        );
    }

    #[test]
    fn no_prior_sale() {
        // Arrange
        let mut ctx = new_context(OWNER);
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(OWNER)));

        // Act
        ContractSaleComponent.cancel_contract_sale();

        // Assert
        assert!(ContractSaleComponent::contract_sale_price().is_none());
        let logs = test_utils::get_logs();
        assert!(logs.is_empty());
    }
}

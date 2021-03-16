use crate::components::contract_ownership::ContractOwnershipComponent;
use crate::{ContractBid, ContractSale};
use crate::{
    ContractBuyerBid, ContractOwner, ContractOwnerObject, ContractOwnership,
    ContractOwnershipAccountIdsObject, ERR_ACCESS_DENIED_MUST_BE_BUYER, ERR_CONTRACT_BID_TOO_LOW,
    ERR_CONTRACT_SALE_NOT_ALLOWED, ERR_CONTRACT_SALE_PRICE_MUST_NOT_BE_ZERO,
    ERR_EXPIRATION_IS_ALREADY_EXPIRED, ERR_NO_ACTIVE_BID, ERR_OWNER_CANNOT_BUY_CONTRACT,
    LOG_EVENT_CONTRACT_BID_CANCELLED, LOG_EVENT_CONTRACT_BID_EXPIRATION_CHANGE,
    LOG_EVENT_CONTRACT_BID_LOWERED, LOG_EVENT_CONTRACT_BID_PLACED, LOG_EVENT_CONTRACT_BID_RAISED,
    LOG_EVENT_CONTRACT_FOR_SALE, LOG_EVENT_CONTRACT_SALE_CANCELLED, LOG_EVENT_CONTRACT_SOLD,
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
        }
        LOG_EVENT_CONTRACT_SALE_CANCELLED.log("");
    }

    fn buy_contract(&mut self, expiration: Option<ExpirationSetting>) {
        assert_near_attached("contract bid requires attached NEAR deposit");
        let expiration = expiration.map(|expiration| {
            let expiration: Expiration = expiration.into();
            ERR_EXPIRATION_IS_ALREADY_EXPIRED.assert(|| !expiration.expired());
            expiration
        });

        let mut account_ids = ContractOwnershipAccountIdsObject::load();
        ERR_OWNER_CANNOT_BUY_CONTRACT.assert(|| env::predecessor_account_id() != account_ids.owner);
        let mut owner = ContractOwnerObject::load();

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
            || "contract sale is not allowed because contract ownership transfer has been initiated",
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
}

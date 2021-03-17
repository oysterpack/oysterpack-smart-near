//! [`ContractOwnershipComponent`]
//! - deployment: [`ContractOwnershipComponent::deploy`]
//!   - config: `ValidAccountId` - owner account ID

use crate::components::contract_metrics::ContractMetricsComponent;
use crate::components::contract_sale::ContractSaleComponent;
use crate::{
    ContractMetrics, ContractOwnerNearBalance, ContractOwnerObject, ContractOwnership,
    ContractOwnershipAccountIdsObject, ERR_OWNER_BALANCE_OVERDRAW,
    LOG_EVENT_CONTRACT_SALE_CANCELLED, LOG_EVENT_CONTRACT_TRANSFER_CANCELLED,
    LOG_EVENT_CONTRACT_TRANSFER_FINALIZED, LOG_EVENT_CONTRACT_TRANSFER_INITIATED,
};
use near_sdk::json_types::ValidAccountId;
use near_sdk::{env, AccountId, Promise};
use oysterpack_smart_near::asserts::{
    assert_request, assert_yocto_near_attached, ERR_CODE_BAD_REQUEST,
};
use oysterpack_smart_near::component::Deploy;
use oysterpack_smart_near::domain::{AccountIdHash, YoctoNear, ZERO_NEAR};

pub struct ContractOwnershipComponent;

impl Deploy for ContractOwnershipComponent {
    /// owner account ID
    type Config = ValidAccountId;

    fn deploy(config: Option<Self::Config>) {
        let owner = config.expect("owner account ID is required");
        ContractOwnerObject::initialize_contract(owner);
    }
}

impl ContractOwnership for ContractOwnershipComponent {
    fn owner() -> AccountId {
        let account_ids = ContractOwnershipAccountIdsObject::load();
        account_ids.owner.clone()
    }

    fn transfer_ownership(&mut self, new_owner: ValidAccountId) {
        assert_yocto_near_attached();

        let mut owner = ContractOwnerObject::assert_owner_access();
        assert_request(
            || new_owner.as_ref() != env::predecessor_account_id().as_str(),
            || "you cannot transfer to yourself",
        );
        let new_owner_account_id_hash: AccountIdHash = new_owner.as_ref().as_str().into();
        let current_prospective_owner_account_id_hash =
            owner.prospective_owner_account_id_hash.as_ref().cloned();

        let mut update_prospective_owner = || {
            let mut account_ids = ContractOwnershipAccountIdsObject::load();
            account_ids.prospective_owner = Some(new_owner.as_ref().to_string());
            account_ids.save();

            owner.prospective_owner_account_id_hash = Some(new_owner_account_id_hash);
            if owner.sale_price.take().is_some() {
                LOG_EVENT_CONTRACT_SALE_CANCELLED
                    .log("contract ownership transfer is being initiated");
            }
            if owner.bid.is_some() {
                let mut account_ids = ContractOwnershipAccountIdsObject::load();
                ContractSaleComponent::cancel_bid(
                    &mut owner,
                    &mut account_ids,
                    "contract ownership transfer has been initiated",
                );
                account_ids.save();
            }

            LOG_EVENT_CONTRACT_TRANSFER_INITIATED.log(new_owner.as_ref());
            owner.save();
        };

        match current_prospective_owner_account_id_hash {
            None => update_prospective_owner(),
            Some(prospective_owner_account_id_hash) => {
                if prospective_owner_account_id_hash != new_owner_account_id_hash {
                    update_prospective_owner()
                }
            }
        }
    }

    fn cancel_ownership_transfer(&mut self) {
        assert_yocto_near_attached();

        let mut owner = ContractOwnerObject::assert_current_or_prospective_owner_access();
        if owner.prospective_owner_account_id_hash.take().is_some() {
            owner.save();

            let mut account_ids = ContractOwnershipAccountIdsObject::load();
            account_ids.prospective_owner.take();
            account_ids.save();

            LOG_EVENT_CONTRACT_TRANSFER_CANCELLED.log("");
        }
    }

    fn prospective_owner() -> Option<AccountId> {
        let account_ids = ContractOwnershipAccountIdsObject::load();
        account_ids.prospective_owner.as_ref().cloned()
    }

    fn finalize_ownership_transfer(&mut self) {
        assert_yocto_near_attached();

        let mut owner = ContractOwnerObject::assert_prospective_owner_access();
        let mut account_ids = ContractOwnershipAccountIdsObject::load();

        // finalize
        owner.account_id_hash = env::predecessor_account_id().into();
        owner.prospective_owner_account_id_hash.take();

        account_ids.owner = env::predecessor_account_id();
        account_ids.prospective_owner.take();

        owner.save();
        account_ids.save();

        LOG_EVENT_CONTRACT_TRANSFER_FINALIZED.log("");
    }

    fn withdraw_owner_balance(&mut self, amount: Option<YoctoNear>) -> ContractOwnerNearBalance {
        assert_yocto_near_attached();
        ContractOwnerObject::assert_owner_access();

        let mut owner_balance = Self::owner_balance();
        let amount = match amount {
            None => owner_balance.available,
            Some(amount) => {
                ERR_CODE_BAD_REQUEST
                    .assert(|| amount > ZERO_NEAR, || "withdraw amount cannot be zero");
                ERR_OWNER_BALANCE_OVERDRAW.assert(|| owner_balance.available >= amount);
                amount
            }
        };

        let account_ids = ContractOwnershipAccountIdsObject::load();
        Promise::new(account_ids.owner.clone()).transfer(amount.value() + 1);

        owner_balance.total -= amount + 1;
        owner_balance.available -= amount;
        owner_balance
    }

    fn owner_balance() -> ContractOwnerNearBalance {
        let near_balances = ContractMetricsComponent::near_balances();
        let storage_usage_costs = ContractMetricsComponent::storage_usage_costs();
        let available = near_balances
            .owner()
            .saturating_sub(storage_usage_costs.owner().value())
            .into();
        ContractOwnerNearBalance {
            total: near_balances.owner(),
            available,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oysterpack_smart_near::domain::ZERO_NEAR;
    use oysterpack_smart_near_test::*;

    #[test]
    fn basic_ownership_workflow() {
        // Arrange
        let alfio = "alfio";
        let bob = "bob";
        let mut ctx = new_context(alfio);
        testing_env!(ctx.clone());

        // Set alfio as owner at deployment
        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));
        // Assert
        assert_eq!(alfio, ContractOwnershipComponent::owner().as_str());
        assert!(ContractOwnershipComponent::prospective_owner().is_none());
        let owner_balance = ContractOwnershipComponent::owner_balance();
        println!("{:?}", owner_balance);

        // Act - initiate transfer
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        ContractOwnershipComponent.transfer_ownership(to_valid_account_id(bob));
        // Assert
        assert_eq!(
            bob,
            ContractOwnershipComponent::prospective_owner()
                .unwrap()
                .as_str()
        );
        let owner_balance = ContractOwnershipComponent::owner_balance();
        println!("{:?}", owner_balance);

        // Act - initiate same transfer again
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        ContractOwnershipComponent.transfer_ownership(to_valid_account_id(bob));
        // Assert - should have no effect
        assert_eq!(
            bob,
            ContractOwnershipComponent::prospective_owner()
                .unwrap()
                .as_str()
        );
        let owner_balance = ContractOwnershipComponent::owner_balance();
        println!("{:?}", owner_balance);

        // Act - cancel the transfer
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        ContractOwnershipComponent.cancel_ownership_transfer();
        // Assert
        assert_eq!(alfio, ContractOwnershipComponent::owner().as_str());
        assert!(ContractOwnershipComponent::prospective_owner().is_none());
        let owner_balance = ContractOwnershipComponent::owner_balance();
        println!("{:?}", owner_balance);

        // Act - withdraw all owner available balance
        // Act - cancel the transfer
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        let owner_balance = ContractOwnershipComponent.withdraw_owner_balance(None);
        println!("after withdrawal: {:?}", owner_balance);
        // Assert
        assert_eq!(owner_balance.available, ZERO_NEAR);
        assert_eq!(owner_balance, ContractOwnershipComponent::owner_balance());

        // Act - initiate transfer again
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        ContractOwnershipComponent.transfer_ownership(to_valid_account_id(bob));
        // Assert
        assert_eq!(
            bob,
            ContractOwnershipComponent::prospective_owner()
                .unwrap()
                .as_str()
        );
        let owner_balance = ContractOwnershipComponent::owner_balance();
        println!("{:?}", owner_balance);

        // Act - prospective owner cancels transfer
        ctx.predecessor_account_id = bob.to_string();
        testing_env!(ctx.clone());
        ContractOwnershipComponent.cancel_ownership_transfer();
        // Assert
        assert_eq!(alfio, ContractOwnershipComponent::owner().as_str());
        assert!(ContractOwnershipComponent::prospective_owner().is_none());
        let owner_balance = ContractOwnershipComponent::owner_balance();
        println!("{:?}", owner_balance);

        // Act - initiate transfer again
        ctx.predecessor_account_id = alfio.to_string();
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        ContractOwnershipComponent.transfer_ownership(to_valid_account_id(bob));
        // Assert
        assert_eq!(
            bob,
            ContractOwnershipComponent::prospective_owner()
                .unwrap()
                .as_str()
        );
        let owner_balance = ContractOwnershipComponent::owner_balance();
        println!("{:?}", owner_balance);

        // Act - finalize the transfer
        ctx.predecessor_account_id = bob.to_string();
        testing_env!(ctx.clone());
        ContractOwnershipComponent.finalize_ownership_transfer();
        // Assert
        // Assert
        assert_eq!(bob, ContractOwnershipComponent::owner().as_str());
        assert!(ContractOwnershipComponent::prospective_owner().is_none());
        let owner_balance = ContractOwnershipComponent::owner_balance();
        println!("{:?}", owner_balance);
    }
}

#[cfg(test)]
mod tests_transfer_ownership {
    use super::*;
    use crate::ContractSale;
    use near_sdk::test_utils;
    use oysterpack_smart_near::YOCTO;
    use oysterpack_smart_near_test::*;

    #[test]
    fn change_transfer_recipient() {
        // Arrange
        let alfio = "alfio";
        let mut ctx = new_context(alfio);
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        ContractOwnershipComponent.transfer_ownership(to_valid_account_id("bob"));

        // Act
        ContractOwnershipComponent.transfer_ownership(to_valid_account_id("alice"));
        // Assert
        assert_eq!(
            ContractOwnershipComponent::prospective_owner().unwrap(),
            "alice".to_string()
        );
    }

    #[test]
    fn while_contract_is_for_sale() {
        // Arrange
        let alfio = "alfio";
        let mut ctx = new_context(alfio);
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        ContractSaleComponent.sell_contract((1000 * YOCTO).into());
        assert!(ContractSaleComponent::contract_sale_price().is_some());

        // Act
        ContractOwnershipComponent.transfer_ownership(to_valid_account_id("bob"));
        assert!(ContractSaleComponent::contract_sale_price().is_none());
        assert_eq!(
            ContractOwnershipComponent::prospective_owner()
                .unwrap()
                .as_str(),
            "bob"
        );
        let logs = test_utils::get_logs();
        println!("{:#?}", logs);
        assert_eq!(
            &logs[0],
            "[INFO] [CONTRACT_FOR_SALE] 1000000000000000000000000000"
        );
        assert!(&logs[1].starts_with("[INFO] [CONTRACT_SALE_CANCELLED]"));
        assert_eq!(&logs[2], "[INFO] [CONTRACT_TRANSFER_INITIATED] bob");
    }

    #[test]
    fn while_contract_has_bid() {
        // Arrange
        let alfio = "alfio";
        let mut ctx = new_context(alfio);
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        ctx.attached_deposit = YOCTO;
        ctx.predecessor_account_id = "bob".to_string();
        testing_env!(ctx.clone());
        ContractSaleComponent.buy_contract(None);
        assert!(ContractSaleComponent::contract_bid().is_some());

        // Act
        ctx.attached_deposit = 1;
        ctx.predecessor_account_id = alfio.to_string();
        testing_env!(ctx.clone());
        ContractOwnershipComponent.transfer_ownership(to_valid_account_id("bob"));
        assert!(ContractSaleComponent::contract_bid().is_none());
        assert_eq!(
            ContractOwnershipComponent::prospective_owner()
                .unwrap()
                .as_str(),
            "bob"
        );
        let receipts = deserialize_receipts();
        assert_eq!("bob", &receipts[0].receiver_id);
        let action = &receipts[0].actions[0];
        match action {
            Action::Transfer(transfer) => {
                assert_eq!(YOCTO, transfer.deposit);
            }
            _ => panic!("expected TransferAction"),
        }
    }

    #[test]
    fn while_contract_for_sale_with_bid() {
        // Arrange
        let alfio = "alfio";
        let mut ctx = new_context(alfio);
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        ContractSaleComponent.sell_contract((1000 * YOCTO).into());
        assert!(ContractSaleComponent::contract_sale_price().is_some());

        ctx.attached_deposit = YOCTO;
        ctx.predecessor_account_id = "bob".to_string();
        testing_env!(ctx.clone());
        ContractSaleComponent.buy_contract(None);
        assert!(ContractSaleComponent::contract_bid().is_some());

        // Act
        ctx.attached_deposit = 1;
        ctx.predecessor_account_id = alfio.to_string();
        testing_env!(ctx.clone());
        ContractOwnershipComponent.transfer_ownership(to_valid_account_id("bob"));
        assert!(ContractSaleComponent::contract_bid().is_none());
        assert!(ContractSaleComponent::contract_sale_price().is_none());
        assert_eq!(
            ContractOwnershipComponent::prospective_owner()
                .unwrap()
                .as_str(),
            "bob"
        );
        let receipts = deserialize_receipts();
        assert_eq!("bob", &receipts[0].receiver_id);
        let action = &receipts[0].actions[0];
        match action {
            Action::Transfer(transfer) => {
                assert_eq!(YOCTO, transfer.deposit);
            }
            _ => panic!("expected TransferAction"),
        }
    }

    #[test]
    #[should_panic(expected = "[ERR] [OWNER_ACCESS_REQUIRED]")]
    fn not_owner() {
        // Arrange
        let alfio = "alfio";
        let mut ctx = new_context(alfio);
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        // Act
        ctx.attached_deposit = 1;
        ctx.predecessor_account_id = "bob".to_string();
        testing_env!(ctx.clone());
        ContractOwnershipComponent.transfer_ownership(to_valid_account_id(alfio));
    }

    #[test]
    #[should_panic(expected = "[ERR] [BAD_REQUEST]")]
    fn transfer_to_self_owner() {
        // Arrange
        let alfio = "alfio";
        let mut ctx = new_context(alfio);
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        // Act
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        ContractOwnershipComponent.transfer_ownership(to_valid_account_id(alfio));
    }

    #[test]
    #[should_panic(expected = "[ERR] [YOCTONEAR_DEPOSIT_REQUIRED]")]
    fn zero_deposit_attached() {
        // Arrange
        let alfio = "alfio";
        let mut ctx = new_context(alfio);
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        // Act
        ctx.attached_deposit = 0;
        testing_env!(ctx.clone());
        ContractOwnershipComponent.transfer_ownership(to_valid_account_id("bob"));
    }

    #[test]
    #[should_panic(expected = "[ERR] [YOCTONEAR_DEPOSIT_REQUIRED]")]
    fn two_deposit_attached() {
        // Arrange
        let alfio = "alfio";
        let mut ctx = new_context(alfio);
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        // Act
        ctx.attached_deposit = 2;
        testing_env!(ctx.clone());
        ContractOwnershipComponent.transfer_ownership(to_valid_account_id("bob"));
    }
}

#[cfg(test)]
mod tests_finalize_transfer_ownership {
    use super::*;
    use near_sdk::test_utils;
    use oysterpack_smart_near_test::*;

    #[test]
    fn finalize_transfer() {
        // Arrange
        let alfio = "alfio";
        let bob = "bob";
        let mut ctx = new_context(alfio);
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        ContractOwnershipComponent.transfer_ownership(to_valid_account_id(bob));

        // Act
        ctx.predecessor_account_id = bob.to_string();
        testing_env!(ctx.clone());
        ContractOwnershipComponent.finalize_ownership_transfer();

        // Assert
        assert_eq!(ContractOwnershipComponent::owner().as_str(), bob);
        let logs = test_utils::get_logs();
        println!("{:#?}", logs);
        assert!(&logs[0].starts_with("[INFO] [CONTRACT_TRANSFER_FINALIZED]"));
    }

    #[test]
    #[should_panic(expected = "[ERR] [PROSPECTIVE_OWNER_ACCESS_REQUIRED]")]
    fn not_prospective_owner() {
        // Arrange
        let alfio = "alfio";
        let bob = "bob";
        let mut ctx = new_context(alfio);
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        ContractOwnershipComponent.transfer_ownership(to_valid_account_id(bob));

        // Act
        ctx.predecessor_account_id = "alice".to_string();
        testing_env!(ctx.clone());
        ContractOwnershipComponent.finalize_ownership_transfer();
    }

    #[test]
    #[should_panic(expected = "[ERR] [CONTRACT_OWNER_TRANSFER_NOT_INITIATED]")]
    fn no_transfer_in_progress() {
        // Arrange
        let alfio = "alfio";
        let mut ctx = new_context(alfio);
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        // Act
        ctx.attached_deposit = 1;
        ctx.predecessor_account_id = "alice".to_string();
        testing_env!(ctx.clone());
        ContractOwnershipComponent.finalize_ownership_transfer();
    }

    #[test]
    #[should_panic(expected = "[ERR] [YOCTONEAR_DEPOSIT_REQUIRED]")]
    fn zero_deposit_attached() {
        // Arrange
        let alfio = "alfio";
        let bob = "bob";
        let mut ctx = new_context(alfio);
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        ContractOwnershipComponent.transfer_ownership(to_valid_account_id(bob));

        // Act
        ctx.attached_deposit = 0;
        ctx.predecessor_account_id = bob.to_string();
        testing_env!(ctx.clone());
        ContractOwnershipComponent.finalize_ownership_transfer();
    }

    #[test]
    #[should_panic(expected = "[ERR] [YOCTONEAR_DEPOSIT_REQUIRED]")]
    fn two_yoctonear_deposit_attached() {
        // Arrange
        let alfio = "alfio";
        let bob = "bob";
        let mut ctx = new_context(alfio);
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        ContractOwnershipComponent.transfer_ownership(to_valid_account_id(bob));

        // Act
        ctx.attached_deposit = 2;
        ctx.predecessor_account_id = bob.to_string();
        testing_env!(ctx.clone());
        ContractOwnershipComponent.finalize_ownership_transfer();
    }
}

#[cfg(test)]
mod tests_cancel_ownership_transfer {
    use super::*;
    use near_sdk::test_utils;
    use oysterpack_smart_near_test::*;

    #[test]
    fn cancelled_by_owner() {
        // Arrange
        let alfio = "alfio";
        let bob = "bob";
        let mut ctx = new_context(alfio);
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        ContractOwnershipComponent.transfer_ownership(to_valid_account_id(bob));

        // Act
        ContractOwnershipComponent.cancel_ownership_transfer();
        // Assert
        assert!(ContractOwnershipComponent::prospective_owner().is_none());
        assert!(ContractOwnershipAccountIdsObject::load()
            .prospective_owner
            .is_none());
        let logs = test_utils::get_logs();
        assert_eq!(&logs[0], "[INFO] [CONTRACT_TRANSFER_INITIATED] bob");
        assert_eq!(&logs[1], "[INFO] [CONTRACT_TRANSFER_CANCELLED] ")
    }

    #[test]
    fn cancelled_by_prospective_owner() {
        // Arrange
        let alfio = "alfio";
        let bob = "bob";
        let mut ctx = new_context(alfio);
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        ContractOwnershipComponent.transfer_ownership(to_valid_account_id(bob));

        // Act
        ctx.attached_deposit = 1;
        ctx.predecessor_account_id = bob.to_string();
        testing_env!(ctx.clone());
        ContractOwnershipComponent.cancel_ownership_transfer();
        // Assert
        assert!(ContractOwnershipComponent::prospective_owner().is_none());
        assert!(ContractOwnershipAccountIdsObject::load()
            .prospective_owner
            .is_none());
        let logs = test_utils::get_logs();
        assert_eq!(&logs[0], "[INFO] [CONTRACT_TRANSFER_CANCELLED] ")
    }

    #[test]
    fn cancelled_by_owner_with_no_transfer_initiated() {
        // Arrange
        let alfio = "alfio";
        let mut ctx = new_context(alfio);
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        // Act
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        ContractOwnershipComponent.cancel_ownership_transfer();
        // Assert
        let logs = test_utils::get_logs();
        assert!(logs.is_empty());
    }

    #[test]
    #[should_panic(expected = "[ERR] [CURRENT_OR_PROSPECTIVE_OWNER_ACCESS_REQUIRED]")]
    fn cancelled_by_non_owner_with_no_transfer_initiated() {
        // Arrange
        let alfio = "alfio";
        let mut ctx = new_context(alfio);
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        // Act
        ctx.attached_deposit = 1;
        ctx.predecessor_account_id = "bob".to_string();
        testing_env!(ctx.clone());
        ContractOwnershipComponent.cancel_ownership_transfer();
        // Assert
        let logs = test_utils::get_logs();
        assert!(logs.is_empty());
    }

    #[test]
    #[should_panic(expected = "[ERR] [YOCTONEAR_DEPOSIT_REQUIRED]")]
    fn cancelled_by_owner_with_zero_deposit() {
        // Arrange
        let alfio = "alfio";
        let bob = "bob";
        let mut ctx = new_context(alfio);
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        ContractOwnershipComponent.transfer_ownership(to_valid_account_id(bob));

        // Act
        ctx.attached_deposit = 0;
        testing_env!(ctx.clone());
        ContractOwnershipComponent.cancel_ownership_transfer();
        // Assert
        assert!(ContractOwnershipComponent::prospective_owner().is_none());
        assert!(ContractOwnershipAccountIdsObject::load()
            .prospective_owner
            .is_none());
        let logs = test_utils::get_logs();
        assert_eq!(&logs[0], "[INFO] [CONTRACT_TRANSFER_CANCELLED] ")
    }

    #[test]
    #[should_panic(expected = "[ERR] [YOCTONEAR_DEPOSIT_REQUIRED]")]
    fn cancelled_by_owner_with_1_deposit() {
        // Arrange
        let alfio = "alfio";
        let bob = "bob";
        let mut ctx = new_context(alfio);
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        ContractOwnershipComponent.transfer_ownership(to_valid_account_id(bob));

        // Act
        ctx.attached_deposit = 2;
        testing_env!(ctx.clone());
        ContractOwnershipComponent.cancel_ownership_transfer();
        // Assert
        assert!(ContractOwnershipComponent::prospective_owner().is_none());
        assert!(ContractOwnershipAccountIdsObject::load()
            .prospective_owner
            .is_none());
        let logs = test_utils::get_logs();
        assert_eq!(&logs[0], "[INFO] [CONTRACT_TRANSFER_CANCELLED] ")
    }

    #[test]
    #[should_panic(expected = "[ERR] [YOCTONEAR_DEPOSIT_REQUIRED]")]
    fn cancelled_by_prospective_owner_with_zero_deposit() {
        // Arrange
        let alfio = "alfio";
        let bob = "bob";
        let mut ctx = new_context(alfio);
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        ContractOwnershipComponent.transfer_ownership(to_valid_account_id(bob));

        // Act
        ctx.attached_deposit = 0;
        ctx.predecessor_account_id = bob.to_string();
        testing_env!(ctx.clone());
        ContractOwnershipComponent.cancel_ownership_transfer();
        // Assert
        assert!(ContractOwnershipComponent::prospective_owner().is_none());
        assert!(ContractOwnershipAccountIdsObject::load()
            .prospective_owner
            .is_none());
        let logs = test_utils::get_logs();
        assert_eq!(&logs[0], "[INFO] [CONTRACT_TRANSFER_CANCELLED] ")
    }

    #[test]
    #[should_panic(expected = "[ERR] [YOCTONEAR_DEPOSIT_REQUIRED]")]
    fn cancelled_by_prospective_owner_with_2_deposit() {
        // Arrange
        let alfio = "alfio";
        let bob = "bob";
        let mut ctx = new_context(alfio);
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        ContractOwnershipComponent.transfer_ownership(to_valid_account_id(bob));

        // Act
        ctx.attached_deposit = 2;
        ctx.predecessor_account_id = bob.to_string();
        testing_env!(ctx.clone());
        ContractOwnershipComponent.cancel_ownership_transfer();
        // Assert
        assert!(ContractOwnershipComponent::prospective_owner().is_none());
        assert!(ContractOwnershipAccountIdsObject::load()
            .prospective_owner
            .is_none());
        let logs = test_utils::get_logs();
        assert_eq!(&logs[0], "[INFO] [CONTRACT_TRANSFER_CANCELLED] ")
    }
}

#[cfg(test)]
mod owner_balance {
    use super::*;
    use oysterpack_smart_near::domain::ZERO_NEAR;
    use oysterpack_smart_near_test::*;

    #[test]
    fn withdraw_all_available_balance() {
        // Arrange
        let alfio = "alfio";
        let mut ctx = new_context(alfio);
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        let owner_balance_1 = ContractOwnershipComponent::owner_balance();
        assert!(owner_balance_1.available > ZERO_NEAR);
        // Act
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        let owner_balance_2 = ContractOwnershipComponent.withdraw_owner_balance(None);
        assert_eq!(owner_balance_2.available, ZERO_NEAR);
        let receipts = deserialize_receipts();
        assert_eq!(
            &receipts[0].receiver_id,
            ContractOwnershipComponent::owner().as_str()
        );
        match &receipts[0].actions[0] {
            Action::Transfer(transfer) => {
                // available balance is bit higher than the initial balance because of transaction rewards
                assert!(transfer.deposit > owner_balance_1.available.value() + 1);
            }
            _ => panic!("expected TransferAction"),
        }
    }

    #[test]
    fn withdraw_partial_available_balance() {
        // Arrange
        let alfio = "alfio";
        let mut ctx = new_context(alfio);
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        // Act
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        let initial_balance = ContractOwnershipComponent::owner_balance();
        let amount = initial_balance.available.value() / 2;
        let owner_balance = ContractOwnershipComponent.withdraw_owner_balance(Some(amount.into()));
        assert!(owner_balance.available < initial_balance.available);
        let receipts = deserialize_receipts();
        assert_eq!(
            &receipts[0].receiver_id,
            ContractOwnershipComponent::owner().as_str()
        );
        match &receipts[0].actions[0] {
            Action::Transfer(transfer) => {
                // available balance is bit higher than the initial balance because of transaction rewards
                assert_eq!(transfer.deposit, amount + 1);
            }
            _ => panic!("expected TransferAction"),
        }
    }

    #[test]
    #[should_panic(expected = "[ERR] [OWNER_BALANCE_OVERDRAW]")]
    fn over_withdraw_partial_available_balance() {
        // Arrange
        let alfio = "alfio";
        let mut ctx = new_context(alfio);
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        // Act
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        let initial_balance = ContractOwnershipComponent::owner_balance();
        let amount = initial_balance.available.value() + 1;
        ContractOwnershipComponent.withdraw_owner_balance(Some(amount.into()));
    }

    #[test]
    #[should_panic(expected = "[ERR] [BAD_REQUEST] withdraw amount cannot be zero")]
    fn zero_withdraw_partial_available_balance() {
        // Arrange
        let alfio = "alfio";
        let mut ctx = new_context(alfio);
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        // Act
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        ContractOwnershipComponent.withdraw_owner_balance(Some(ZERO_NEAR));
    }

    #[test]
    #[should_panic(expected = "[ERR] [OWNER_ACCESS_REQUIRED]")]
    fn withdraw_partial_available_balance_as_non_owner() {
        // Arrange
        let alfio = "alfio";
        let mut ctx = new_context(alfio);
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        // Act
        ctx.attached_deposit = 1;
        ctx.predecessor_account_id = "bob".to_string();
        testing_env!(ctx.clone());
        ContractOwnershipComponent.withdraw_owner_balance(Some(100.into()));
    }

    #[test]
    #[should_panic(expected = "[ERR] [OWNER_ACCESS_REQUIRED]")]
    fn withdraw_all_available_balance_as_non_owner() {
        // Arrange
        let alfio = "alfio";
        let mut ctx = new_context(alfio);
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        // Act
        ctx.attached_deposit = 1;
        ctx.predecessor_account_id = "bob".to_string();
        testing_env!(ctx.clone());
        ContractOwnershipComponent.withdraw_owner_balance(None);
    }

    #[test]
    #[should_panic(
        expected = "[ERR] [YOCTONEAR_DEPOSIT_REQUIRED] exactly 1 yoctoNEAR must be attached"
    )]
    fn withdraw_all_available_balance_zero_deposit_attached() {
        // Arrange
        let alfio = "alfio";
        let ctx = new_context(alfio);
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        // Act
        testing_env!(ctx.clone());
        ContractOwnershipComponent.withdraw_owner_balance(None);
    }

    #[test]
    #[should_panic(
        expected = "[ERR] [YOCTONEAR_DEPOSIT_REQUIRED] exactly 1 yoctoNEAR must be attached"
    )]
    fn withdraw_all_available_balance_2_deposit_attached() {
        // Arrange
        let alfio = "alfio";
        let mut ctx = new_context(alfio);
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        // Act
        ctx.attached_deposit = 2;
        testing_env!(ctx.clone());
        ContractOwnershipComponent.withdraw_owner_balance(None);
    }

    #[test]
    #[should_panic(
        expected = "[ERR] [YOCTONEAR_DEPOSIT_REQUIRED] exactly 1 yoctoNEAR must be attached"
    )]
    fn withdraw_partial_available_balance_zero_deposit_attached() {
        // Arrange
        let alfio = "alfio";
        let ctx = new_context(alfio);
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        // Act
        testing_env!(ctx.clone());
        ContractOwnershipComponent.withdraw_owner_balance(Some(100.into()));
    }

    #[test]
    #[should_panic(
        expected = "[ERR] [YOCTONEAR_DEPOSIT_REQUIRED] exactly 1 yoctoNEAR must be attached"
    )]
    fn withdraw_partial_available_balance_2_deposit_attached() {
        // Arrange
        let alfio = "alfio";
        let mut ctx = new_context(alfio);
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(Some(to_valid_account_id(alfio)));

        // Act
        ctx.attached_deposit = 2;
        testing_env!(ctx.clone());
        ContractOwnershipComponent.withdraw_owner_balance(Some(100.into()));
    }
}

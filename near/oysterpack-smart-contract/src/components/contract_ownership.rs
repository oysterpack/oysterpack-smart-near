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
use oysterpack_smart_near::asserts::assert_yocto_near_attached;
use oysterpack_smart_near::component::Deploy;
use oysterpack_smart_near::domain::{AccountIdHash, YoctoNear};

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
                ContractSaleComponent::cancel_bid(&mut owner, &mut account_ids);
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
                ERR_OWNER_BALANCE_OVERDRAW.assert(|| owner_balance.available >= amount);
                amount
            }
        };

        let account_ids = ContractOwnershipAccountIdsObject::load();
        Promise::new(account_ids.owner.clone()).transfer(amount.value());

        owner_balance.total -= amount;
        owner_balance.available -= amount;
        owner_balance
    }

    fn owner_balance() -> ContractOwnerNearBalance {
        let near_balances = ContractMetricsComponent::near_balances();
        let storage_usage_costs = ContractMetricsComponent::storage_usage_costs();
        ContractOwnerNearBalance {
            total: near_balances.owner(),
            available: near_balances.owner() - storage_usage_costs.owner(),
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
    use oysterpack_smart_near::YOCTO;
    use oysterpack_smart_near_test::*;

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
}

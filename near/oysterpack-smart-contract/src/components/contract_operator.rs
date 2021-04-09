use crate::contract::contract_operator::CONTRACT_LOCKED_STORAGE_BALANCE_ID;
use crate::interface::contract::contract_operator::ContractOperator;
use crate::ContractNearBalances;
use oysterpack_smart_account_management::components::account_management::AccountManagementComponent;
use oysterpack_smart_account_management::AccountRepository;
use oysterpack_smart_near::{
    domain::StorageUsage,
    near_sdk::{
        borsh::{BorshDeserialize, BorshSerialize},
        env,
    },
};
use std::fmt::Debug;

pub struct ContractOperatorComponent<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    account_manager: AccountManagementComponent<T>,
}

impl<T> ContractOperatorComponent<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    pub fn new(account_manager: AccountManagementComponent<T>) -> Self {
        Self { account_manager }
    }
}

impl<T> ContractOperator for ContractOperatorComponent<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    fn ops_operator_lock_storage_balance(&mut self, storage_usage: StorageUsage) {
        self.account_manager.assert_operator();
        let storage_use_cost = env::storage_byte_cost() * *storage_usage as u128;
        ContractNearBalances::set_balance(
            CONTRACT_LOCKED_STORAGE_BALANCE_ID,
            storage_use_cost.into(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::contract_metrics::ContractMetricsComponent;
    use crate::ContractMetrics;
    use oysterpack_smart_account_management::components::account_management::AccountManagementComponentConfig;
    use oysterpack_smart_account_management::{ContractPermissions, StorageManagement};
    use oysterpack_smart_near::component::Deploy;
    use oysterpack_smart_near::YOCTO;
    use oysterpack_smart_near_test::*;

    type AccountManager = AccountManagementComponent<()>;

    #[test]
    fn lock_storage_balance() {
        // Arrange
        let operator = "bob";
        let ctx = new_context(operator);
        testing_env!(ctx.clone());

        AccountManager::deploy(AccountManagementComponentConfig {
            storage_usage_bounds: None,
            component_account_storage_mins: None,
            admin_account: to_valid_account_id(operator),
        });

        let mut operator =
            ContractOperatorComponent::new(AccountManager::new(&ContractPermissions::default()));

        // act
        testing_env!(ctx.clone());
        operator.ops_operator_lock_storage_balance(1024.into());

        let contract_balances = ContractMetricsComponent
            .ops_metrics_near_balances()
            .balances()
            .unwrap();
        assert_eq!(
            **contract_balances
                .get(&CONTRACT_LOCKED_STORAGE_BALANCE_ID)
                .unwrap(),
            1024 * env::storage_byte_cost()
        );

        operator.ops_operator_lock_storage_balance(0.into());
        let contract_balances = ContractMetricsComponent
            .ops_metrics_near_balances()
            .balances();
        assert!(contract_balances.is_none());
    }

    #[test]
    #[should_panic(expected = "[ERR] [ACCOUNT_NOT_REGISTERED]")]
    fn with_unregistered_account() {
        // Arrange
        let operator = "bob";
        let mut ctx = new_context(operator);
        testing_env!(ctx.clone());

        AccountManager::deploy(AccountManagementComponentConfig {
            storage_usage_bounds: None,
            component_account_storage_mins: None,
            admin_account: to_valid_account_id(operator),
        });

        let mut operator =
            ContractOperatorComponent::new(AccountManager::new(&ContractPermissions::default()));

        // act
        ctx.predecessor_account_id = "not_registered".to_string();
        testing_env!(ctx.clone());
        operator.ops_operator_lock_storage_balance(1024.into());
    }

    #[test]
    #[should_panic(expected = "[ERR] [NOT_AUTHORIZED]")]
    fn with_not_operator() {
        // Arrange
        let account = "bob";
        let mut ctx = new_context(account);
        testing_env!(ctx.clone());

        AccountManager::deploy(AccountManagementComponentConfig {
            storage_usage_bounds: None,
            component_account_storage_mins: None,
            admin_account: to_valid_account_id("owner"),
        });

        let mut operator =
            ContractOperatorComponent::new(AccountManager::new(&ContractPermissions::default()));

        {
            let mut ctx = ctx.clone();
            ctx.attached_deposit = YOCTO;
            testing_env!(ctx);
            operator.account_manager.storage_deposit(None, Some(true));
        }

        // act
        ctx.predecessor_account_id = account.to_string();
        testing_env!(ctx.clone());
        operator.ops_operator_lock_storage_balance(1024.into());
    }
}

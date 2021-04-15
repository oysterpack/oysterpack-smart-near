use crate::*;
use oysterpack_smart_account_management::components::account_management::AccountManagementComponent;
use oysterpack_smart_account_management::ContractPermissions;
use oysterpack_smart_contract::components::contract_operator::ContractOperatorComponent;
use oysterpack_smart_staking_pool::components::staking_pool::StakingPoolComponent;
use oysterpack_smart_staking_pool::{StakeAccountData, PERMISSION_TREASURER};
use std::collections::HashMap;

pub type AccountData = StakeAccountData;

pub type AccountManager = AccountManagementComponent<AccountData>;

pub type StakeFungibleToken = FungibleTokenComponent<AccountData>;

pub type ContractOperator = ContractOperatorComponent<AccountData>;

impl Contract {
    pub(crate) fn account_manager() -> AccountManager {
        StakeFungibleToken::register_storage_management_event_handler();

        let contract_permissions = {
            let mut permissions = HashMap::with_capacity(1);
            permissions.insert(0, PERMISSION_TREASURER);
            ContractPermissions(permissions)
        };

        AccountManager::new(contract_permissions)
    }

    pub(crate) fn ft_stake() -> StakeFungibleToken {
        StakeFungibleToken::new(Self::account_manager())
    }

    pub(crate) fn contract_operator() -> ContractOperator {
        ContractOperator::new(Self::account_manager())
    }

    pub(crate) fn staking_pool() -> StakingPoolComponent {
        StakingPoolComponent::new(Self::account_manager(), Self::ft_stake())
    }
}

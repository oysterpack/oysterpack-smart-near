use crate::*;
use oysterpack_smart_account_management::components::account_management::AccountManagementComponent;
use oysterpack_smart_contract::components::contract_operator::ContractOperatorComponent;
use oysterpack_smart_staking_pool::components::staking_pool::{
    StakeAccountData, StakingPoolComponent,
};

pub type AccountData = StakeAccountData;

pub type AccountManager = AccountManagementComponent<AccountData>;

pub type StakeFungibleToken = FungibleTokenComponent<AccountData>;

pub type ContractOperator = ContractOperatorComponent<AccountData>;

impl Contract {
    pub(crate) fn account_manager() -> AccountManager {
        StakeFungibleToken::register_storage_management_event_handler();
        AccountManager::default()
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

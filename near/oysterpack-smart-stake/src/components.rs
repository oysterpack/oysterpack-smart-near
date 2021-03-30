use crate::*;
use oysterpack_smart_account_management::components::account_management::AccountManagementComponent;

pub type AccountData = ();

pub type AccountManager = AccountManagementComponent<AccountData>;

pub type StakeFungibleToken = FungibleTokenComponent<AccountData>;

impl Contract {
    pub fn account_manager() -> AccountManager {
        StakeFungibleToken::register_storage_management_event_handler();
        AccountManager::default()
    }

    pub fn ft_stake() -> StakeFungibleToken {
        StakeFungibleToken::new(Self::account_manager())
    }
}

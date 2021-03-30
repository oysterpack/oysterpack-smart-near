use crate::*;
use oysterpack_smart_account_management::components::account_management::{
    AccountManagementComponent, ContractPermissions,
};
use teloc::*;

pub type AccountData = ();

pub type AccountManager = AccountManagementComponent<AccountData>;

pub type StakeFungibleToken = FungibleTokenComponent<AccountData>;

impl Contract {
    pub fn account_manager() -> AccountManager {
        StakeFungibleToken::register_storage_management_event_handler();

        let container = ServiceProvider::new()
            .add_instance(ContractPermissions::default())
            .add_transient::<AccountManager>();
        container.resolve()
    }

    pub fn ft_stake() -> StakeFungibleToken {
        let container = ServiceProvider::new()
            .add_instance(ContractPermissions::default())
            .add_transient::<AccountManager>()
            .add_transient::<StakeFungibleToken>();
        container.resolve()
    }
}

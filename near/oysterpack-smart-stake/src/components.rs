use crate::*;
use oysterpack_smart_account_management::components::account_management::{
    AccountManagementComponent, ContractPermissions, UnregisterAccount,
};
use oysterpack_smart_fungible_token::components::fungible_token::FungibleTokenUnregisterAccountHandler;
use teloc::*;

pub type AccountData = ();

pub type AccountManager = AccountManagementComponent<AccountData>;

pub type StakeFungibleToken = FungibleTokenComponent<AccountData>;

impl Contract {
    pub fn account_manager() -> AccountManager {
        let container = ServiceProvider::new()
            .add_transient_c::<Box<dyn UnregisterAccount>, Box<UnregisterHandler>>()
            .add_instance(ContractPermissions::default())
            .add_transient::<AccountManager>();

        container.resolve()
    }

    pub fn ft_stake() -> StakeFungibleToken {
        let container = ServiceProvider::new()
            .add_transient_c::<Box<dyn UnregisterAccount>, Box<UnregisterHandler>>()
            .add_instance(ContractPermissions::default())
            .add_transient::<AccountManager>()
            .add_transient::<StakeFungibleToken>();

        container.resolve()
    }
}

#[derive(Dependency)]
struct UnregisterHandler;

impl UnregisterAccount for UnregisterHandler {
    fn unregister_account(&self, force: bool) {
        FungibleTokenUnregisterAccountHandler.unregister_account(force);
    }
}

impl From<Box<UnregisterHandler>> for Box<dyn UnregisterAccount> {
    fn from(x: Box<UnregisterHandler>) -> Self {
        x
    }
}

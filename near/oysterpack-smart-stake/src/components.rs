use crate::Contract;
use oysterpack_smart_account_management::components::account_management::{
    AccountManagementComponent, UnregisterAccount,
};
use teloc::*;

pub type AccountData = ();

pub type AccountManager = AccountManagementComponent<AccountData>;

impl Contract {
    pub fn account_manager() -> AccountManager {
        let container = ServiceProvider::new()
            .add_transient_c::<Box<dyn UnregisterAccount>, Box<UnregisterMock>>()
            .add_transient::<AccountManager>();

        container.resolve()
    }
}

#[derive(Dependency)]
struct UnregisterMock;

impl UnregisterAccount for UnregisterMock {
    fn unregister_account(&mut self, _force: bool) {}
}

impl From<Box<UnregisterMock>> for Box<dyn UnregisterAccount> {
    fn from(x: Box<UnregisterMock>) -> Self {
        x
    }
}

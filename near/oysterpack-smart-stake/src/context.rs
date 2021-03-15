use teloc::*;

use oysterpack_smart_account_management::components::account_management::*;
use oysterpack_smart_near::contract_context::SmartContractContext;

pub type AccountData = ();

pub type AccountManager = AccountManagementComponent<AccountData>;

#[derive(Default)]
pub struct Context {
    pub account_management: AccountManager,
}

impl SmartContractContext for Context {
    type Config = ();

    fn build(_config: Self::Config) -> Self {
        let container = ServiceProvider::new()
            .add_transient_c::<Box<dyn UnregisterAccount>, Box<UnregisterMock>>()
            .add_transient::<AccountManager>();

        Self {
            account_management: container.resolve(),
        }
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

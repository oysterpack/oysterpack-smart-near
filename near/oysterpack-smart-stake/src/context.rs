use near_sdk::env;
use teloc::*;

use oysterpack_smart_account_management::{components::account_management::*, StorageUsageBounds};
use oysterpack_smart_near::{component::*, contract_context::SmartContractContext};

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

    fn deploy(_context: &mut Self) {
        assert!(!env::state_exists(), "contract is already initialized");
        AccountManagementComponent::<AccountData>::deploy(Some(StorageUsageBounds {
            min: 1000.into(),
            max: None,
        }));
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

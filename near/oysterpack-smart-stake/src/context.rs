use near_sdk::env;
use oysterpack_smart_account_management::{
    components::{account_management::*, account_storage_usage::*},
    AccountStats, StorageUsageBounds,
};
use oysterpack_smart_near::{contract_context::SmartContractContext, eventbus, service::*};
use teloc::*;

#[derive(Default)]
pub struct Context {
    pub account_management: AccountManagementComponent<()>,
}

impl SmartContractContext for Context {
    type Config = ();

    fn build(_config: Self::Config) -> Self {
        eventbus::register(AccountStats::on_account_storage_event);
        Self {
            account_management: create_account_management_component(),
        }
    }

    fn deploy(_context: &mut Self) {
        assert!(!env::state_exists(), "contract is already initialized");
        AccountStorageUsageComponent::deploy(Some(StorageUsageBounds {
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

fn create_account_management_component() -> AccountManagementComponent<()> {
    let container = ServiceProvider::new()
        .add_transient_c::<Box<dyn UnregisterAccount>, Box<UnregisterMock>>()
        .add_transient::<AccountManagementComponent<()>>();
    container.resolve()
}

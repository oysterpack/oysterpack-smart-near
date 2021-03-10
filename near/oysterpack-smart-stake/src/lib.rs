use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env,
    json_types::U128,
    near_bindgen, wee_alloc, PanicOnDefault,
};
use oysterpack_smart_account_management::components::account_management::{
    AccountManagementComponent, UnregisterAccount,
};
use oysterpack_smart_account_management::components::account_storage_usage::AccountStorageUsageComponent;
use oysterpack_smart_account_management::{
    Account, AccountStats, AccountStorageEvent, AccountStorageUsage, StorageBalance,
    StorageBalanceBounds, StorageUsageBounds,
};
use oysterpack_smart_near::{eventbus, service::*};
use teloc::*;

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
#[borsh_init(init)]
pub struct Contract {
    #[borsh_skip]
    account_management: AccountManagementComponent<()>,
}

#[near_bindgen]
impl Contract {
    #[init]
    pub fn init_state() -> Self {
        assert!(!env::state_exists(), "contract is already initialized");
        AccountStorageUsageComponent::<()>::deploy(Some(StorageUsageBounds {
            min: 1000.into(),
            max: None,
        }));
        Self {
            account_management: create_account_management_component(),
        }
    }

    pub fn simulate_account_storage_event(&self) {
        eventbus::post(&AccountStorageEvent::Registered(
            StorageBalance {
                total: 100.into(),
                available: 0.into(),
            },
            1000.into(),
        ));
    }

    pub fn storage_usage_bounds(&self) -> StorageUsageBounds {
        self.account_management.storage_usage_bounds()
    }
}

impl Contract {
    /// gets run each time the contract is loaded from storage and instantiated
    fn init(&mut self) {
        // TODO: should be owned by the component
        eventbus::register(AccountStats::on_account_storage_event);
        self.account_management = create_account_management_component();
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

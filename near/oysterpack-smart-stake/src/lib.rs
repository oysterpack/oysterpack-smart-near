use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env,
    json_types::U128,
    near_bindgen, wee_alloc, PanicOnDefault,
};
use oysterpack_smart_account_management::{
    Account, AccountStats, AccountStorageEvent, StorageBalance, StorageBalanceBounds,
    StorageUsageBounds,
};
use oysterpack_smart_near::{data::Object, domain::YoctoNear, eventbus, service::*};

use oysterpack_smart_account_management::components::account_management::AccountManagementComponent;
use oysterpack_smart_account_management::components::account_storage_usage::AccountStorageUsageComponent;

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
#[borsh_init(init)]
pub struct Contract {
    #[borsh_skip]
    account_management: Option<AccountManagementComponent<()>>,
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
            account_management: Some(AccountManagementComponent::new(unregister_account)),
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
}

impl Contract {
    /// gets run each time the contract is loaded from storage and instantiated
    fn init(&mut self) {
        // TODO: should be owned by the component
        eventbus::register(AccountStats::on_account_storage_event);
    }
}

fn unregister_account(account: Account<()>, force: bool) {}

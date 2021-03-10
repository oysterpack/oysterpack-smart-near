mod context;

pub use context::*;

use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    near_bindgen, wee_alloc, PanicOnDefault,
};
use oysterpack_smart_account_management::{
    AccountStorageEvent, AccountStorageUsage, StorageBalance, StorageUsageBounds,
};
use oysterpack_smart_near::{contract_context::SmartContractContext, eventbus};

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
#[borsh_init(init)]
pub struct Contract {
    #[borsh_skip]
    context: Context,
}

#[near_bindgen]
impl Contract {
    #[init]
    pub fn deploy() -> Self {
        let mut context = Context::build(());
        Context::deploy(&mut context);
        Self { context }
    }
}

#[near_bindgen]
impl Contract {
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
        self.context.account_management.storage_usage_bounds()
    }
}

impl Contract {
    /// gets run each time the contract is loaded from storage and instantiated
    fn init(&mut self) {
        self.context = Context::build(());
    }
}

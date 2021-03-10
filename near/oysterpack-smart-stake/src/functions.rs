use crate::*;
use near_sdk::near_bindgen;
use oysterpack_smart_account_management::{AccountStorageEvent, StorageUsageBounds};
use oysterpack_smart_account_management::{AccountStorageUsage, StorageBalance};
use oysterpack_smart_near::eventbus;

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

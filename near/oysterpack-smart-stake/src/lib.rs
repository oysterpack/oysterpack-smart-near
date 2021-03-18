mod account_metrics;
mod account_storage_usage;
mod components;
mod storage_management;

use components::*;
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env, near_bindgen, PanicOnDefault,
};
use oysterpack_smart_account_management::StorageUsageBounds;
use oysterpack_smart_near::component::Deploy;

near_sdk::setup_alloc!();

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract;

#[near_bindgen]
impl Contract {
    #[init]
    pub fn deploy(config: Option<StorageUsageBounds>) -> Self {
        assert!(!env::state_exists(), "contract is already initialized");

        let config = config.unwrap_or_else(|| StorageUsageBounds {
            min: AccountManager::measure_storage_usage(()),
            max: None,
        });
        AccountManager::deploy(Some(config));

        Self
    }
}

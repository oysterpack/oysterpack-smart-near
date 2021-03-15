mod account_metrics;
mod storage_management;

use crate::storage_management::AccountManager;
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env, near_bindgen, wee_alloc, PanicOnDefault,
};
use oysterpack_smart_account_management::StorageUsageBounds;
use oysterpack_smart_near::component::Deploy;

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract;

#[near_bindgen]
impl Contract {
    #[init]
    pub fn deploy(config: Option<StorageUsageBounds>) -> Self {
        assert!(!env::state_exists(), "contract is already initialized");

        let config = config.unwrap_or_else(|| StorageUsageBounds {
            min: 1000.into(),
            max: None,
        });
        AccountManager::deploy(Some(config));

        Self
    }
}

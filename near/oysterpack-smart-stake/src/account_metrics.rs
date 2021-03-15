use crate::storage_management::AccountManager;
use crate::*;
use near_sdk::near_bindgen;
use oysterpack_smart_account_management::{AccountMetrics, GetAccountMetrics};

#[near_bindgen]
impl GetAccountMetrics for Contract {
    fn account_metrics() -> AccountMetrics {
        AccountManager::account_metrics()
    }
}

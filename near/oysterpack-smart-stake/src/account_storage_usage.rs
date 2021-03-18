use crate::*;
use near_sdk::json_types::ValidAccountId;
use oysterpack_smart_account_management::AccountStorageUsage;
use oysterpack_smart_near::domain::StorageUsage;

#[near_bindgen]
impl AccountStorageUsage for Contract {
    fn storage_usage_bounds(&self) -> StorageUsageBounds {
        Self::account_manager().storage_usage_bounds()
    }

    fn account_storage_usage(&self, account_id: ValidAccountId) -> Option<StorageUsage> {
        Self::account_manager().account_storage_usage(account_id)
    }
}

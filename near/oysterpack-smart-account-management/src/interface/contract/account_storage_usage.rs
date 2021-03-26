use crate::StorageUsageBounds;
use oysterpack_smart_near::domain::StorageUsage;
use oysterpack_smart_near::near_sdk::json_types::ValidAccountId;

/// # **Contract Interface**: Account Storage Usage API
///
/// Used to lookup account storage usage info   
pub trait AccountStorageUsage {
    /// returns the account storage use bounds set by the contract
    ///
    /// NOTE: [`crate::StorageBalanceBounds`] derives from this
    fn ops_storage_usage_bounds(&self) -> StorageUsageBounds;

    /// Returns the account storage usage in bytes
    ///
    /// Returns None if the account is not registered
    fn ops_storage_usage(&self, account_id: ValidAccountId) -> Option<StorageUsage>;
}

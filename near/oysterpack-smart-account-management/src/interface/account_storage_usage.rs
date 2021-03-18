use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    json_types::ValidAccountId,
    serde::{Deserialize, Serialize},
};
use oysterpack_smart_near::domain::StorageUsage;

/// Used to lookup account storage usage info   
pub trait AccountStorageUsage {
    /// returns the account storage use bounds set by the contract
    fn storage_usage_bounds(&self) -> StorageUsageBounds;

    /// Returns the account storage usage in bytes
    ///
    /// Returns None if the account is not registered
    fn storage_usage(&self, account_id: ValidAccountId) -> Option<StorageUsage>;
}

/// Used to configure account storage usage
#[derive(
    BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, PartialEq, Clone, Copy,
)]
#[serde(crate = "near_sdk::serde")]
pub struct StorageUsageBounds {
    /// the minimum storage that is required for the account on the contract
    pub min: StorageUsage,
    /// max storage that the contract is allowed to have on the contract
    pub max: Option<StorageUsage>,
}

/// Used to implement [`AccountStorageUsage`] on any type that implements this trait
/// - used to expose [`AccountStorageUsage`] on components
pub trait HasAccountStorageUsage {
    fn account_storage_usage(&self) -> &dyn AccountStorageUsage;
}

impl<T> AccountStorageUsage for T
where
    T: HasAccountStorageUsage,
{
    fn storage_usage_bounds(&self) -> StorageUsageBounds {
        self.account_storage_usage().storage_usage_bounds()
    }

    fn storage_usage(&self, account_id: ValidAccountId) -> Option<StorageUsage> {
        self.account_storage_usage().storage_usage(account_id)
    }
}

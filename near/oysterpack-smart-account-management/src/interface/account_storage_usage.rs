use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    json_types::ValidAccountId,
    serde::{Deserialize, Serialize},
};
use oysterpack_smart_near::domain::StorageUsage;

pub trait AccountStorageUsage {
    fn storage_usage_bounds(&self) -> StorageUsageBounds;

    /// Returns the account storage usage in bytes
    ///
    /// Returns None if the account is not registered
    fn storage_usage(&self, account_id: ValidAccountId) -> Option<StorageUsage>;
}

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

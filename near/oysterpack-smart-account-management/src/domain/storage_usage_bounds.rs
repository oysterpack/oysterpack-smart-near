use oysterpack_smart_near::{
    domain::StorageUsage,
    near_sdk::{
        borsh::{self, BorshDeserialize, BorshSerialize},
        serde::{Deserialize, Serialize},
    },
};

/// Used to configure account storage usage
#[derive(
    BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, PartialEq, Clone, Copy,
)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub struct StorageUsageBounds {
    /// the minimum storage that is required for the account on the contract
    pub min: StorageUsage,
    /// max storage that the contract is allowed to have on the contract
    pub max: Option<StorageUsage>,
}

use crate::ContractStorageUsageCosts;
use oysterpack_smart_near::domain::StorageUsage;
use oysterpack_smart_near::near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
};

/// Reports a breakdown of contract storage usage
#[derive(
    BorshSerialize, BorshDeserialize, Deserialize, Serialize, Copy, Clone, Debug, PartialEq, Default,
)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub struct ContractStorageUsage {
    total: StorageUsage,
    accounts: StorageUsage,
}

impl ContractStorageUsage {
    pub fn new(accounts: StorageUsage) -> Self {
        Self {
            total: StorageUsage::from_env(),
            accounts,
        }
    }

    /// total contract storage usage
    pub fn total(&self) -> StorageUsage {
        self.total
    }

    /// storage usage that is owned by all accounts registered with the contract
    pub fn accounts(&self) -> StorageUsage {
        self.accounts
    }

    /// returns the storage usage that the contract owner is responsible to pay for
    pub fn owner(&self) -> StorageUsage {
        self.total - self.accounts
    }

    pub fn costs(&self) -> ContractStorageUsageCosts {
        ContractStorageUsageCosts::from(*self)
    }
}

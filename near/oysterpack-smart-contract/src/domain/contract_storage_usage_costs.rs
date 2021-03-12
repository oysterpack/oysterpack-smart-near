use crate::ContractStorageUsage;
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
};
use oysterpack_smart_near::domain::YoctoNear;

#[derive(
    BorshSerialize, BorshDeserialize, Deserialize, Serialize, Copy, Clone, Debug, PartialEq, Default,
)]
#[serde(crate = "near_sdk::serde")]
pub struct ContractStorageUsageCosts {
    total: YoctoNear,
    accounts: YoctoNear,
}

impl ContractStorageUsageCosts {
    /// total contract storage usage
    pub fn total(&self) -> YoctoNear {
        self.total
    }

    /// storage usage that is owned by all accounts registered with the contract
    pub fn accounts(&self) -> YoctoNear {
        self.accounts
    }

    /// returns the storage usage that the contract owner is responsible to pay for
    pub fn owner(&self) -> YoctoNear {
        self.total - self.accounts
    }
}

impl From<ContractStorageUsage> for ContractStorageUsageCosts {
    fn from(storage_usage: ContractStorageUsage) -> Self {
        Self {
            total: storage_usage.total().cost(),
            accounts: storage_usage.accounts().cost(),
        }
    }
}

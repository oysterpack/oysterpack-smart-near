use crate::ContractStorageUsage;
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env,
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
    owner: YoctoNear,
}

impl ContractStorageUsageCosts {
    pub fn new(accounts: YoctoNear) -> Self {
        let total: YoctoNear = env::account_balance().into();
        Self {
            total,
            accounts,
            owner: total - accounts,
        }
    }

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
        self.owner
    }
}

impl From<ContractStorageUsage> for ContractStorageUsageCosts {
    fn from(storage_usage: ContractStorageUsage) -> Self {
        Self {
            total: storage_usage.total().cost(),
            accounts: storage_usage.accounts().cost(),
            owner: storage_usage.owner().cost(),
        }
    }
}

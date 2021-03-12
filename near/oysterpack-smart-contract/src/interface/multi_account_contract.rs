use crate::{ContractNearBalances, ContractStorageUsage, ContractStorageUsageCosts};
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
};
use oysterpack_smart_near::data::numbers::U128;
use oysterpack_smart_near::domain::BlockTime;

/// Contracts that support multiple account registration must track storage usage and NEAR balances
/// because at the very least accounts are required to pay for their own storage.
trait MultiAccountContract {
    fn total_registered_accounts(&self) -> U128;

    fn storage_usage(&self) -> ContractStorageUsage;

    fn near_balances(&self) -> ContractNearBalances;

    fn storage_usage_costs(&self) -> ContractStorageUsageCosts {
        self.storage_usage().into()
    }

    fn contract_stats(&self) -> ContractStats {
        let storage_usage = self.storage_usage();
        ContractStats {
            block_time: BlockTime::from_env(),
            total_registered_accounts: self.total_registered_accounts(),
            storage_usage,
            near_balances: self.near_balances(),
            storage_usage_costs: storage_usage.into(),
        }
    }
}

#[derive(BorshSerialize, BorshDeserialize, Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(crate = "near_sdk::serde")]
pub struct ContractStats {
    pub block_time: BlockTime,
    pub total_registered_accounts: U128,
    pub storage_usage: ContractStorageUsage,
    pub near_balances: ContractNearBalances,
    pub storage_usage_costs: ContractStorageUsageCosts,
}

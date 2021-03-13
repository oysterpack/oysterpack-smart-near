use crate::{ContractNearBalances, ContractStorageUsage, ContractStorageUsageCosts};
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
};
use oysterpack_smart_account_management::GetAccountMetrics;
use oysterpack_smart_near::data::numbers::U128;
use oysterpack_smart_near::domain::BlockTime;

/// Provides metrics that track storage usage and NEAR balances
pub trait ContractMetrics: GetAccountMetrics {
    fn total_registered_accounts(&self) -> U128 {
        self.account_metrics().total_registered_accounts
    }

    fn storage_usage(&self) -> ContractStorageUsage;

    fn near_balances(&self) -> ContractNearBalances;

    fn storage_usage_costs(&self) -> ContractStorageUsageCosts {
        self.storage_usage().into()
    }

    fn metrics(&self) -> ContractMetricsSnapshot {
        let storage_usage = self.storage_usage();
        ContractMetricsSnapshot {
            block_time: BlockTime::from_env(),
            total_registered_accounts: self.total_registered_accounts(),
            storage_usage,
            near_balances: self.near_balances(),
            storage_usage_costs: storage_usage.into(),
        }
    }
}

/// Provides a point in time metrics snapshot
#[derive(BorshSerialize, BorshDeserialize, Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(crate = "near_sdk::serde")]
pub struct ContractMetricsSnapshot {
    pub block_time: BlockTime,
    pub total_registered_accounts: U128,
    pub storage_usage: ContractStorageUsage,
    pub near_balances: ContractNearBalances,
    pub storage_usage_costs: ContractStorageUsageCosts,
}

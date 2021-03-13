use crate::{ContractNearBalances, ContractStorageUsage, ContractStorageUsageCosts};
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env,
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

    fn storage_usage(&self) -> ContractStorageUsage {
        let account_metrics = self.account_metrics();
        ContractStorageUsage::new(account_metrics.total_storage_usage)
    }

    fn near_balances(&self) -> ContractNearBalances {
        let account_metrics = self.account_metrics();
        let near_balances = ContractNearBalances::load_near_balances();
        let near_balances = if near_balances.is_empty() {
            None
        } else {
            Some(near_balances)
        };
        ContractNearBalances::new(
            env::account_balance().into(),
            account_metrics.total_near_balance,
            near_balances,
        )
    }

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

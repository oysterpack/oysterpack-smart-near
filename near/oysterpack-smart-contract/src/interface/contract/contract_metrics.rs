use crate::{ContractNearBalances, ContractStorageUsage, ContractStorageUsageCosts};
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
};
use oysterpack_smart_account_management::AccountMetrics;
use oysterpack_smart_near::data::numbers::U128;
use oysterpack_smart_near::domain::BlockTime;

/// # **Contract Interface**: Contract Metrics API
/// Provides metrics that track storage usage and NEAR balances
pub trait ContractMetrics {
    fn ops_metrics_total_registered_accounts() -> U128;

    fn ops_metrics_contract_storage_usage() -> ContractStorageUsage;

    fn ops_metrics_near_balances() -> ContractNearBalances;

    fn ops_metrics_storage_usage_costs() -> ContractStorageUsageCosts;

    fn ops_metrics() -> ContractMetricsSnapshot;

    fn ops_metrics_accounts() -> AccountMetrics;
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

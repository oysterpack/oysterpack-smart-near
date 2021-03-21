use crate::*;
use oysterpack_smart_account_management::AccountMetrics;
use oysterpack_smart_contract::components::contract_metrics::ContractMetricsComponent;
use oysterpack_smart_contract::{
    ContractMetrics, ContractMetricsSnapshot, ContractNearBalances, ContractStorageUsage,
    ContractStorageUsageCosts,
};
use oysterpack_smart_near::data::numbers::U128;

#[near_bindgen]
impl ContractMetrics for Contract {
    fn ops_metrics_total_registered_accounts() -> U128 {
        ContractMetricsComponent::ops_metrics_total_registered_accounts()
    }

    fn ops_metrics_contract_storage_usage() -> ContractStorageUsage {
        ContractMetricsComponent::ops_metrics_contract_storage_usage()
    }

    fn ops_metrics_near_balances() -> ContractNearBalances {
        ContractMetricsComponent::ops_metrics_near_balances()
    }

    fn ops_metrics_storage_usage_costs() -> ContractStorageUsageCosts {
        ContractMetricsComponent::ops_metrics_storage_usage_costs()
    }

    fn ops_metrics() -> ContractMetricsSnapshot {
        ContractMetricsComponent::ops_metrics()
    }

    fn ops_metrics_accounts() -> AccountMetrics {
        ContractMetricsComponent::ops_metrics_accounts()
    }
}

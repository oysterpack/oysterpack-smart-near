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
    fn total_registered_accounts() -> U128 {
        ContractMetricsComponent::total_registered_accounts()
    }

    fn contract_storage_usage() -> ContractStorageUsage {
        ContractMetricsComponent::contract_storage_usage()
    }

    fn near_balances() -> ContractNearBalances {
        ContractMetricsComponent::near_balances()
    }

    fn storage_usage_costs() -> ContractStorageUsageCosts {
        ContractMetricsComponent::storage_usage_costs()
    }

    fn metrics() -> ContractMetricsSnapshot {
        ContractMetricsComponent::metrics()
    }

    fn account_metrics() -> AccountMetrics {
        ContractMetricsComponent::account_metrics()
    }
}

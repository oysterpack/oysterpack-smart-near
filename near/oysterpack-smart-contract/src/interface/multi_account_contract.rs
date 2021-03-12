use crate::{ContractNearBalances, ContractStorageUsage, ContractStorageUsageCosts};
use oysterpack_smart_near::data::numbers::U128;

/// Contracts that support multiple account registration must track storage usage and NEAR balances
/// because at the very least accounts are required to pay for their own storage.
trait MultiAccountContract {
    fn total_registered_accounts(&self) -> U128;

    fn storage_usage(&self) -> ContractStorageUsage;

    fn near_balances(&self) -> ContractNearBalances;

    fn storage_usage_costs(&self) -> ContractStorageUsageCosts {
        self.storage_usage().into()
    }
}

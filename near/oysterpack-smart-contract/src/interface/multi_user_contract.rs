use crate::{ContractNearBalances, ContractStorageUsage};

trait MultiUserContract {
    fn storage_usage(&self) -> ContractStorageUsage;

    fn near_balances(&self) -> ContractNearBalances;
}

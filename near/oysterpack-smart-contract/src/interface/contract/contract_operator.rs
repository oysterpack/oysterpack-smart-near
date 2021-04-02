use crate::BalanceId;
use oysterpack_smart_near::domain::StorageUsage;

pub trait ContractOperator {
    /// Locks a portion of the contract's balance to reserve to pay for contract storage usage.
    ///
    /// The main use case is to lock enough contract balance to ensure the contract has enough balance to
    /// operate, i.e., pay for contract storage. This would prevent the contract owner from withdrawing
    /// all of his available balance, which might break the contract.
    ///
    /// setting the balance to zero, effectively unlocks the storage balance
    ///
    /// ## Panics
    /// - requires operator permission
    fn ops_operator_lock_storage_balance(&mut self, storage_usage: StorageUsage);
}

/// used by ['ContractOwnership::ops_owner_lock_balance`]
pub const CONTRACT_LOCKED_STORAGE_BALANCE_ID: BalanceId =
    BalanceId(1955299460766524333040021403508226880);

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

    /// Allows the owner to grants admin permission himself
    ///
    /// The owner should always be able to have admin access. If the admin permission was revoked
    /// from the owner, then this enables the owner to grant admin to himself.
    ///
    /// ## Panics
    /// If not invoked by the owner account
    fn ops_owner_grant_admin(&mut self);
}

/// used by ['ContractOwnership::ops_owner_lock_balance`]
pub const CONTRACT_LOCKED_STORAGE_BALANCE: BalanceId =
    BalanceId(1955299460766524333040021403508226880);

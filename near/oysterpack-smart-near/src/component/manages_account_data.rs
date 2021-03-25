use crate::domain::StorageUsage;

/// Components that manage account data must implement this trait.
/// WHen the contract is deployed this is used to set the minimum storage required for the account
/// to register.
pub trait ManagesAccountData {
    /// returns the minimum account storage required by this component
    fn account_storage_min() -> StorageUsage;
}

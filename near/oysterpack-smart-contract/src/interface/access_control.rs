use near_sdk::json_types::ValidAccountId;

pub trait AccessControl {
    /// contract owner is admin by default
    fn is_admin(&self, account_id: ValidAccountId) -> bool;

    /// Is restricted to contract owner and admins
    ///
    /// ## Panics
    /// - if predecessor account is not owner or admin
    /// - if `account_id` is not registered
    fn grant_admin(&mut self, account_id: ValidAccountId);

    /// Is restricted to contract owner and admins
    ///
    /// ## Panics
    /// - if predecessor account is not owner or admin
    /// - if `account_id` is not registered
    fn revoke_admin(&mut self, account_id: ValidAccountId);

    /// contract owner is admin by default
    fn is_operator(&self, account_id: ValidAccountId) -> bool;

    /// Is restricted to contract owner and admins
    ///
    /// ## Panics
    /// - if predecessor account is not owner or admin
    /// - if `account_id` is not registered
    fn grant_operator(&mut self, account_id: ValidAccountId);

    /// Is restricted to contract owner and admins
    ///
    /// ## Panics
    /// - if predecessor account is not owner or admin
    /// - if `account_id` is not registered
    fn revoke_operator(&mut self, account_id: ValidAccountId);

    /// Is restricted to contract owner and admins
    ///
    /// ## Panics
    /// - if predecessor account is not owner or admin
    /// - if `account_id` is not registered
    fn grant_access(&mut self, account_id: ValidAccountId, bitflags: u64);

    /// Is restricted to contract owner and admins
    ///
    /// ## Panics
    /// - if predecessor account is not owner or admin
    /// - if `account_id` is not registered
    fn revoke_access(&mut self, account_id: ValidAccountId, bitflags: u64);
}

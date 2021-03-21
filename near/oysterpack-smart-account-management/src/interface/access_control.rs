use crate::Permissions;
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
    fn grant_permissions(&mut self, account_id: ValidAccountId, permissions: Permissions);

    /// Is restricted to contract owner and admins
    ///
    /// ## Panics
    /// - if predecessor account is not owner or admin
    /// - if `account_id` is not registered
    fn revoke_permissions(&mut self, account_id: ValidAccountId, permissions: Permissions);

    /// Is restricted to contract owner and admins
    ///
    /// ## Panics
    /// - if predecessor account is not owner or admin
    /// - if `account_id` is not registered
    fn revoke_all_permissions(&mut self, account_id: ValidAccountId);

    /// returns true if the account has all of the specified permissions
    fn has_permissions(&self, account_id: ValidAccountId, permissions: Permissions) -> bool;
}

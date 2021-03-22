use crate::Permissions;
use near_sdk::json_types::ValidAccountId;
use oysterpack_smart_near::{ErrCode, ErrorConst, Level, LogEvent};
use std::collections::HashMap;

/// # **Contract Interface**: Permissions Management API
///
/// ## Notes
/// - admins have full access
pub trait PermissionsManagement {
    fn ops_permissions_is_admin(&self, account_id: ValidAccountId) -> bool;

    /// Is restricted to admins
    ///
    /// ## Panics
    /// - if predecessor account is not owner or admin
    /// - if `account_id` is not registered
    fn ops_permissions_grant_admin(&mut self, account_id: ValidAccountId);

    /// Is restricted to admins
    ///
    /// ## Panics
    /// - if predecessor account is not owner or admin
    /// - if `account_id` is not registered
    fn ops_permissions_revoke_admin(&mut self, account_id: ValidAccountId);

    /// contract owner is admin by default
    fn ops_permissions_is_operator(&self, account_id: ValidAccountId) -> bool;

    /// Is restricted to admins
    ///
    /// ## Panics
    /// - if predecessor account is not owner or admin
    /// - if `account_id` is not registered
    fn ops_permissions_grant_operator(&mut self, account_id: ValidAccountId);

    /// Is restricted to admins
    ///
    /// ## Panics
    /// - if predecessor account is not owner or admin
    /// - if `account_id` is not registered
    fn ops_permissions_revoke_operator(&mut self, account_id: ValidAccountId);

    /// Is restricted to admins.
    ///
    /// [`crate::Permissions::ADMIN`] and [`crate::Permissions::OPERATOR`] can not be granted - explicit grant functions
    /// must be used.
    ///
    /// ## Panics
    /// - if predecessor account is not owner or admin
    /// - if `account_id` is not registered
    /// - if permissions are not supported by the contract
    fn ops_permissions_grant(&mut self, account_id: ValidAccountId, permissions: Permissions);

    /// Is restricted to admins
    ///
    /// ## Panics
    /// - if predecessor account is not owner or admin
    /// - if `account_id` is not registered
    fn ops_permissions_revoke(&mut self, account_id: ValidAccountId, permissions: Permissions);

    /// Is restricted to admins
    ///
    /// ## Panics
    /// - if predecessor account is not owner or admin
    /// - if `account_id` is not registered
    fn ops_permissions_revoke_all(&mut self, account_id: ValidAccountId);

    /// returns true if the account has all of the specified permissions
    fn ops_permissions_contains(
        &self,
        account_id: ValidAccountId,
        permissions: Permissions,
    ) -> bool;

    /// returns the account's permissions
    /// - returns None if the account is not registered
    fn ops_permissions(&self, account_id: ValidAccountId) -> Option<Permissions>;

    /// lists the permission bits that are supported by the contract with a human friendly name
    /// - [`crate::Permissions::ADMIN`] and [`crate::Permissions::OPERATOR`] are excluded
    fn ops_permissions_contract_permissions(&self) -> Option<HashMap<u8, String>>;
}

pub const ERR_NOT_AUTHORIZED: ErrorConst = ErrorConst(
    ErrCode("NOT_AUTHORIZED"),
    "account is not authorized to perform the requested action",
);

pub const LOG_EVENT_PERMISSIONS_GRANT: LogEvent = LogEvent(Level::INFO, "PERMISSIONS_GRANT");
pub const LOG_EVENT_PERMISSIONS_REVOKE: LogEvent = LogEvent(Level::INFO, "PERMISSIONS_REVOKE");

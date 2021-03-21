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
    /// [`PERMISSION_ADMIN`] and [`PERMISSION_OPERATOR`] can not be granted - explicit grant functions
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
    /// - [`PERMISSION_ADMIN`] and [`PERMISSION_OPERATOR`] are excluded
    fn ops_permissions_supported_bits(&self) -> Option<HashMap<u8, String>>;
}

pub const ERR_NOT_AUTHORIZED: ErrorConst = ErrorConst(
    ErrCode("NOT_AUTHORIZED"),
    "account is not authorized to perform the requested action",
);

pub const LOG_EVENT_PERMISSIONS_GRANT: LogEvent = LogEvent(Level::INFO, "PERMISSIONS_GRANT");
pub const LOG_EVENT_PERMISSIONS_REVOKE: LogEvent = LogEvent(Level::INFO, "PERMISSIONS_REVOKE");

#[derive(Debug, Clone, Default)]
pub struct ContractPermissions(pub Option<HashMap<u8, &'static str>>);

impl ContractPermissions {
    pub fn is_supported(&self, permissions: Permissions) -> bool {
        self.0.as_ref().map_or(false, |perms| {
            let supported_perms = perms
                .keys()
                .fold(0_u64, |supported_perms, perm| supported_perms | 1 << *perm);
            Permissions(supported_perms.into()).contains(permissions)
        })
    }

    pub fn permission_labels(&self, permissions: Permissions) -> Vec<String> {
        self.0.as_ref().map_or(vec![], |perms| {
            perms
                .keys()
                .filter(|perm| permissions.contains(1_u64 << *perm))
                .fold(vec![], |mut labels, perm| {
                    labels.push(perms.get(perm).as_ref().unwrap().to_string());
                    labels
                })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_permissions() {
        let contract_permissions = ContractPermissions::default();
        assert!(!contract_permissions.is_supported((1 << 15).into()));
        assert!(!contract_permissions.is_supported((1 << 0).into()));
        assert!(contract_permissions
            .permission_labels((1 << 15).into())
            .is_empty());

        let mut perms = HashMap::new();
        perms.insert(10, "minter");
        perms.insert(20, "burner");
        let contract_permissions = ContractPermissions(Some(perms));

        assert!(!contract_permissions.is_supported((1 << 15).into()));
        assert!(contract_permissions.is_supported((1 << 10).into()));
        assert!(contract_permissions.is_supported(((1 << 10) | (1 << 20)).into()));
        assert!(!contract_permissions.is_supported(((1 << 10) | (1 << 15)).into()));

        let labels = contract_permissions.permission_labels(((1 << 10) | (1 << 20)).into());
        println!("{:?}", labels);
        assert_eq!(labels.len(), 2);
        assert!(labels.contains(&"minter".to_string()));
        assert!(labels.contains(&"burner".to_string()));

        let labels =
            contract_permissions.permission_labels(((1 << 10) | (1 << 20) | (1 << 15)).into());
        assert_eq!(labels.len(), 2);
        assert!(labels.contains(&"minter".to_string()));
        assert!(labels.contains(&"burner".to_string()));
    }
}

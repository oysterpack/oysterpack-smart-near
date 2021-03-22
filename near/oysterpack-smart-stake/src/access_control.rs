use crate::*;
use near_sdk::json_types::ValidAccountId;
use oysterpack_smart_account_management::{Permissions, PermissionsManagement};
use std::collections::HashMap;

impl PermissionsManagement for Contract {
    fn ops_permissions_is_admin(&self, account_id: ValidAccountId) -> bool {
        Self::account_manager().ops_permissions_is_admin(account_id)
    }

    fn ops_permissions_grant_admin(&mut self, account_id: ValidAccountId) {
        Self::account_manager().ops_permissions_grant_admin(account_id);
    }

    fn ops_permissions_revoke_admin(&mut self, account_id: ValidAccountId) {
        Self::account_manager().ops_permissions_revoke_admin(account_id);
    }

    fn ops_permissions_is_operator(&self, account_id: ValidAccountId) -> bool {
        Self::account_manager().ops_permissions_is_operator(account_id)
    }

    fn ops_permissions_grant_operator(&mut self, account_id: ValidAccountId) {
        Self::account_manager().ops_permissions_grant_operator(account_id);
    }

    fn ops_permissions_revoke_operator(&mut self, account_id: ValidAccountId) {
        Self::account_manager().ops_permissions_revoke_operator(account_id);
    }

    fn ops_permissions_grant(&mut self, account_id: ValidAccountId, permissions: Permissions) {
        Self::account_manager().ops_permissions_grant(account_id, permissions);
    }

    fn ops_permissions_revoke(&mut self, account_id: ValidAccountId, permissions: Permissions) {
        Self::account_manager().ops_permissions_revoke(account_id, permissions);
    }

    fn ops_permissions_revoke_all(&mut self, account_id: ValidAccountId) {
        Self::account_manager().ops_permissions_revoke_all(account_id);
    }

    fn ops_permissions_contains(
        &self,
        account_id: ValidAccountId,
        permissions: Permissions,
    ) -> bool {
        Self::account_manager().ops_permissions_contains(account_id, permissions)
    }

    fn ops_permissions(&self, account_id: ValidAccountId) -> Option<Permissions> {
        Self::account_manager().ops_permissions(account_id)
    }

    fn ops_permissions_contract_permissions(&self) -> Option<HashMap<u8, String>> {
        Self::account_manager().ops_permissions_contract_permissions()
    }
}

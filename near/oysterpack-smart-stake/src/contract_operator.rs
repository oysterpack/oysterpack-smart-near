use crate::*;
use oysterpack_smart_contract::ContractOperator;
use oysterpack_smart_near::{domain::StorageUsage, near_sdk::near_bindgen};

#[near_bindgen]
impl ContractOperator for Contract {
    fn ops_operator_lock_storage_balance(&mut self, storage_usage: StorageUsage) {
        Self::contract_operator().ops_operator_lock_storage_balance(storage_usage);
    }

    fn ops_owner_grant_admin(&mut self) {
        Self::contract_operator().ops_owner_grant_admin();
    }
}

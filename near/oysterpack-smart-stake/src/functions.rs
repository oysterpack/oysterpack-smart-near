use crate::*;
use near_sdk::json_types::ValidAccountId;
use near_sdk::near_bindgen;
use oysterpack_smart_account_management::{AccountMetrics, GetAccountMetrics, StorageBalance};
use oysterpack_smart_account_management::{StorageBalanceBounds, StorageManagement};
use oysterpack_smart_near::domain::YoctoNear;

#[near_bindgen]
impl StorageManagement for Contract {
    #[payable]
    fn storage_deposit(
        &mut self,
        account_id: Option<ValidAccountId>,
        registration_only: Option<bool>,
    ) -> StorageBalance {
        Context::build(())
            .account_management
            .storage_deposit(account_id, registration_only)
    }

    #[payable]
    fn storage_withdraw(&mut self, amount: Option<YoctoNear>) -> StorageBalance {
        Context::build(())
            .account_management
            .storage_withdraw(amount)
    }

    #[payable]
    fn storage_unregister(&mut self, force: Option<bool>) -> bool {
        Context::build(())
            .account_management
            .storage_unregister(force)
    }

    fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        Context::build(())
            .account_management
            .storage_balance_bounds()
    }

    fn storage_balance_of(&self, account_id: ValidAccountId) -> Option<StorageBalance> {
        Context::build(())
            .account_management
            .storage_balance_of(account_id)
    }
}

#[near_bindgen]
impl GetAccountMetrics for Contract {
    fn account_metrics() -> AccountMetrics {
        AccountManager::account_metrics()
    }
}

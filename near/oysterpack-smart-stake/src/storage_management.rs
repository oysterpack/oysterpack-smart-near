use crate::*;

use near_sdk::json_types::ValidAccountId;
use near_sdk::near_bindgen;
use oysterpack_smart_account_management::StorageBalance;
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
        Self::account_manager().storage_deposit(account_id, registration_only)
    }

    #[payable]
    fn storage_withdraw(&mut self, amount: Option<YoctoNear>) -> StorageBalance {
        Self::account_manager().storage_withdraw(amount)
    }

    #[payable]
    fn storage_unregister(&mut self, force: Option<bool>) -> bool {
        Self::account_manager().storage_unregister(force)
    }

    fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        Self::account_manager().storage_balance_bounds()
    }

    fn storage_balance_of(&self, account_id: ValidAccountId) -> Option<StorageBalance> {
        Self::account_manager().storage_balance_of(account_id)
    }
}

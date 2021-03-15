use crate::*;

use near_sdk::json_types::ValidAccountId;
use near_sdk::near_bindgen;
use oysterpack_smart_account_management::components::account_management::{
    AccountManagementComponent, UnregisterAccount,
};
use oysterpack_smart_account_management::StorageBalance;
use oysterpack_smart_account_management::{StorageBalanceBounds, StorageManagement};
use oysterpack_smart_near::domain::YoctoNear;
use teloc::*;

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

pub type AccountData = ();

pub type AccountManager = AccountManagementComponent<AccountData>;

impl Contract {
    pub fn account_manager() -> AccountManager {
        let container = ServiceProvider::new()
            .add_transient_c::<Box<dyn UnregisterAccount>, Box<UnregisterMock>>()
            .add_transient::<AccountManager>();

        container.resolve()
    }
}

#[derive(Dependency)]
struct UnregisterMock;

impl UnregisterAccount for UnregisterMock {
    fn unregister_account(&mut self, _force: bool) {}
}

impl From<Box<UnregisterMock>> for Box<dyn UnregisterAccount> {
    fn from(x: Box<UnregisterMock>) -> Self {
        x
    }
}

use crate::AccountDataObject;
use crate::AccountNearDataObject;
use near_sdk::borsh::{BorshDeserialize, BorshSerialize};
use oysterpack_smart_near::{domain::YoctoNear, ErrCode, ErrorConst};
use std::fmt::Debug;

pub type Account<T> = (AccountNearDataObject, Option<AccountDataObject<T>>);

/// Used for account data access
pub trait AccountRepository<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    /// Creates a new account.
    ///
    /// - tracks storage usage - emits [`AccountStorageEvent::StorageUsageChanged`]
    ///
    /// # Panics
    /// if the account already is registered
    fn create_account(
        &mut self,
        account_id: &str,
        near_balance: YoctoNear,
        data: Option<T>,
    ) -> Account<T>;

    /// tries to load the account from storage
    fn load_account(&self, account_id: &str) -> Option<Account<T>>;

    /// tries to load the account data from storage
    fn load_account_data(&self, account_id: &str) -> Option<AccountDataObject<T>>;

    /// tries to load the account NEAR data from storage
    fn load_account_near_data(&self, account_id: &str) -> Option<AccountNearDataObject>;

    /// ## Panics
    /// if the account is not registered
    fn registered_account(&self, account_id: &str) -> Account<T>;
    /// ## Panics
    /// if the account is not registered
    fn registered_account_near_data(&self, account_id: &str) -> AccountNearDataObject;

    /// Assumes that the account will always have data if registered.
    ///
    /// ## Panics
    /// if the account is not registered
    fn registered_account_data(&self, account_id: &str) -> AccountDataObject<T>;

    fn account_exists(&self, account_id: &str) -> bool;

    /// Deletes [AccountNearDataObject] and [AccountDataObject] for the specified  account ID
    /// - tracks storage usage - emits [`AccountStorageEvent::StorageUsageChanged`]
    fn delete_account(&mut self, account_id: &str);
}

pub const ERR_ACCOUNT_NOT_REGISTERED: ErrorConst = ErrorConst(
    ErrCode("ACCOUNT_NOT_REGISTERED"),
    "account is not registered",
);

pub const ERR_ACCOUNT_ALREADY_REGISTERED: ErrorConst = ErrorConst(
    ErrCode("ACCOUNT_ALREADY_REGISTERED"),
    "account is already registered",
);

use crate::Account;
use near_sdk::borsh::{BorshDeserialize, BorshSerialize};
use oysterpack_smart_near::domain::YoctoNear;
use std::fmt::Debug;

/// Provides default implementation for managing accounts on blockchain storage
pub trait AccountRepository<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    /// Creates a new in memory account object
    fn new_account(&self, account_id: &str, near_balance: YoctoNear, data: T) -> Account<T> {
        Account::<T>::new(account_id, near_balance, data)
    }

    /// tries to load the account from storage
    fn load_account(&self, account_id: &str) -> Option<Account<T>> {
        Account::<T>::load(account_id)
    }

    /// ## Panics
    /// if the account is not registered
    fn registered_account(&self, account_id: &str) -> Account<T> {
        Account::<T>::registered_account(account_id)
    }

    fn account_exists(&self, account_id: &str) -> bool {
        Account::<T>::exists(account_id)
    }
}

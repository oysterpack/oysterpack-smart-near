use oysterpack_smart_near::data::Object;
use oysterpack_smart_near::{eventbus, Hash};

use oysterpack_smart_near::near_sdk::{
    borsh::{BorshDeserialize, BorshSerialize},
    env,
};

use crate::{AccountIdHash, AccountStorageEvent};
use std::fmt::Debug;
use std::ops::{Deref, DerefMut};

type DAO<T> = Object<AccountIdHash, T>;

/// Generic persistent account data
///
/// ## Notes
/// - keeps track of its own storage usage, i.e., emits [`AccountStorageEvent::StorageUsageChanged`]
///   events when the object is saved or deleted
/// - any account storage usage that is outside of this Account object must be tracked externally
#[derive(Clone, Debug, PartialEq)]
pub struct AccountDataObject<T>(DAO<T>)
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default;

impl<T> AccountDataObject<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    /// Creates a new in memory account object
    /// - its storage usage will be initialized to the serialized object byte size, but this won't
    ///   match the actual storage usage when the object is saved because there is overhead
    pub fn new(account_id: &str, data: T) -> Self {
        let key = Hash::from(account_id);
        Self(DAO::<T>::new(AccountIdHash(key), data))
    }

    /// tries to load the account from storage
    pub fn load(account_id: &str) -> Option<Self> {
        let key = Hash::from(account_id);
        DAO::load(&AccountIdHash(key)).map(|account| Self(account))
    }

    pub fn exists(account_id: &str) -> bool {
        let key = Hash::from(account_id);
        DAO::<T>::exists(&AccountIdHash(key))
    }

    /// tracks storage usage changes - emits [`AccountStorageEvent::StorageUsageChanged`] event
    pub fn save(&mut self) {
        let storage_usage_before_save = env::storage_usage();
        self.0.save();
        let storage_usage_after_save = env::storage_usage();
        if storage_usage_after_save == storage_usage_before_save {
            return;
        }
        let event = if storage_usage_after_save > storage_usage_before_save {
            let storage_usage_change = storage_usage_after_save - storage_usage_before_save;
            AccountStorageEvent::StorageUsageChanged(
                self.key().clone(),
                storage_usage_change.into(),
            )
        } else {
            let storage_usage_change = storage_usage_before_save - storage_usage_after_save;
            AccountStorageEvent::StorageUsageChanged(
                self.key().clone(),
                (storage_usage_change as i64 * -1).into(),
            )
        };
        eventbus::post(&event);
    }

    /// tracks storage usage - emits [`AccountStorageEvent::StorageUsageChanged`] event
    pub fn delete(self) -> bool {
        let key = self.key().clone();
        let storage_usage_before_save = env::storage_usage();
        let result = self.0.delete();
        let storage_usage_deleted = storage_usage_before_save - env::storage_usage();
        if storage_usage_deleted > 0 {
            eventbus::post(&AccountStorageEvent::StorageUsageChanged(
                key,
                (storage_usage_deleted as i64 * -1).into(),
            ))
        }
        result
    }
}

impl<T> Deref for AccountDataObject<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    type Target = DAO<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for AccountDataObject<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oysterpack_smart_near_test::*;

    type ContractAccount = AccountDataObject<String>;

    #[test]
    fn account_crud() {
        // Arrange
        let account_id = "bob.near";
        let context = new_context(account_id);
        testing_env!(context);

        // Assert
        assert!(ContractAccount::load(account_id).is_none());

        // Act - create account
        let mut account = ContractAccount::new(account_id, "data".to_string());
        account.save();

        // Act - load account from storage
        let mut account2 = ContractAccount::load(account_id).unwrap();
        assert_eq!(account, account2);
        println!("{:?}", *account);

        // Act - update account data
        let data = &mut account2;
        println!("{:?}", data);
        data.make_ascii_uppercase();
        println!("{:?}", data);
        account2.save();

        // Assert - update was persisted
        let account3 = ContractAccount::load(account_id).unwrap();
        {
            assert_eq!(account3, account2);
            assert_eq!(account3.as_str(), "DATA");
        }

        // Act - delete account
        assert!(account3.delete());
        assert!(!ContractAccount::exists(account_id));
    }
}

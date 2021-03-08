use oysterpack_smart_near::data::Object;
use oysterpack_smart_near::Hash;

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};

use crate::StorageBalance;
use oysterpack_smart_near::domain::{StorageUsage, StorageUsageChange, YoctoNear};
use std::convert::TryInto;
use std::fmt::Debug;
use std::ops::{Deref, DerefMut};

/// account ID hash -> [`AccountData`]
pub type AccountObject<T> = Object<Hash, AccountData<T>>;

/// Represents a persistent contract account that wraps [`AccountObject`]
#[derive(Clone, Debug, PartialEq)]
pub struct Account<T>(AccountObject<T>)
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default;

impl<T> Account<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    /// Creates a new in memory account object
    /// - its storage usage will be initialized to the serialized object byte size
    pub fn new(account_id: &str, near_balance: YoctoNear, data: T) -> Self {
        let key = Hash::from(account_id);
        let mut account = Self(AccountObject::<T>::new(
            key,
            AccountData::new(near_balance, 0.into(), data),
        ));
        let storage_usage = account.serialized_byte_size();
        account.set_storage_usage(storage_usage.into());
        account
    }

    /// tries to load the account from storage
    pub fn load(account_id: &str) -> Option<Self> {
        let key = Hash::from(account_id);
        AccountObject::load(&key).map(|account| Self(account))
    }

    /// ## Panics
    /// if the account is not registered
    pub fn registered_account(account_id: &str) -> Self {
        Account::load(account_id).unwrap()
    }

    pub fn exists(account_id: &str) -> bool {
        let key = Hash::from(account_id);
        AccountObject::<T>::exists(&key)
    }

    pub fn delete(self) -> bool {
        self.0.delete()
    }

    pub fn storage_balance(&self, required_min_storage_balance: YoctoNear) -> StorageBalance {
        StorageBalance {
            total: self.near_balance,
            available: (self.near_balance.value() - required_min_storage_balance.value()).into(),
        }
    }
}

impl<T> Deref for Account<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    type Target = AccountObject<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Account<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Account data that is stored on the blockchain
///
/// On the NEAR blockchain, all accounts are required to pay for their own storage. Thus, for each
/// account we must track its storage usage and its NEAR balance to ensure the account has enough
/// NEAR balance to pay for storage.
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq)]
pub struct AccountData<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq,
{
    near_balance: YoctoNear,
    storage_usage: StorageUsage,
    data: T,
}

impl<T> AccountData<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq,
{
    /// constructor
    pub fn new(near_balance: YoctoNear, storage_usage: StorageUsage, data: T) -> Self {
        Self {
            near_balance,
            storage_usage,
            data,
        }
    }

    pub fn near_balance(&self) -> YoctoNear {
        self.near_balance
    }

    pub fn storage_usage(&self) -> StorageUsage {
        self.storage_usage
    }

    /// ## Panics
    /// if overflow occurs
    pub fn incr_near_balance(&mut self, amount: YoctoNear) {
        *self.near_balance = self.near_balance.checked_add(amount.value()).unwrap();
    }

    /// ## Panics
    /// if overflow occurs
    pub fn dec_near_balance(&mut self, amount: YoctoNear) {
        *self.near_balance = self.near_balance.checked_sub(amount.value()).unwrap();
    }

    pub fn set_near_balance(&mut self, amount: YoctoNear) {
        *self.near_balance = amount.value();
    }

    /// ## Panics
    /// if overflow occurs
    pub fn update_storage_usage(&mut self, change: StorageUsageChange) {
        let storage_usage = *self.storage_usage;
        let storage_usage: i64 = storage_usage.try_into().unwrap();
        let storage_value = storage_usage.checked_add(change.value()).unwrap();
        *self.storage_usage = storage_value.try_into().unwrap();
    }

    /// ## Panics
    /// if overflow occurs
    pub fn incr_storage_usage(&mut self, amount: StorageUsage) {
        *self.storage_usage = self.storage_usage.checked_add(amount.value()).unwrap();
    }

    /// ## Panics
    /// if overflow occurs
    pub fn dec_storage_usage(&mut self, amount: StorageUsage) {
        *self.storage_usage = self.storage_usage.checked_sub(amount.value()).unwrap();
    }

    pub fn set_storage_usage(&mut self, amount: StorageUsage) {
        *self.storage_usage = amount.value();
    }

    pub fn data(&self) -> &T {
        &self.data
    }

    /// returns a mutable reference to data that enables the account data to be changed
    pub fn data_mut(&mut self) -> &mut T {
        &mut self.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oysterpack_smart_near::YOCTO;
    use oysterpack_smart_near_test::*;

    type ContractAccount = Account<String>;

    #[test]
    fn account_crud() {
        // Arrange
        let account_id = "bob.near";
        let context = new_context(account_id);
        testing_env!(context);

        // Assert
        assert!(ContractAccount::load(account_id).is_none());

        // Act - create account
        let account = ContractAccount::new(account_id, YOCTO.into(), "data".to_string());
        account.save();

        // Act - load account from storage
        let mut account2 = ContractAccount::load(account_id).unwrap();
        assert_eq!(account, account2);
        println!("near_balance: {:?}", account.near_balance());

        // Act - update account data
        let data = account2.data_mut();
        println!("{:?}", data);
        data.make_ascii_uppercase();
        println!("{:?}", data);
        account2.save();

        // Assert - update was persisted
        let account3 = ContractAccount::load(account_id).unwrap();
        {
            assert_eq!(account3, account2);
            assert_eq!(account3.data, "DATA".to_string());
        }

        // Act - delete account
        assert!(account3.delete());
        assert!(!ContractAccount::exists(account_id));
    }

    #[test]
    fn update_near_balance() {
        // Arrange
        let account_id = "bob.near";
        let context = new_context(account_id);
        testing_env!(context);

        let mut account = ContractAccount::new(account_id, YOCTO.into(), "data".to_string());

        // Act - incr near balance
        account.incr_near_balance(YOCTO.into());
        account.save();

        // Assert
        let mut account = ContractAccount::load(account_id).unwrap();
        assert_eq!(account.near_balance(), (2 * YOCTO).into());

        // Act - dec near balance
        account.dec_near_balance(YOCTO.into());
        account.save();

        // Assert
        let mut account = ContractAccount::load(account_id).unwrap();
        assert_eq!(account.near_balance(), YOCTO.into());

        // Act - set near balance
        account.set_near_balance((2 * YOCTO).into());
        account.save();

        // Assert
        let account = ContractAccount::load(account_id).unwrap();
        assert_eq!(account.near_balance(), (2 * YOCTO).into());
    }

    #[test]
    fn update_storage_usage() {
        // Arrange
        let account_id = "bob.near";
        let context = new_context(account_id);
        testing_env!(context);

        let mut account = ContractAccount::new(account_id, YOCTO.into(), "data".to_string());
        let initial_storage_usage = account.storage_usage;

        // Act - incr near balance
        account.incr_storage_usage(1000.into());
        account.save();

        // Assert
        let mut account = ContractAccount::load(account_id).unwrap();
        assert_eq!(
            account.storage_usage(),
            (initial_storage_usage.value() + 1000).into()
        );

        // Act - dec
        account.dec_storage_usage(1000.into());
        account.save();

        // Assert
        let mut account = ContractAccount::load(account_id).unwrap();
        assert_eq!(account.storage_usage(), initial_storage_usage);

        // Act - set near balance
        account.set_storage_usage(2000.into());
        account.save();

        // Assert
        let mut account = ContractAccount::load(account_id).unwrap();
        assert_eq!(account.storage_usage(), 2000.into());

        // Act - update near balance
        account.update_storage_usage(1000.into());
        account.save();

        // Assert
        let mut account = ContractAccount::load(account_id).unwrap();
        assert_eq!(account.storage_usage(), 3000.into());

        // Act - update near balance
        account.update_storage_usage(StorageUsageChange(-1000));
        account.save();

        // Assert
        let mut account = ContractAccount::load(account_id).unwrap();
        assert_eq!(account.storage_usage(), 2000.into());

        // Act - update near balance
        account.update_storage_usage(0.into());
        account.save();

        // Assert
        let account = ContractAccount::load(account_id).unwrap();
        assert_eq!(account.storage_usage(), 2000.into());
    }
}

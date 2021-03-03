use oysterpack_smart_near::data::Object;
use oysterpack_smart_near::hash::Hash;

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};

use oysterpack_smart_near::domain::YoctoNear;
use std::fmt::Debug;
use std::ops::{Deref, DerefMut};

/// key = account ID hash
pub type AccountObject<T> = Object<Hash, AccountData<T>>;

#[derive(Clone, Debug, PartialEq)]
pub struct Account<T>(AccountObject<T>)
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq;

impl<T> Account<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq,
{
    pub fn new(account_id: &str, near_balance: YoctoNear, data: T) -> Self {
        let key = Hash::from(account_id);
        Self(AccountObject::<T>::new(
            key,
            AccountData::new(near_balance, data),
        ))
    }

    /// tries to load the account from storage
    pub fn load(account_id: &str) -> Option<Self> {
        let key = Hash::from(account_id);
        AccountObject::load(&key).map(|account| Self(account))
    }

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
}

impl<T> Deref for Account<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq,
{
    type Target = AccountObject<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Account<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq)]
pub struct AccountData<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq,
{
    near_balance: YoctoNear,
    data: T,
}

impl<T> AccountData<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq,
{
    pub fn new(near_balance: YoctoNear, data: T) -> Self {
        Self { near_balance, data }
    }

    pub fn near_balance(&self) -> YoctoNear {
        self.near_balance
    }

    pub fn incr_near_balance(&mut self, amount: YoctoNear) {
        *self.near_balance.deref_mut() += amount.value();
    }

    pub fn dec_near_balance(&mut self, amount: YoctoNear) {
        *self.near_balance.deref_mut() -= amount.value();
    }

    pub fn set_near_balance(&mut self, amount: YoctoNear) {
        *self.near_balance.deref_mut() = amount.value();
    }

    pub fn data(&self) -> &T {
        &self.data
    }

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
}

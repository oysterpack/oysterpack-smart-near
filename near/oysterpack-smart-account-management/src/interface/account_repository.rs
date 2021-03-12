use crate::AccountNearDataObject;
use crate::{AccountDataObject, AccountStorageEvent};
use near_sdk::{
    borsh::{BorshDeserialize, BorshSerialize},
    env,
};
use oysterpack_smart_near::{domain::YoctoNear, eventbus, ErrCode, ErrorConst};
use std::fmt::Debug;

pub type Account<T> = (AccountNearDataObject, Option<AccountDataObject<T>>);

/// Provides default implementation for managing accounts on blockchain storage
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
    ) -> Account<T> {
        ERR_ACCOUNT_ALREADY_REGISTERED.assert(|| !AccountNearDataObject::exists(account_id));

        let near_data = AccountNearDataObject::new(account_id, near_balance);

        // measure the storage usage
        let initial_storage_usage = env::storage_usage();
        near_data.save();
        let account_storage_usage = env::storage_usage() - initial_storage_usage;
        // update the account storage usage
        eventbus::post(&AccountStorageEvent::StorageUsageChanged(
            near_data.key().account_id_hash(),
            account_storage_usage.into(),
        ));

        match data {
            Some(data) => {
                let mut data = AccountDataObject::<T>::new(account_id, data);
                data.save();
                (near_data, Some(data))
            }
            None => (near_data, None),
        }
    }

    /// tries to load the account from storage
    fn load_account(&self, account_id: &str) -> Option<Account<T>> {
        self.load_account_near_data(account_id)
            .map(|near_data| (near_data, self.load_account_data(account_id)))
    }

    /// tries to load the account data from storage
    fn load_account_data(&self, account_id: &str) -> Option<AccountDataObject<T>> {
        AccountDataObject::<T>::load(account_id)
    }

    /// tries to load the account NEAR data from storage
    fn load_account_near_data(&self, account_id: &str) -> Option<AccountNearDataObject> {
        AccountNearDataObject::load(account_id)
    }

    /// ## Panics
    /// if the account is not registered
    fn registered_account(&self, account_id: &str) -> Account<T> {
        self.load_account(account_id).unwrap_or_else(|| {
            ERR_ACCOUNT_NOT_REGISTERED.panic();
            unreachable!()
        })
    }

    /// ## Panics
    /// if the account is not registered
    fn registered_account_near_data(&self, account_id: &str) -> AccountNearDataObject {
        self.load_account_near_data(account_id).unwrap_or_else(|| {
            ERR_ACCOUNT_NOT_REGISTERED.panic();
            unreachable!()
        })
    }

    /// Assumes that the account will always have data if registered.
    ///
    /// ## Panics
    /// if the account is not registered
    fn registered_account_data(&self, account_id: &str) -> AccountDataObject<T> {
        self.load_account_data(account_id).unwrap_or_else(|| {
            ERR_ACCOUNT_NOT_REGISTERED.panic();
            unreachable!()
        })
    }

    fn account_exists(&self, account_id: &str) -> bool {
        AccountNearDataObject::exists(account_id)
    }

    /// Deletes [AccountNearDataObject] and [AccountDataObject] for the specified  account ID
    /// - tracks storage usage - emits [`AccountStorageEvent::StorageUsageChanged`]
    fn delete_account(&mut self, account_id: &str) {
        if let Some((near_data, data)) = self.load_account(account_id) {
            near_data.delete();
            if let Some(data) = data {
                data.delete();
            }
        }
    }
}

pub const ERR_ACCOUNT_NOT_REGISTERED: ErrorConst = ErrorConst(
    ErrCode("ACCOUNT_NOT_REGISTERED"),
    "account is not registered",
);

pub const ERR_ACCOUNT_ALREADY_REGISTERED: ErrorConst = ErrorConst(
    ErrCode("ACCOUNT_ALREADY_REGISTERED"),
    "account is already registered",
);

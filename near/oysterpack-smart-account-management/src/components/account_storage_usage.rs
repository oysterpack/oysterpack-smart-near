use near_sdk::{
    borsh::{BorshDeserialize, BorshSerialize},
    json_types::ValidAccountId,
};
use oysterpack_smart_near::domain::StorageUsage;

use crate::{AccountRepository, AccountStorageUsage, StorageUsageBounds};
use oysterpack_smart_near::service::{Deploy, Service};
use std::fmt::Debug;
use std::marker::PhantomData;

#[derive(Default)]
pub struct AccountStorageUsageComponent<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    _phantom: PhantomData<T>,
}

impl<T> AccountStorageUsage for AccountStorageUsageComponent<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    fn storage_usage_bounds(&self) -> StorageUsageBounds {
        *Self::load_state().unwrap()
    }

    fn storage_usage(&self, account_id: ValidAccountId) -> Option<StorageUsage> {
        self.load_account(account_id.as_ref().as_str())
            .map(|account| account.storage_usage())
    }
}

impl<T> AccountRepository<T> for AccountStorageUsageComponent<T> where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default
{
}

impl<T> Service for AccountStorageUsageComponent<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    type State = StorageUsageBounds;

    fn state_key() -> u128 {
        1952475351321611295376996018476025471
    }
}

impl<T> Deploy for AccountStorageUsageComponent<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    type Config = Self::State;

    fn deploy(config: Option<Self::Config>) {
        let state = config.expect("initial state must be provided");
        let state = Self::new_state(state);
        state.save();
    }
}

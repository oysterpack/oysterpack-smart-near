use crate::{AccountRepository, AccountStorageUsage, AccountTracking, StorageManagement};
use near_sdk::borsh::{BorshDeserialize, BorshSerialize};
use shaku::Interface;
use std::fmt::Debug;

pub trait AccountManagementService<T>:
    StorageManagement + AccountStorageUsage + AccountTracking + AccountRepository<T> + Interface
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
}

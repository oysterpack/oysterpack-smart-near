//! Provides abstraction for object storage on the NEAR blockchain

use near_sdk::{
    borsh::{BorshDeserialize, BorshSerialize},
    env,
};
use std::ops::{Deref, DerefMut};
use std::{fmt::Debug, hash::Hash};

/// Object supports persistence to NEAR blockchain storage, i.e., on the Trie
#[derive(Clone, Debug, PartialEq)]
pub struct Object<K, V>(K, V)
where
    K: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Hash,
    V: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq;

impl<K, V> Object<K, V>
where
    K: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Hash,
    V: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq,
{
    /// Object is created in memory, i.e., it is not persisted to storage.
    /// - use [`Object::save`] to persist the object to storage on the NEAR blockchain
    pub fn new(key: K, value: V) -> Self {
        Self(key, value)
    }

    pub fn key(&self) -> &K {
        &self.0
    }

    pub fn exists(key: &K) -> bool {
        object_exists(key)
    }

    /// Tries to load the object from storage using the specified key.
    ///
    /// ## Panics
    /// if Borsh deserialization fails - which should never happen, but if it does then it means
    /// there is a bug
    /// - either in borsh (unlikely), or an object of a different type was stored with the same key
    pub fn load(key: &K) -> Option<Self> {
        let key_bytes = object_serialize_key(key);
        env::storage_read(&key_bytes)
            .map(|value| V::try_from_slice(&value).unwrap())
            .map(|value| Object(key.clone(), value))
    }

    /// Saves the object to persistent storage on the NEAR blockchain
    /// - will overwrite any other object with the same key - if your use case requires that there
    ///   shouldn't be a pre-existing value, then  use [`Object::exists`] before saving the object
    pub fn save(&self) {
        let key = object_serialize_key(&self.0);
        let value = self.1.try_to_vec().unwrap();
        env::storage_write(&key, &value);
    }

    /// Deletes the object from storage and consumes the object
    ///
    /// Returns true if the object existed.
    pub fn delete(self) -> bool {
        let key = object_serialize_key(&self.0);
        env::storage_remove(&key)
    }

    /// Returns the borsh serialized byte size for the object
    pub fn serialized_byte_size(&self) -> u64 {
        let key_len = self.0.try_to_vec().unwrap().len() as u64;
        let value_len = self.1.try_to_vec().unwrap().len() as u64;
        key_len + value_len
    }
}

impl<K, V> Deref for Object<K, V>
where
    K: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Hash,
    V: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq,
{
    type Target = V;

    fn deref(&self) -> &Self::Target {
        &self.1
    }
}

impl<K, V> DerefMut for Object<K, V>
where
    K: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Hash,
    V: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.1
    }
}

/// Used to check if the object is persisted to contract storage on the NEAR blockchain.
///
/// ## Panics
/// if `key` fails to serialize, but this is expected to never happen, unless there is a bug in borsh
pub fn object_exists<K: BorshSerialize>(key: &K) -> bool {
    let key = object_serialize_key(key);
    env::storage_has_key(&key)
}

/// Serializes the key using Borsh and then applies sha256 hash.
/// - Keys are always hashed before storing to ensure even distribution and const size.
///
/// ## Notes
/// - depends on NEAR provided `env::sha256()`
///
/// ## Panics
/// if data fails to serialize, but this is expected to never happen, unless there is a bug in borsh
fn object_serialize_key<K: BorshSerialize>(key: &K) -> Vec<u8> {
    let bytes = key.try_to_vec().unwrap();
    env::sha256(&bytes)
}

#[cfg(test)]
mod test {
    use super::*;
    use oysterpack_smart_near_test::*;

    type Data = Object<u128, u128>;

    #[test]
    fn crud() {
        // Arrange
        let context = new_context("bob");
        testing_env!(context);

        let data = Data::new(1, 2);

        // Assert
        assert!(!object_exists(data.key()));

        // Act - save the object
        data.save();
        assert!(object_exists(data.key()));

        let mut data2 = Data::load(data.key()).unwrap();
        assert_eq!(data, data2);

        // change the value and then save it
        *data2 = 3_u128;
        assert_eq!(*data2, 3);
        data2.save();

        let data3 = Data::load(data.key()).unwrap();
        assert_eq!(data3, data2);

        // delete from storage
        assert!(data3.delete());
        assert!(Data::load(data.key()).is_none())
    }
}

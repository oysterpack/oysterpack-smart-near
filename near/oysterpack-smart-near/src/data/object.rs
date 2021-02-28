//! Provides abstraction for object storage on the NEAR blockchain

use failure::Fail;
use near_sdk::{
    borsh::{maybestd::io::Error as BorshError, BorshDeserialize, BorshSerialize},
    env,
};
use std::ops::{Deref, DerefMut};
use std::{fmt::Debug, hash::Hash};

/// Object supports persistence to NEAR blockchain storage, i.e., on the Trie
#[derive(Clone, Debug, PartialEq)]
pub struct Object<K, V>(K, V)
where
    K: BorshSerialize + Clone + Debug + PartialEq + Hash,
    V: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq;

impl<K, V> Object<K, V>
where
    K: BorshSerialize + Clone + Debug + PartialEq + Hash,
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

    pub fn value(&self) -> &V {
        &self.1
    }

    pub fn exists(key: &K) -> bool {
        object_exists(key)
    }

    /// Tries to load the object from storage using the specified key.
    pub fn load(key: &K) -> Result<Option<Self>, ObjectError> {
        let key_bytes = object_serialize_key(key);
        match env::storage_read(&key_bytes) {
            None => Ok(None),
            Some(value) => match V::try_from_slice(&value) {
                Ok(value) => Ok(Some(Object(key.clone(), value))),
                Err(err) => Err(ObjectError::BorshDeserializationFailed(err)),
            },
        }
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
}

impl<K, V> Deref for Object<K, V>
where
    K: BorshSerialize + Clone + Debug + PartialEq + Hash,
    V: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq,
{
    type Target = V;

    fn deref(&self) -> &Self::Target {
        self.value()
    }
}

impl<K, V> DerefMut for Object<K, V>
where
    K: BorshSerialize + Clone + Debug + PartialEq + Hash,
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

#[derive(Fail, Debug)]
pub enum ObjectError {
    #[fail(display = "Borsh deserialization failed: {}", _0)]
    BorshDeserializationFailed(BorshError),
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

        let mut data2 = Data::load(data.key()).unwrap().unwrap();
        assert_eq!(data, data2);

        // change the value and then save it
        *data2.deref_mut() = 3_u128;
        assert_eq!(*data2.value(), 3);
        data2.save();

        let data3 = Data::load(data.key()).unwrap().unwrap();
        assert_eq!(data3, data2);

        // delete from storage
        assert!(data3.delete());
        assert!(Data::load(data.key()).unwrap().is_none())
    }
}

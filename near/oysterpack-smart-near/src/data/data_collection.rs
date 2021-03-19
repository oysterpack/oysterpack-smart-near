use crate::ErrCode;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use std::collections::HashMap;
use std::marker::PhantomData;

pub type Key = u8;

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Default)]
pub struct DataCollection(HashMap<Key, Vec<u8>>);

impl DataCollection {
    pub fn get<T: BorshDeserialize>(&self, key: Key) -> Option<T> {
        self.0.get(&key).map(|value| {
            T::try_from_slice(value)
                .map_err(|err| ERR_CODE_BORSH_DESERIALIZATION_FAILED.error(err.to_string()))
                .unwrap()
        })
    }

    pub fn insert<T: BorshSerialize>(&mut self, key: Key, value: T) {
        self.0
            .insert(key, value.try_to_vec().expect("borsh serialization failed"));
    }

    pub fn contains_key(&self, key: Key) -> bool {
        self.0.contains_key(&key)
    }

    pub fn remove(&mut self, key: Key) {
        self.0.remove(&key);
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

pub struct Field<T: BorshSerialize + BorshDeserialize>(Key, PhantomData<T>);

impl<T: BorshSerialize + BorshDeserialize> Field<T> {
    pub fn get(&self, data: &DataCollection, key: Key) -> Option<T> {
        data.get::<T>(key)
    }

    pub fn insert(&self, data: &mut DataCollection, key: Key, value: T) {
        data.insert::<T>(key, value);
    }

    pub fn contains_key(&self, data: &DataCollection, key: Key) -> bool {
        data.contains_key(key)
    }

    pub fn remove(&mut self, data: &mut DataCollection, key: Key) {
        data.remove(key);
    }
}

pub const ERR_CODE_BORSH_DESERIALIZATION_FAILED: ErrCode = ErrCode("BORSH_DESERIALIZATION_FAILED");

#[cfg(test)]
mod tests {
    use super::*;

    const A: u8 = 1;
    const B: u8 = 2;

    #[test]
    fn crud() {
        let mut foo = DataCollection::default();
        foo.insert(A, 1_u128);
        assert_eq!(foo.get::<u128>(A).unwrap(), 1);

        foo.insert(B, "foo");
        assert_eq!(foo.get::<String>(B).unwrap(), "foo");

        foo.remove(B);
        assert!(!foo.contains_key(B));
        assert!(foo.get::<String>(B).is_none());
        assert_eq!(foo.get::<u128>(A).unwrap(), 1);

        foo.clear();
        assert_eq!(foo.len(), 0);
        assert!(foo.is_empty());
    }
}

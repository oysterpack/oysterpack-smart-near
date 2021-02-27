use near_sdk::json_types::ValidAccountId;
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env,
};
use std::convert::TryInto;

#[derive(
    BorshDeserialize,
    BorshSerialize,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Debug,
    Ord,
    PartialOrd,
    Default,
)]
pub struct Hash([u8; Hash::LENGTH]);

impl Hash {
    const LENGTH: usize = 32;
}

impl From<[u8; Hash::LENGTH]> for Hash {
    fn from(value: [u8; Hash::LENGTH]) -> Self {
        Self(value)
    }
}

impl From<&[u8]> for Hash {
    fn from(value: &[u8]) -> Self {
        assert!(value.len() > 0, "value cannot be empty");
        let hash = env::sha256(value);
        Self(hash.try_into().unwrap())
    }
}

impl From<&str> for Hash {
    fn from(value: &str) -> Self {
        assert!(value.len() > 0, "value cannot be empty");
        let hash = env::sha256(value.as_bytes());
        Self(hash.try_into().unwrap())
    }
}

impl From<ValidAccountId> for Hash {
    fn from(account_id: ValidAccountId) -> Self {
        Hash::from(account_id.as_ref().as_str())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use near_sdk::test_utils::test_env;

    #[test]
    fn hash_from_string() {
        test_env::setup();
        let data = "Alfio Zappala";
        let hash = Hash::from(data);
        let hash2 = Hash::from(data);
        assert_eq!(hash, hash2);
    }

    #[test]
    #[should_panic(expected = "value cannot be empty")]
    fn hash_from_empty_string() {
        test_env::setup();
        Hash::from("");
    }

    #[test]
    fn hash_from_bytes() {
        test_env::setup();
        let data = "Alfio Zappala II";
        let hash = Hash::from(data.as_bytes());
        let hash2 = Hash::from(data.as_bytes());
        assert_eq!(hash, hash2);
    }

    #[test]
    #[should_panic(expected = "value cannot be empty")]
    fn hash_from_empty_bytes() {
        test_env::setup();
        Hash::from("".as_bytes());
    }
}

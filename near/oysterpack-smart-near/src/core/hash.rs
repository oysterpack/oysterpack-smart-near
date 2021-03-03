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
        Hash::from(value.as_bytes())
    }
}

impl From<ValidAccountId> for Hash {
    fn from(account_id: ValidAccountId) -> Self {
        let hash = env::sha256(account_id.as_ref().as_bytes());
        Self(hash.try_into().unwrap())
    }
}

impl From<&ValidAccountId> for Hash {
    fn from(account_id: &ValidAccountId) -> Self {
        let hash = env::sha256(account_id.as_ref().as_bytes());
        Self(hash.try_into().unwrap())
    }
}

/// Use Case: used as object attribute key
/// - (object_id, attribute_id)
/// - where ULID should be used for attribute_id to avoid collisions
impl From<(&str, u128)> for Hash {
    fn from((k1, k2): (&str, u128)) -> Self {
        Hash::from((k1.as_bytes(), k2))
    }
}

/// Use Case: used as object attribute key
/// - (object_id, attribute_id)
/// - where ULID should be used for attribute_id to avoid collisions
impl From<(ValidAccountId, u128)> for Hash {
    fn from((k1, k2): (ValidAccountId, u128)) -> Self {
        Hash::from((k1.as_ref().as_bytes(), k2))
    }
}

impl From<(&ValidAccountId, u128)> for Hash {
    fn from((k1, k2): (&ValidAccountId, u128)) -> Self {
        Hash::from((k1.as_ref().as_bytes(), k2))
    }
}

/// Use Case: used as object attribute key
/// - (object_id, attribute_id)
/// - where ULID should be used for attribute_id to avoid collisions
impl From<(&[u8], u128)> for Hash {
    fn from((k1, k2): (&[u8], u128)) -> Self {
        // taken from rusty_ulid crate
        fn u128_to_bytes(value: u128) -> [u8; 16] {
            let value = ((value >> 64) as u64, (value & 0xFFFF_FFFF_FFFF_FFFF) as u64);
            [
                ((value.0 >> 56) & 0xFF) as u8,
                ((value.0 >> 48) & 0xFF) as u8,
                ((value.0 >> 40) & 0xFF) as u8,
                ((value.0 >> 32) & 0xFF) as u8,
                ((value.0 >> 24) & 0xFF) as u8,
                ((value.0 >> 16) & 0xFF) as u8,
                ((value.0 >> 8) & 0xFF) as u8,
                (value.0 & 0xFF) as u8,
                ((value.1 >> 56) & 0xFF) as u8,
                ((value.1 >> 48) & 0xFF) as u8,
                ((value.1 >> 40) & 0xFF) as u8,
                ((value.1 >> 32) & 0xFF) as u8,
                ((value.1 >> 24) & 0xFF) as u8,
                ((value.1 >> 16) & 0xFF) as u8,
                ((value.1 >> 8) & 0xFF) as u8,
                (value.1 & 0xFF) as u8,
            ]
        }

        assert!(k1.len() > 0, "k1 cannot be empty");
        let ulid_bytes: [u8; 16] = u128_to_bytes(k2);
        let key: Vec<u8> = [k1, &ulid_bytes].concat();
        Hash::from(key.as_slice())
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

    #[test]
    fn from_object_attribute() {
        test_env::setup();

        let object_id = "object_id";
        let attribute_id: u128 = 1952096956452045446682821566586971355;
        assert_eq!(
            Hash::from((object_id, attribute_id)),
            Hash::from((object_id, attribute_id))
        );
    }
}

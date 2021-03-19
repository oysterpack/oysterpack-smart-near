use crate::ErrCode;
use near_sdk::json_types::ValidAccountId;
use near_sdk::serde::{self, de, Deserialize, Deserializer, Serialize, Serializer};
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env,
};
use std::convert::TryInto;

/// sha256 hashed data
/// - JSON serialization format is Base64 encoded bytes
///
/// The main use case is to use Hash to hash keys for NEAR Trie storage, which provides the following
/// benefits
/// - keys are evenly distributed
/// - key size is constant 32 bytes
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
pub struct Hash(pub [u8; Hash::LENGTH]);

impl Hash {
    pub const LENGTH: usize = 32;
}

impl From<[u8; Hash::LENGTH]> for Hash {
    fn from(value: [u8; Hash::LENGTH]) -> Self {
        Self(value)
    }
}

impl From<&[u8]> for Hash {
    fn from(value: &[u8]) -> Self {
        assert!(!value.is_empty(), "value must not be empty");
        let hash = env::sha256(value);
        Self(hash.try_into().unwrap())
    }
}

impl From<u128> for Hash {
    fn from(value: u128) -> Self {
        Hash::from(u128_to_bytes(value).as_ref())
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

/// Use Case: used as object attribute key
/// - (object_id, attribute_id)
/// - where ULID should be used for attribute_id to avoid collisions
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
        assert!(!k1.is_empty(), "k1 must not be empty");
        let ulid_bytes: [u8; 16] = u128_to_bytes(k2);
        let key: Vec<u8> = [k1, &ulid_bytes].concat();
        Hash::from(key.as_slice())
    }
}

/// use case for using compound key
impl From<(&[u8], &[u8])> for Hash {
    fn from((k1, k2): (&[u8], &[u8])) -> Self {
        assert!(!k1.is_empty(), "k1 must not be empty");
        assert!(!k2.is_empty(), "k2 must not be empty");
        let key: Vec<u8> = [k1, k2].concat();
        Hash::from(key.as_slice())
    }
}

fn u128_to_bytes(value: u128) -> [u8; 16] {
    [
        ((value >> 120) & 0xFF) as u8,
        ((value >> 112) & 0xFF) as u8,
        ((value >> 104) & 0xFF) as u8,
        ((value >> 96) & 0xFF) as u8,
        ((value >> 88) & 0xFF) as u8,
        ((value >> 80) & 0xFF) as u8,
        ((value >> 72) & 0xFF) as u8,
        ((value >> 64) & 0xFF) as u8,
        ((value >> 56) & 0xFF) as u8,
        ((value >> 48) & 0xFF) as u8,
        ((value >> 40) & 0xFF) as u8,
        ((value >> 32) & 0xFF) as u8,
        ((value >> 24) & 0xFF) as u8,
        ((value >> 16) & 0xFF) as u8,
        ((value >> 8) & 0xFF) as u8,
        (value & 0xFF) as u8,
    ]
}

impl Serialize for Hash {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&base64::encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for Hash {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = serde::Deserialize::deserialize(deserializer)?;
        base64::decode(&s)
            .map_err(|err| de::Error::custom(ERR_INVALID_HASH.error(err.to_string())))
            .map(|hash| {
                ERR_INVALID_HASH.assert(
                    || hash.len() == Hash::LENGTH,
                    || "hash length must be 32 bytes",
                );
                Self(
                    hash.try_into()
                        .map_err(|_err| ERR_INVALID_HASH.error(""))
                        .unwrap(),
                )
            })
    }
}

/// Error is used when trying to deserialize HASH from JSON
pub const ERR_INVALID_HASH: ErrCode = ErrCode("INVALID_HASH");

#[cfg(test)]
mod test {
    use super::*;
    use near_sdk::{serde_json, test_utils::test_env};

    #[test]
    fn serde_json() {
        test_env::setup();
        let data = "Alfio Zappala II";
        let hash = Hash::from(data.as_bytes());

        let json = serde_json::to_string(&hash).unwrap();
        println!("json hash: {}", json);
        let hash2: Hash = serde_json::from_str(&json).unwrap();
        assert_eq!(hash, hash2);
    }

    #[test]
    #[should_panic(expected = "[ERR] [INVALID_HASH] hash length must be 32 bytes")]
    fn serde_json_33_char_string() {
        test_env::setup();
        let data = "Alfio Zappala II";
        let hash = Hash::from(data.as_bytes());

        let json = serde_json::to_string(&hash).unwrap();
        let invalid_hash = format!("\"{}", json.split_at(33).0);
        let invalid_hash = format!("{}a\"", invalid_hash);
        println!("{}", invalid_hash);
        let _hash: Hash = serde_json::from_str(&invalid_hash).unwrap();
    }

    #[test]
    #[should_panic(expected = "[ERR] [INVALID_HASH] Invalid last symbol 115")]
    fn serde_json_31_char_string() {
        test_env::setup();
        let data = "Alfio Zappala II";
        let hash = Hash::from(data.as_bytes());

        let json = serde_json::to_string(&hash).unwrap();
        println!("{}", json);
        let invalid_hash = format!("\"{}", json.split_at(2).1);
        println!("{}", invalid_hash);
        let _hash: Hash = serde_json::from_str(&invalid_hash).unwrap();
    }

    #[test]
    fn hash_from_string() {
        test_env::setup();
        let data = "Alfio Zappala";
        let hash = Hash::from(data);
        let hash2 = Hash::from(data);
        assert_eq!(hash, hash2);
    }

    #[test]
    #[should_panic(expected = "value must not be empty")]
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
    #[should_panic(expected = "value must not be empty")]
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

        let attribute_id = b"a";
        assert_eq!(
            Hash::from((object_id.as_bytes(), attribute_id.as_ref())),
            Hash::from((object_id.as_bytes(), attribute_id.as_ref()))
        );
    }
}

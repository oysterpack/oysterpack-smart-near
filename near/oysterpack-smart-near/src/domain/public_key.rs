use crate::asserts::ERR_INVALID;
use crate::Error;
use near_sdk::json_types::Base58PublicKey;
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Deserializer, Serialize, Serializer},
};
use std::fmt::Formatter;
use std::{
    convert::{TryFrom, TryInto},
    fmt::{self, Display},
};

#[derive(BorshDeserialize, BorshSerialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PublicKey {
    ED25519([u8; 32]),
    SECP256K1(([u8; 32], [u8; 32])),
}

impl Serialize for PublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        let key = Base58PublicKey::from(*self);
        Serialize::serialize(&key, serializer)
    }
}

impl<'de> Deserialize<'de> for PublicKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        let key: Base58PublicKey = Deserialize::deserialize(deserializer)?;
        Ok(key.into())
    }
}

impl TryFrom<&[u8]> for PublicKey {
    type Error = Error<String>;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        match value.len() {
            33 if value[0] == 0 => Ok(Self::ED25519((&value[1..]).try_into().unwrap())),
            65 if value[0] == 1 => Ok(Self::SECP256K1((
                (&value[1..33]).try_into().unwrap(),
                (&value[33..]).try_into().unwrap(),
            ))),
            _ => Err(ERR_INVALID.error("invalid public key".to_string())),
        }
    }
}

impl From<PublicKey> for Vec<u8> {
    fn from(key: PublicKey) -> Self {
        match key {
            PublicKey::ED25519(k) => {
                let mut key = Vec::with_capacity(33);
                key.push(0);
                for b in k.iter() {
                    key.push(*b);
                }
                key
            }
            PublicKey::SECP256K1((k1, k2)) => {
                let mut key = Vec::with_capacity(64);
                key.push(1);
                for b in k1.iter() {
                    key.push(*b);
                }
                for b in k2.iter() {
                    key.push(*b);
                }
                key
            }
        }
    }
}

impl From<Base58PublicKey> for PublicKey {
    fn from(key: Base58PublicKey) -> Self {
        key.0.as_slice().try_into().unwrap()
    }
}

impl From<PublicKey> for Base58PublicKey {
    fn from(key: PublicKey) -> Self {
        Self(key.into())
    }
}

impl Display for PublicKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let key = Base58PublicKey::from(*self);
        let s: String = (&key).try_into().map_err(|_| fmt::Error)?;
        s.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::serde_json;

    #[test]
    fn from_vec_ed25519() {
        let key = [0_u8; 33];

        let key: PublicKey = key[..].try_into().unwrap();
        println!("{}", key);
        match key {
            PublicKey::ED25519(_) => {}
            PublicKey::SECP256K1(_) => panic!("expected ED25519"),
        }
    }

    #[test]
    fn from_vec_secp256k1() {
        let key = [1_u8; 65];

        let key: PublicKey = key[..].try_into().unwrap();
        println!("{}", key);
        match key {
            PublicKey::ED25519(_) => panic!("expected ED25519"),
            PublicKey::SECP256K1(_) => {}
        }
    }

    #[test]
    fn json_serde() {
        let key = [1_u8; 65];
        let key: PublicKey = key[..].try_into().unwrap();

        let json = serde_json::to_string(&key).unwrap();
        println!("{}", json);
        let key2: PublicKey = serde_json::from_str(&json).unwrap();
        assert_eq!(key, key2);
    }
}

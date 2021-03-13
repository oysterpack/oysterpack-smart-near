use crate::Hash;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::ValidAccountId;
use near_sdk::{
    base64,
    serde::{self, de::Error, Deserialize, Deserializer, Serialize, Serializer},
};
use std::convert::TryInto;

#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AccountIdHash(pub Hash);

impl AccountIdHash {
    pub fn hash(&self) -> Hash {
        self.0
    }
}

impl From<&str> for AccountIdHash {
    fn from(account_id: &str) -> Self {
        Self(account_id.into())
    }
}

impl From<String> for AccountIdHash {
    fn from(account_id: String) -> Self {
        Self(account_id.as_str().into())
    }
}

impl From<&ValidAccountId> for AccountIdHash {
    fn from(account_id: &ValidAccountId) -> Self {
        Self(account_id.as_ref().as_str().into())
    }
}

impl From<ValidAccountId> for AccountIdHash {
    fn from(account_id: ValidAccountId) -> Self {
        Self(account_id.as_ref().as_str().into())
    }
}

impl Serialize for AccountIdHash {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&base64::encode(&self.0 .0))
    }
}

impl<'de> Deserialize<'de> for AccountIdHash {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = serde::Deserialize::deserialize(deserializer)?;
        base64::decode(&s)
            .map_err(|err| Error::custom(err.to_string()))
            .map(|bytes| Self(Hash(bytes.try_into().unwrap())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::{serde_json, test_utils::test_env};

    #[test]
    fn borsh() {
        test_env::setup();

        let hash = AccountIdHash::from("bob");
        let bytes = hash.try_to_vec().unwrap();

        let hash2 = AccountIdHash::try_from_slice(&bytes).unwrap();
        assert_eq!(hash, hash2);
    }

    #[test]
    fn json() {
        test_env::setup();

        let hash = AccountIdHash::from("bob");
        let json = serde_json::to_string(&hash).unwrap();
        println!("{}", json);

        let hash2: AccountIdHash = serde_json::from_str(&json).unwrap();
        assert_eq!(hash, hash2);
    }
}

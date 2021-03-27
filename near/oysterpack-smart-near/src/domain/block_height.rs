use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env,
    serde::{
        de::{self, Visitor},
        Deserialize, Deserializer, Serialize, Serializer,
    },
    serde_json,
};
use std::{
    fmt::{self, Display, Formatter},
    ops::{Deref, DerefMut},
};

#[derive(
    BorshSerialize,
    BorshDeserialize,
    Debug,
    Clone,
    Copy,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Default,
    Hash,
)]
pub struct BlockHeight(pub u64);

impl BlockHeight {
    pub fn from_env() -> Self {
        Self(env::block_index())
    }

    pub fn value(&self) -> u64 {
        self.0
    }
}

impl From<u64> for BlockHeight {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl Deref for BlockHeight {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for BlockHeight {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Display for BlockHeight {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Serialize for BlockHeight {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        let value = self.0.to_string();
        serializer.serialize_str(&value)
    }
}

impl<'de> Deserialize<'de> for BlockHeight {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(YoctoNearVisitor)
    }
}

struct YoctoNearVisitor;

impl<'de> Visitor<'de> for YoctoNearVisitor {
    type Value = BlockHeight;

    fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("u64 serialized as string")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let value: u64 = serde_json::from_str(v)
            .map_err(|_| de::Error::custom("JSON parsing failed for YoctoNear"))?;
        Ok(BlockHeight(value))
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_str(&v)
    }
}

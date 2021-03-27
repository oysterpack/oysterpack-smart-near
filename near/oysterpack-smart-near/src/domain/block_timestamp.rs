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

/// Current block timestamp, i.e, number of non-leap-nanoseconds since January 1, 1970 0:00:00 UTC.
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
pub struct BlockTimestamp(pub u64);

impl BlockTimestamp {
    pub fn from_env() -> Self {
        Self(env::block_timestamp())
    }

    pub fn value(&self) -> u64 {
        self.0
    }
}

impl From<u64> for BlockTimestamp {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl Deref for BlockTimestamp {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for BlockTimestamp {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Display for BlockTimestamp {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Serialize for BlockTimestamp {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        let value = self.0.to_string();
        serializer.serialize_str(&value)
    }
}

impl<'de> Deserialize<'de> for BlockTimestamp {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(YoctoNearVisitor)
    }
}

struct YoctoNearVisitor;

impl<'de> Visitor<'de> for YoctoNearVisitor {
    type Value = BlockTimestamp;

    fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("u64 serialized as string")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let value: u64 = serde_json::from_str(v)
            .map_err(|_| de::Error::custom("JSON parsing failed for YoctoNear"))?;
        Ok(BlockTimestamp(value))
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_str(&v)
    }
}

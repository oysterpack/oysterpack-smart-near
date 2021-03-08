use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
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
    BorshSerialize, BorshDeserialize, Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Default,
)]
pub struct StorageUsage(pub u64);

impl StorageUsage {
    pub fn value(&self) -> u64 {
        self.0
    }
}

impl From<u64> for StorageUsage {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl Deref for StorageUsage {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for StorageUsage {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Display for StorageUsage {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Serialize for StorageUsage {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        let value = self.0.to_string();
        serializer.serialize_str(&value)
    }
}

impl<'de> Deserialize<'de> for StorageUsage {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(YoctoNearVisitor)
    }
}

struct YoctoNearVisitor;

impl<'de> Visitor<'de> for YoctoNearVisitor {
    type Value = StorageUsage;

    fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("u64 serialized as string")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let value: u64 = serde_json::from_str(v)
            .map_err(|_| de::Error::custom("JSON parsing failed for YoctoNear"))?;
        Ok(StorageUsage(value))
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_str(&v)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn json_serialization() {
        let amount = StorageUsage::from(100);
        let amount_as_json = serde_json::to_string(&amount).unwrap();
        println!("{}", amount_as_json);

        let amount2: StorageUsage = serde_json::from_str(&amount_as_json).unwrap();
        assert_eq!(amount, amount2);
    }
}

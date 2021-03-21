use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{
        de::{self, Visitor},
        Deserialize, Deserializer, Serialize, Serializer,
    },
    serde_json,
};
use std::ops::{Add, AddAssign, Div, Mul, Sub, SubAssign};
use std::{
    fmt::{self, Display, Formatter},
    ops::{Deref, DerefMut},
};

pub const U64_ZERO: U64 = U64(0);

/// Borsh and JSON serializable u64
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
pub struct U64(pub u64);

impl U64 {
    pub fn value(&self) -> u64 {
        self.0
    }
}

impl From<u64> for U64 {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl Deref for U64 {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for U64 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Add<u64> for U64 {
    type Output = Self;

    fn add(self, rhs: u64) -> Self::Output {
        (self.0 + rhs).into()
    }
}

impl AddAssign<u64> for U64 {
    fn add_assign(&mut self, rhs: u64) {
        self.0 += rhs;
    }
}

impl Sub<u64> for U64 {
    type Output = Self;

    fn sub(self, rhs: u64) -> Self::Output {
        (self.0 - rhs).into()
    }
}

impl SubAssign<u64> for U64 {
    fn sub_assign(&mut self, rhs: u64) {
        self.0 -= rhs;
    }
}

impl Mul<u64> for U64 {
    type Output = Self;

    fn mul(self, rhs: u64) -> Self::Output {
        (self.0 * rhs).into()
    }
}

impl Div<u64> for U64 {
    type Output = Self;

    fn div(self, rhs: u64) -> Self::Output {
        (self.0 / rhs).into()
    }
}

impl Display for U64 {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Serialize for U64 {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        let value = self.0.to_string();
        serializer.serialize_str(&value)
    }
}

impl<'de> Deserialize<'de> for U64 {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(U128Visitor)
    }
}

struct U128Visitor;

impl<'de> Visitor<'de> for U128Visitor {
    type Value = U64;

    fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("u64 serialized as string")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let value: u64 = serde_json::from_str(v)
            .map_err(|_| de::Error::custom("JSON parsing failed for U128"))?;
        Ok(U64(value))
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
    use crate::TERA;

    #[test]
    fn json_serialization() {
        let amount = U64::from(TERA);
        let amount_as_json = serde_json::to_string(&amount).unwrap();
        println!("{}", amount_as_json);

        let amount2: U64 = serde_json::from_str(&amount_as_json).unwrap();
        assert_eq!(amount, amount2);
    }
}

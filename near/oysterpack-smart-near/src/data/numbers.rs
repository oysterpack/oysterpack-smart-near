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

pub const U128_ZERO: U128 = U128(0);

#[derive(
    BorshSerialize, BorshDeserialize, Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Default,
)]
pub struct U128(pub u128);

impl U128 {
    pub fn value(&self) -> u128 {
        self.0
    }
}

impl From<u128> for U128 {
    fn from(value: u128) -> Self {
        Self(value)
    }
}

impl Deref for U128 {
    type Target = u128;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for U128 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Add<u128> for U128 {
    type Output = Self;

    fn add(self, rhs: u128) -> Self::Output {
        (self.0 + rhs).into()
    }
}

impl AddAssign<u128> for U128 {
    fn add_assign(&mut self, rhs: u128) {
        self.0 += rhs;
    }
}

impl Sub<u128> for U128 {
    type Output = Self;

    fn sub(self, rhs: u128) -> Self::Output {
        (self.0 - rhs).into()
    }
}

impl SubAssign<u128> for U128 {
    fn sub_assign(&mut self, rhs: u128) {
        self.0 -= rhs;
    }
}

impl Mul<u128> for U128 {
    type Output = Self;

    fn mul(self, rhs: u128) -> Self::Output {
        (self.0 * rhs).into()
    }
}

impl Div<u128> for U128 {
    type Output = Self;

    fn div(self, rhs: u128) -> Self::Output {
        (self.0 / rhs).into()
    }
}

impl Display for U128 {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Serialize for U128 {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        let value = self.0.to_string();
        serializer.serialize_str(&value)
    }
}

impl<'de> Deserialize<'de> for U128 {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(U128Visitor)
    }
}

struct U128Visitor;

impl<'de> Visitor<'de> for U128Visitor {
    type Value = U128;

    fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("u128 serialized as string")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let value: u128 = serde_json::from_str(v)
            .map_err(|_| de::Error::custom("JSON parsing failed for U128"))?;
        Ok(U128(value))
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
    use crate::YOCTO;

    #[test]
    fn json_serialization() {
        let amount = U128::from(YOCTO);
        let amount_as_json = serde_json::to_string(&amount).unwrap();
        println!("{}", amount_as_json);

        let amount2: U128 = serde_json::from_str(&amount_as_json).unwrap();
        assert_eq!(amount, amount2);
    }
}

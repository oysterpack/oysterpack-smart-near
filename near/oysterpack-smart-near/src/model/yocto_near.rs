use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    json_types::U128,
};
use std::{
    fmt::{self, Display, Formatter},
    ops::{Deref, DerefMut},
};

#[derive(
    BorshSerialize, BorshDeserialize, Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Default,
)]
pub struct YoctoNear(u128);

impl From<u128> for YoctoNear {
    fn from(value: u128) -> Self {
        Self(value)
    }
}

impl Deref for YoctoNear {
    type Target = u128;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for YoctoNear {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Display for YoctoNear {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

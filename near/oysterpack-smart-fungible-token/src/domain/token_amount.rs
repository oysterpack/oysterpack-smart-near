use near_sdk::serde::{Deserialize, Serialize};
use oysterpack_smart_near::data::numbers::U128;
use std::ops::{Deref, DerefMut};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
#[serde(crate = "near_sdk::serde")]
pub struct TokenAmount(pub U128);

impl Deref for TokenAmount {
    type Target = u128;

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

impl DerefMut for TokenAmount {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.deref_mut()
    }
}

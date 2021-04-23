use oysterpack_smart_near::data::numbers::U128;
use oysterpack_smart_near::near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
};
use std::fmt::{self, Display, Formatter};
use std::ops::{Add, AddAssign, Deref, DerefMut, Sub, SubAssign};

#[derive(
    BorshDeserialize,
    BorshSerialize,
    Serialize,
    Deserialize,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Default,
)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub struct TokenAmount(pub U128);

impl TokenAmount {
    pub const ZERO: TokenAmount = TokenAmount(U128(0));
}

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

impl From<u128> for TokenAmount {
    fn from(amount: u128) -> Self {
        TokenAmount(amount.into())
    }
}

impl Display for TokenAmount {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Sub<TokenAmount> for TokenAmount {
    type Output = TokenAmount;

    fn sub(self, rhs: TokenAmount) -> Self::Output {
        (*self - *rhs).into()
    }
}

impl SubAssign<TokenAmount> for TokenAmount {
    fn sub_assign(&mut self, rhs: TokenAmount) {
        **self -= *rhs;
    }
}

impl Add<TokenAmount> for TokenAmount {
    type Output = TokenAmount;

    fn add(self, rhs: TokenAmount) -> Self::Output {
        (*self + *rhs).into()
    }
}

impl AddAssign<TokenAmount> for TokenAmount {
    fn add_assign(&mut self, rhs: TokenAmount) {
        **self += *rhs;
    }
}

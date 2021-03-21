use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
};
use oysterpack_smart_near::data::numbers::U64;
use std::fmt::{self, Debug, Display, Formatter};
use std::ops::{Deref, DerefMut};

/// Permissions are modeled as bitflags.
///
/// By default the account supports 64 bits, i.e., permissions, which should be enough to cover most
/// use cases.
///
/// ## Example on how to construct permission bits
/// ```rust
/// use oysterpack_smart_account_management::{Permissions, PERMISSION_ADMIN};
/// let permission: Permissions = (1 << PERMISSION_ADMIN).into();
/// ```
#[derive(
    BorshSerialize,
    BorshDeserialize,
    Serialize,
    Deserialize,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Default,
)]
#[serde(crate = "near_sdk::serde")]
pub struct Permissions(U64);

impl Deref for Permissions {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

impl DerefMut for Permissions {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.deref_mut()
    }
}

impl From<u64> for Permissions {
    fn from(amount: u64) -> Self {
        Permissions(amount.into())
    }
}

impl Display for Permissions {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl Permissions {
    pub fn grant<T: Into<Permissions>>(&mut self, permissions: T) {
        *self.0 |= *permissions.into();
    }

    pub fn revoke<T: Into<Permissions>>(&mut self, permissions: T) {
        *self.0 &= !*permissions.into();
    }

    pub fn revoke_all(&mut self) {
        *self.0 = 0
    }

    /// returns true if all permission bitflags are set
    pub fn contains<T: Into<Permissions>>(&self, permissions: T) -> bool {
        let permissions = permissions.into();
        (*self.0 & *permissions) == *permissions
    }
}

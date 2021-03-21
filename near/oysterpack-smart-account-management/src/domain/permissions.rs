use crate::AccountRoles;
use enumflags2::BitFlags;
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
};
use oysterpack_smart_near::data::numbers::U64;
use std::fmt::{self, Display, Formatter};
use std::ops::{Deref, DerefMut};

/// Permissions are modeled as bitflags.
/// By default the account supports 64 bits, i.e., permissions, which should be enough to cover most
/// use cases.
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
        self.0.fmt(f)
    }
}

impl Permissions {
    pub fn is_admin(&self) -> bool {
        let acl: BitFlags<AccountRoles> = BitFlags::from_bits(*self.0).unwrap();
        acl.contains(AccountRoles::Admin)
    }

    pub fn grant_admin(&mut self) {
        let mut acl: BitFlags<AccountRoles> = BitFlags::from_bits(*self.0).unwrap();
        acl.insert(AccountRoles::Admin);
        *self.0 = acl.bits();
    }

    pub fn revoke_admin(&mut self) {
        let mut acl: BitFlags<AccountRoles> = BitFlags::from_bits(*self.0).unwrap();
        acl.remove(AccountRoles::Admin);
        *self.0 = acl.bits();
    }

    pub fn is_operator(&self) -> bool {
        let acl: BitFlags<AccountRoles> = BitFlags::from_bits(*self.0).unwrap();
        acl.contains(AccountRoles::Operator)
    }

    pub fn grant_operator(&mut self) {
        let mut acl: BitFlags<AccountRoles> = BitFlags::from_bits(*self.0).unwrap();
        acl.insert(AccountRoles::Operator);
        *self.0 = acl.bits();
    }

    pub fn revoke_operator(&mut self) {
        let mut acl: BitFlags<AccountRoles> = BitFlags::from_bits(*self.0).unwrap();
        acl.remove(AccountRoles::Operator);
        *self.0 = acl.bits();
    }

    pub fn grant_access(&mut self, permissions: Permissions) {
        let mut acl: BitFlags<AccountRoles> = BitFlags::from_bits(*self.0).unwrap();
        acl.insert(BitFlags::from_bits(*permissions.0).unwrap());
        *self.0 = acl.bits();
    }

    pub fn revoke_access(&mut self, permissions: Permissions) {
        let mut acl: BitFlags<AccountRoles> = BitFlags::from_bits(*self.0).unwrap();
        acl.remove(BitFlags::from_bits(*permissions).unwrap());
        *self.0 = acl.bits();
    }

    pub fn revoke_all_access(&mut self) {
        *self.0 = 0
    }

    /// returns true if all permission bitflags are set
    pub fn has_access(&self, permissions: Permissions) -> bool {
        let acl: BitFlags<AccountRoles> = BitFlags::from_bits(*self.0).unwrap();
        acl.contains(BitFlags::from_bits(*permissions.0).unwrap())
    }
}

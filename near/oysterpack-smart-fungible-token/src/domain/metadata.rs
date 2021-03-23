use crate::FT_METADATA_SPEC;
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
};
use oysterpack_smart_near::asserts::ERR_INVALID;
use oysterpack_smart_near::Hash;
use std::fmt::{self, Display, Formatter};
use std::ops::Deref;

/// The following fields are immutable once the FT contract is deployed:
/// - [`Metadata::spec`]
/// - [`Metadata::name`]
/// - [`Metadata::symbol`]
/// - [`Metadata::decimals`]
///
/// The following fields can be updated by the contract owner or accounts that have the admin permission:
/// - [`Metadata::icon`]
/// - [`Metadata::reference`]
/// - [`Metadata::reference_hash`]
///
/// NOTE: how optional metadata is stored off-chain is out of scope
#[derive(BorshDeserialize, BorshSerialize, Clone, Deserialize, Serialize, PartialEq, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct Metadata {
    pub spec: Spec,
    pub name: Name,
    pub symbol: Symbol,
    pub decimals: u8,

    pub icon: Option<Icon>,
    pub reference: Option<Reference>,
    /// sha256 hash of the JSON file contained in the reference field. This is to guard against off-chain tampering.
    pub reference_hash: Option<Hash>,
}

impl Metadata {
    pub fn assert_valid(&self) {
        ERR_INVALID.assert(
            || FT_METADATA_SPEC == self.spec.0,
            || format!("`spec` must be `{}`", FT_METADATA_SPEC),
        );

        ERR_INVALID.assert(
            || {
                self.reference.is_some() == self.reference_hash.is_some()
            },
            || "if one of `reference` and `reference_hash` is specified, then both must be specified",
        );
    }
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Deserialize, Serialize, PartialEq, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct Spec(pub String);

impl Deref for Spec {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<&str> for Spec {
    fn from(memo: &str) -> Self {
        Self(memo.to_string())
    }
}

impl From<String> for Spec {
    fn from(memo: String) -> Self {
        Self(memo)
    }
}

impl Display for Spec {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Deserialize, Serialize, PartialEq, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct Name(pub String);

impl Deref for Name {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<&str> for Name {
    fn from(memo: &str) -> Self {
        Self(memo.to_string())
    }
}

impl From<String> for Name {
    fn from(memo: String) -> Self {
        Self(memo)
    }
}

impl Display for Name {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Deserialize, Serialize, PartialEq, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct Symbol(pub String);

impl Deref for Symbol {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<&str> for Symbol {
    fn from(memo: &str) -> Self {
        Self(memo.to_string())
    }
}

impl From<String> for Symbol {
    fn from(memo: String) -> Self {
        Self(memo)
    }
}

impl Display for Symbol {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A small image associated with this token.
/// Must be a data URL, to help consumers display it quickly while protecting user data.
///
/// ### Recommendations
/// - use optimized SVG, which can result in high-resolution images with only 100s of bytes of storage cost.
///   - Note that these storage costs are incurred to the token owner/deployer, but that querying these
///     icons is a very cheap & cacheable read operation for all consumers of the contract and the RPC
///     nodes that serve the data.
/// - create icons that will work well with both light-mode and dark-mode websites by either using
/// middle-tone color schemes, or by embedding media queries in the SVG.
#[derive(BorshDeserialize, BorshSerialize, Clone, Deserialize, Serialize, PartialEq, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct Icon(pub String);

impl Deref for Icon {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<&str> for Icon {
    fn from(memo: &str) -> Self {
        Self(memo.to_string())
    }
}

impl From<String> for Icon {
    fn from(memo: String) -> Self {
        Self(memo)
    }
}

impl Display for Icon {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A link to a valid JSON file containing various keys offering supplementary details on the token.
/// Example: "/ipfs/QmdmQXB2mzChmMeKY47C43LxUdg1NDJ5MWcKMKxDu7RgQm", "https://example.com/token.json", etc.
///
///
/// If the information given in this document conflicts with the on-chain attributes, then the values
/// in reference shall be considered the source of truth.
#[derive(BorshDeserialize, BorshSerialize, Clone, Deserialize, Serialize, PartialEq, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct Reference(pub String);

impl Deref for Reference {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<&str> for Reference {
    fn from(memo: &str) -> Self {
        Self(memo.to_string())
    }
}

impl From<String> for Reference {
    fn from(memo: String) -> Self {
        Self(memo)
    }
}

impl Display for Reference {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

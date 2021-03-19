use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
};
use oysterpack_smart_near::Hash;
use std::fmt::{self, Display, Formatter};
use std::ops::Deref;

#[derive(BorshDeserialize, BorshSerialize, Clone, Deserialize, Serialize, PartialEq, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct FungibleTokenMetadata {
    pub spec: String,
    pub name: String,
    pub symbol: String,
    pub icon: Option<String>,
    pub reference: Option<String>,
    pub reference_hash: Option<Hash>,
    pub decimals: u8,
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Deserialize, Serialize, PartialEq, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct Spec(String);

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
pub struct Name(String);

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
pub struct Symbol(String);

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

#[derive(BorshDeserialize, BorshSerialize, Clone, Deserialize, Serialize, PartialEq, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct Icon(String);

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

#[derive(BorshDeserialize, BorshSerialize, Clone, Deserialize, Serialize, PartialEq, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct Reference(String);

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

use near_sdk::serde::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};
use std::ops::Deref;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(crate = "near_sdk::serde")]
pub struct Memo(pub String);

impl Deref for Memo {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<&str> for Memo {
    fn from(memo: &str) -> Self {
        Self(memo.to_string())
    }
}

impl From<String> for Memo {
    fn from(memo: String) -> Self {
        Self(memo)
    }
}

impl Display for Memo {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

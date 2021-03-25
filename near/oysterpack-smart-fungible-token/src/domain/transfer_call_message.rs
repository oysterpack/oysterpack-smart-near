use oysterpack_smart_near::near_sdk::serde::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};
use std::ops::Deref;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub struct TransferCallMessage(pub String);

impl Deref for TransferCallMessage {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<&str> for TransferCallMessage {
    fn from(memo: &str) -> Self {
        Self(memo.to_string())
    }
}

impl From<String> for TransferCallMessage {
    fn from(memo: String) -> Self {
        Self(memo)
    }
}

impl Display for TransferCallMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

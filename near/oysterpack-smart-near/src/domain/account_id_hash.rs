use crate::Hash;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::ValidAccountId;

#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AccountIdHash(pub Hash);

impl AccountIdHash {
    pub fn hash(&self) -> Hash {
        self.0
    }
}

impl From<&str> for AccountIdHash {
    fn from(account_id: &str) -> Self {
        Self(account_id.into())
    }
}

impl From<&ValidAccountId> for AccountIdHash {
    fn from(account_id: &ValidAccountId) -> Self {
        Self(account_id.as_ref().as_str().into())
    }
}

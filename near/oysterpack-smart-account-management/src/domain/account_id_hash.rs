use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use oysterpack_smart_near::Hash;

/// Used as key to store account data
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

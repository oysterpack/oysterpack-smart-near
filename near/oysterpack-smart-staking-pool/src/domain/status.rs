use oysterpack_smart_near::near_sdk::serde::export::Formatter;
use oysterpack_smart_near::near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
};
use std::fmt::{self, Debug, Display};

#[derive(
    BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug,
)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub enum Status {
    /// While offline, accounts can still stake, but the funds are held until are held until the
    /// staking pool goes online
    /// - when the pool goes back online, then the staked funds are staked
    Offline(OfflineReason),
    /// the pool is actively staking
    Online,
}

impl Status {
    pub fn is_online(&self) -> bool {
        match self {
            Status::Offline(_) => false,
            Status::Online => true,
        }
    }
}

#[derive(
    BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug,
)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub enum OfflineReason {
    Stopped,
    StakeActionFailed,
}

impl Display for OfflineReason {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Debug::fmt(self, f)
    }
}

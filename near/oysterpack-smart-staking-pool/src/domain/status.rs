use oysterpack_smart_near::near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
};

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
    Paused,
    StakeActionFailed,
}

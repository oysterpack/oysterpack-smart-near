use oysterpack_smart_near::domain::{EpochHeight, YoctoNear};
use oysterpack_smart_near::near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
};

#[derive(
    BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Default,
)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub struct Treasury {
    /// NEAR deposited into the treasury is staked and stake earnings are used to boost the
    /// staking pool yield
    /// - on a scheduled basis, the staking rewards earned by the treasury are burned, which effectively
    ///   distributes the earnings to all current stake holders
    pub treasury_balance: YoctoNear,
    pub last_treasury_distribution: EpochHeight,
}

impl Treasury {
    /// treasury earnings are distributed every 180 epochs, which is roughly every 90 days
    pub const DISTRIBUTION_EPOCH_FREQUENCY: u8 = 180;
}

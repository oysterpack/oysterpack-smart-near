use oysterpack_smart_near::domain::{EpochHeight, YoctoNear};
use oysterpack_smart_near::near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
};

/// specified the unstaked balance and when it is available for withdrawal
#[derive(
    BorshDeserialize,
    BorshSerialize,
    Serialize,
    Deserialize,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Ord,
    PartialOrd,
    Eq,
)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub struct UnstakedBalance {
    pub available_on: EpochHeight,
    pub balance: YoctoNear,
}

impl UnstakedBalance {
    pub fn new(balance: YoctoNear, available_on: EpochHeight) -> Self {
        Self {
            balance,
            available_on,
        }
    }
}

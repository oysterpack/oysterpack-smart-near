use crate::domain::{BlockHeight, BlockTimestamp, EpochHeight};
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
};

#[derive(
    BorshSerialize, BorshDeserialize, Deserialize, Serialize, Clone, Copy, Debug, PartialEq,
)]
#[serde(crate = "near_sdk::serde")]
pub struct BlockTime {
    pub timestamp: BlockTimestamp,
    pub height: BlockHeight,
    pub epoch: EpochHeight,
}

impl BlockTime {
    pub fn from_env() -> Self {
        Self {
            timestamp: BlockTimestamp::from_env(),
            height: BlockHeight::from_env(),
            epoch: EpochHeight::from_env(),
        }
    }
}

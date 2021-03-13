use crate::domain::{BlockHeight, BlockTimestamp, EpochHeight};
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env,
    serde::{Deserialize, Serialize},
};

/// Expiration can be set on the epoch, block, or timestamp.
///
/// The time is considered expired if the current time is after the specified expiration.
/// Current block timestamp, i.e, number of non-leap-nanoseconds since January 1, 1970 0:00:00 UTC.
#[derive(
    BorshSerialize,
    BorshDeserialize,
    Serialize,
    Deserialize,
    Debug,
    Clone,
    Copy,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
)]
#[serde(crate = "near_sdk::serde")]
pub enum Expiration {
    Epoch(EpochHeight),
    Block(BlockHeight),
    Timestamp(BlockTimestamp),
}

impl Expiration {
    pub fn expired(&self) -> bool {
        match *self {
            Expiration::Epoch(epoch) => env::epoch_height() > epoch.value(),
            Expiration::Block(block) => env::block_index() > block.value(),
            Expiration::Timestamp(timestamp) => env::block_timestamp() > timestamp.value(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oysterpack_smart_near_test::*;

    #[test]
    fn epoch_expiration() {
        let mut ctx = new_context("bob");

        ctx.epoch_height = 1000;
        testing_env!(ctx.clone());

        assert!(!Expiration::Epoch(1000.into()).expired());
        assert!(Expiration::Epoch(999.into()).expired());
    }

    #[test]
    fn block_expiration() {
        let mut ctx = new_context("bob");

        ctx.block_index = 1000;
        testing_env!(ctx.clone());

        assert!(!Expiration::Block(1000.into()).expired());
        assert!(Expiration::Block(999.into()).expired());
    }

    #[test]
    fn timestamp_expiration() {
        let mut ctx = new_context("bob");

        ctx.block_timestamp = 1000;
        testing_env!(ctx.clone());

        assert!(!Expiration::Timestamp(1000.into()).expired());
        assert!(Expiration::Timestamp(999.into()).expired());
    }
}

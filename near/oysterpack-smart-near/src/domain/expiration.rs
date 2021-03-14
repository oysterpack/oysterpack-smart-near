use crate::domain::{BlockHeight, BlockTimestamp, EpochHeight};
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env,
    serde::{Deserialize, Serialize},
};
use std::fmt::{self, Display, Formatter};

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

impl Display for Expiration {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match *self {
            Expiration::Epoch(expiration) => write!(f, "{:?}", expiration),
            Expiration::Block(expiration) => write!(f, "{:?}", expiration),
            Expiration::Timestamp(expiration) => write!(f, "{:?}", expiration),
        }
    }
}

impl From<ExpirationDuration> for Expiration {
    fn from(duration: ExpirationDuration) -> Self {
        match duration {
            ExpirationDuration::Epochs(duration) => {
                Expiration::Epoch((env::epoch_height() + duration as u64).into())
            }
            ExpirationDuration::Blocks(duration) => {
                Expiration::Block((env::block_index() + duration as u64).into())
            }
            ExpirationDuration::Seconds(duration) => Expiration::Timestamp(
                (env::block_timestamp() + (1_000_000_000 * duration as u64)).into(),
            ),
        }
    }
}

impl From<ExpirationSetting> for Expiration {
    fn from(settings: ExpirationSetting) -> Self {
        match settings {
            ExpirationSetting::Absolute(settings) => settings,
            ExpirationSetting::Relative(settings) => settings.into(),
        }
    }
}

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
pub enum ExpirationDuration {
    Epochs(u32),
    Blocks(u32),
    Seconds(u32),
}

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
pub enum ExpirationSetting {
    Absolute(Expiration),
    Relative(ExpirationDuration),
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::serde_json;
    use oysterpack_smart_near_test::*;

    #[test]
    fn epoch_expiration() {
        let mut ctx = new_context("bob");

        ctx.epoch_height = 1000;
        testing_env!(ctx.clone());

        assert!(!Expiration::Epoch(1000.into()).expired());
        assert!(Expiration::Epoch(999.into()).expired());

        println!(
            "{}",
            serde_json::to_string(&Expiration::Epoch(1000.into())).unwrap()
        );
    }

    #[test]
    fn block_expiration() {
        let mut ctx = new_context("bob");

        ctx.block_index = 1000;
        testing_env!(ctx.clone());

        assert!(!Expiration::Block(1000.into()).expired());
        assert!(Expiration::Block(999.into()).expired());

        println!(
            "{}",
            serde_json::to_string(&Expiration::Block(1000.into())).unwrap()
        );
    }

    #[test]
    fn timestamp_expiration() {
        let mut ctx = new_context("bob");

        ctx.block_timestamp = 1000;
        testing_env!(ctx.clone());

        assert!(!Expiration::Timestamp(1000.into()).expired());
        assert!(Expiration::Timestamp(999.into()).expired());

        println!(
            "{}",
            serde_json::to_string(&Expiration::Timestamp(1000.into())).unwrap()
        );
    }
}

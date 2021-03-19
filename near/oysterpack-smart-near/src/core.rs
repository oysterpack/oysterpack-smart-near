pub mod asserts;
mod errors;
pub mod eventbus;
mod hash;
mod log_events;

pub use errors::*;
pub use hash::*;
pub use log_events::*;
use near_sdk::json_types::ValidAccountId;

/// YOCTO = 10^24
pub const YOCTO: u128 = 1_000_000_000_000_000_000_000_000;

/// TERA = 10^12
pub const TERA: u128 = 1_000_000_000_000;

use std::convert::TryFrom;

/// Panics if `account_id` is not valid
pub fn to_valid_account_id(account_id: &str) -> ValidAccountId {
    ValidAccountId::try_from(account_id).unwrap()
}

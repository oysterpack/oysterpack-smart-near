pub mod asserts;
mod errors;
pub mod eventbus;
mod hash;
mod log_events;
mod promise;

pub use errors::*;
pub use hash::*;
pub use log_events::*;
pub use promise::*;

use near_sdk::json_types::ValidAccountId;

/// YOCTO = 10^24
pub const YOCTO: u128 = 1_000_000_000_000_000_000_000_000;

/// TERA = 10^12
pub const TERA: u64 = 1_000_000_000_000;

use std::convert::TryFrom;

/// Panics if `account_id` is not valid
pub fn to_valid_account_id(account_id: &str) -> ValidAccountId {
    match ValidAccountId::try_from(account_id) {
        Ok(account_id) => account_id,
        Err(_) => {
            ERR_INVALID_ACCOUNT_ID.panic_with_message(account_id);
            unreachable!();
        }
    }
}

pub const ERR_INVALID_ACCOUNT_ID: ErrorConst = ErrorConst(ErrCode("INVALID_ACCOUNT_ID"), "");

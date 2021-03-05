mod eventbus;
mod hash;

pub use eventbus::*;
pub use hash::*;

use uint::construct_uint;

/// YOCTO = 10^24
pub const YOCTO: u128 = 1_000_000_000_000_000_000_000_000;

/// TERA = 10^12
pub const TERA: u128 = 1_000_000_000_000;

construct_uint! {
    /// 256-bit unsigned integer.
    pub struct U256(4);
}

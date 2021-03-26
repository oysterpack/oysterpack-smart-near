use oysterpack_smart_near::{
    domain::YoctoNear,
    near_sdk::serde::{Deserialize, Serialize},
};

/// Tracks account storage balance
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub struct StorageBalance {
    /// total NEAR funds that is purposed to pay for account storage
    pub total: YoctoNear,
    /// the amount of NEAR funds that are available for withdrawal from the account's storage balance
    pub available: YoctoNear,
}

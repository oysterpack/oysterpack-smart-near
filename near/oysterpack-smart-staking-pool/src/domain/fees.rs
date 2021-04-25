use oysterpack_smart_near::domain::BasisPoints;
use oysterpack_smart_near::near_sdk::serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Default)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub struct Fees {
    /// fee charged when staking funds
    pub staking_fee: BasisPoints,
    /// fee charged based on earnings - this aligns the owner's financial interests and incentives with the stakers
    pub earnings_fee: BasisPoints,
}

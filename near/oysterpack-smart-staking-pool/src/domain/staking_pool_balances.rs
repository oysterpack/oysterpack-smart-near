use crate::components::staking_pool::State;
use oysterpack_smart_contract::ContractNearBalances;
use oysterpack_smart_near::{
    domain::YoctoNear,
    near_sdk::serde::{Deserialize, Serialize},
};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Eq, PartialEq)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub struct StakingPoolBalances {
    /// total NEAR funds that have been staked and confirmed
    pub total_staked: YoctoNear,
    /// tracks in flight funds that have been staked but not yet confirmed
    pub staked: YoctoNear,
    /// tracks in flight funds that have been unstaked but not yet confirmed
    pub unstaked: YoctoNear,

    /// total unstaked funds that have not yet been withdrawn
    /// - includes locked and unlocked funds
    /// - excludes [`StakingPoolBalances::unstaked_liquidity`]
    pub total_unstaked: YoctoNear,
    /// unstaked funds that can be withdrawn from liquidity added by staking
    pub unstaked_liquidity: YoctoNear,
}

impl StakingPoolBalances {
    pub fn load() -> Self {
        Self {
            total_staked: ContractNearBalances::near_balance(State::TOTAL_STAKED_BALANCE),
            staked: ContractNearBalances::near_balance(State::STAKED_BALANCE),
            unstaked: ContractNearBalances::near_balance(State::UNSTAKED_BALANCE),
            total_unstaked: ContractNearBalances::near_balance(State::TOTAL_UNSTAKED_BALANCE),
            unstaked_liquidity: ContractNearBalances::near_balance(State::UNSTAKED_LIQUIDITY_POOL),
        }
    }
}

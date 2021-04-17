use crate::components::staking_pool::{StakingPoolComponent, State};
use oysterpack_smart_contract::ContractNearBalances;
use oysterpack_smart_near::{
    domain::YoctoNear,
    near_sdk::serde::{Deserialize, Serialize},
};

/// Staking Pool Contract NEAR Balances
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Eq, PartialEq)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub struct StakingPoolBalances {
    /// total NEAR funds that have been staked and confirmed
    pub total_staked: YoctoNear,

    /// total unstaked funds that have not yet been withdrawn
    /// - includes locked and unlocked funds
    /// - excludes [`StakingPoolBalances::unstaked_liquidity`]
    pub total_unstaked: YoctoNear,
    /// unstaked funds that can be withdrawn from liquidity added by staking
    pub unstaked_liquidity: YoctoNear,

    pub treasury_balance: YoctoNear,
}

impl StakingPoolBalances {
    pub fn load() -> Self {
        let state = StakingPoolComponent::state();
        Self {
            total_staked: state.total_staked_balance,
            total_unstaked: ContractNearBalances::near_balance(State::TOTAL_UNSTAKED_BALANCE),
            unstaked_liquidity: ContractNearBalances::near_balance(State::UNSTAKED_LIQUIDITY_POOL),
            treasury_balance: state.treasury_balance,
        }
    }
}

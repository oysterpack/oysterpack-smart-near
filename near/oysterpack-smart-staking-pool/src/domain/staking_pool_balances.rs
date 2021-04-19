use crate::components::staking_pool::{StakingPoolComponent, State};
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

    /// used to track the treasury STAKE NEAR value
    /// - staking rewards earned by the treasury are distributed as dividends
    /// - balance gets updated when funds are staked
    pub treasury_balance: YoctoNear,

    pub current_contract_managed_total_balance: YoctoNear,
    /// used to track transaction fee earnings
    /// - transaction gas earnings are staked into the pool, which effectively increases STAKE value
    pub last_contract_managed_total_balance: YoctoNear,
    /// [`StakingPoolBalances::last_contract_managed_total_balance`] - [`StakingPoolBalances::current_contract_managed_total_balance`]
    pub transaction_fee_earnings: YoctoNear,
}

impl StakingPoolBalances {
    pub fn load() -> Self {
        let state = StakingPoolComponent::state();
        let current_contract_managed_total_balance =
            State::contract_managed_total_balance_in_view_mode();
        Self {
            total_staked: State::total_staked_balance(),
            total_unstaked: State::total_unstaked_balance(),
            unstaked_liquidity: State::liquidity(),
            treasury_balance: state.treasury_balance,

            current_contract_managed_total_balance,
            last_contract_managed_total_balance: state.last_contract_managed_total_balance,
            transaction_fee_earnings: current_contract_managed_total_balance
                .saturating_sub(*state.last_contract_managed_total_balance)
                .into(),
        }
    }
}

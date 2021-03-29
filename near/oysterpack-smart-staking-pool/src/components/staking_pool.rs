use crate::{StakeAccount, StakeAccountBalances, StakeAmount, StakingPool, UnstakedBalances};
use oysterpack_smart_account_management::{
    components::account_management::AccountManagementComponent, AccountRepository,
    StorageManagement,
};
use oysterpack_smart_contract::components::contract_metrics::ContractMetricsComponent;
use oysterpack_smart_contract::{
    components::contract_ownership::ContractOwnershipComponent, BalanceId, ContractMetrics,
    ContractNearBalances, ContractOwnership, ContractStorageUsageCosts,
};
use oysterpack_smart_fungible_token::{
    components::fungible_token::FungibleTokenComponent, FungibleToken, TokenAmount,
};
use oysterpack_smart_near::data::numbers::U256;
use oysterpack_smart_near::domain::ZERO_NEAR;
use oysterpack_smart_near::near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env,
    json_types::ValidAccountId,
};
use oysterpack_smart_near::{
    component::{Component, ComponentState, Deploy},
    domain::{EpochHeight, PublicKey, YoctoNear},
    YOCTO,
};
use teloc::*;

type StakeAccountData = UnstakedBalances;

pub struct StakingPoolComponent {
    account_manager: AccountManagementComponent<StakeAccountData>,
    stake: FungibleTokenComponent<StakeAccountData>,
    contract_ownership: ContractOwnershipComponent,
    contract_metrics: ContractMetricsComponent,
}

#[inject]
impl StakingPoolComponent {
    pub fn new(
        account_manager: AccountManagementComponent<StakeAccountData>,
        stake: FungibleTokenComponent<StakeAccountData>,
    ) -> Self {
        Self {
            account_manager,
            stake,
            contract_ownership: ContractOwnershipComponent,
            contract_metrics: ContractMetricsComponent,
        }
    }
}

impl Component for StakingPoolComponent {
    type State = State;
    const STATE_KEY: u128 = 1954854625400732566949949714395710108;
}

#[derive(BorshSerialize, BorshDeserialize, Clone, PartialEq, Debug)]
pub struct State {
    /// validator public key used for staking
    stake_public_key: PublicKey,
}

impl Deploy for StakingPoolComponent {
    type Config = StakingPoolComponentConfig;

    fn deploy(config: Self::Config) {
        let state = State {
            stake_public_key: config.stake_public_key,
        };
        let state = Self::new_state(state);
        state.save();
    }
}

pub struct StakingPoolComponentConfig {
    pub stake_public_key: PublicKey,

    pub contract_earnings_profit_sharing_percent: u8,
}

impl StakingPool for StakingPoolComponent {
    fn ops_stake_balance(&self, account_id: ValidAccountId) -> Option<StakeAccountBalances> {
        self.account_manager
            .load_account(account_id.as_ref().as_str())
            .map(|(near_data, data)| {
                let (unstaked_balance, unstaked_available_balance) =
                    data.as_ref().map_or((ZERO_NEAR, ZERO_NEAR), |data| {
                        (
                            data.total_unstaked_balance(),
                            data.unstaked_available_balance(),
                        )
                    });
                let stake_token_balance = self.stake.ft_balance_of(account_id);
                let staked_near_balance = self.stake_near_value(stake_token_balance);
                let storage_balance_bounds = self.account_manager.storage_balance_bounds();

                StakeAccountBalances {
                    total: unstaked_balance + staked_near_balance,
                    available: near_data
                        .storage_balance(storage_balance_bounds.min)
                        .available
                        + unstaked_available_balance,
                    staked: staked_near_balance,
                    unstaked: data.map_or(UnstakedBalances::Zero, |data| {
                        data.remove_available_balances()
                    }),
                }
            })
    }

    fn ops_stake(&mut self, amount: Option<StakeAmount>) -> StakeAccountBalances {
        unimplemented!()
    }

    fn ops_unstake(&mut self, amount: Option<YoctoNear>) -> StakeAccountBalances {
        unimplemented!()
    }

    fn ops_stake_token_value(&self) -> YoctoNear {
        let total_staked_balance = env::account_locked_balance();
        if total_staked_balance == 0 {
            YOCTO.into()
        } else {
            let value = (total_staked_balance / *self.stake.ft_total_supply()).into();
            // since we are rounding down, we need to make sure that the value of 1 STAKE is at least 1 NEAR
            std::cmp::max(value, YOCTO.into())
        }
    }

    fn ops_stake_available_liquidity(&self) -> YoctoNear {
        ContractNearBalances::load_near_balances()
            .get(&StakingPoolComponent::UNSTAKED_LIQUIDITY)
            .cloned()
            .unwrap_or(ZERO_NEAR)
    }
}

impl StakingPoolComponent {
    /// when there is an unstaked balance, delegators add liquidity when they deposit funds to stake
    /// - this enables accounts to withdraw against the unstaked liquidity on a first come first serve basis
    pub const UNSTAKED_LIQUIDITY: BalanceId = BalanceId(0);
    pub const TOTAL_UNSTAKED_BALANCE: BalanceId = BalanceId(1);

    fn state() -> ComponentState<State> {
        Self::load_state().expect("component has not been deployed")
    }

    fn stake(&mut self, account: &str, amount: YoctoNear) -> StakeAccountBalances {
        unimplemented!()
    }

    fn stake_near_value(&self, stake: TokenAmount) -> YoctoNear {
        if *stake == 0 {
            return ZERO_NEAR;
        }
        let total_staked_balance = env::account_locked_balance();

        (U256::from(total_staked_balance) * U256::from(*stake)
            / U256::from(*self.stake.ft_total_supply()))
        .as_u128()
        .into()
    }
}

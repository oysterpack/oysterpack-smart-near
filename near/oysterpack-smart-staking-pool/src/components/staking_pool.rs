use crate::{StakeAccountBalances, StakeAmount, StakingPool, UnstakedBalances};
use oysterpack_smart_account_management::{
    components::account_management::AccountManagementComponent, AccountDataObject,
    AccountNearDataObject, AccountRepository, StorageManagement, ERR_ACCOUNT_NOT_REGISTERED,
};
use oysterpack_smart_contract::components::contract_metrics::ContractMetricsComponent;
use oysterpack_smart_contract::{
    components::contract_ownership::ContractOwnershipComponent, BalanceId, ContractNearBalances,
};
use oysterpack_smart_fungible_token::{
    components::fungible_token::FungibleTokenComponent, FungibleToken, TokenAmount,
};
use oysterpack_smart_near::asserts::{assert_near_attached, ERR_INSUFFICIENT_FUNDS};
use oysterpack_smart_near::data::numbers::U256;
use oysterpack_smart_near::domain::ZERO_NEAR;
use oysterpack_smart_near::near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env,
    json_types::ValidAccountId,
};
use oysterpack_smart_near::{
    component::{Component, ComponentState, Deploy},
    domain::{PublicKey, YoctoNear},
    YOCTO,
};

type StakeAccountData = UnstakedBalances;

pub struct StakingPoolComponent {
    account_manager: AccountManagementComponent<StakeAccountData>,
    stake: FungibleTokenComponent<StakeAccountData>,
    contract_ownership: ContractOwnershipComponent,
    contract_metrics: ContractMetricsComponent,
}

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
            .load_account_data(account_id.as_ref().as_str())
            .map(|data| {
                let staked_near_balance = {
                    let stake_token_balance = self.stake.ft_balance_of(account_id);
                    self.stake_near_value(stake_token_balance)
                };
                let unstaked_balance = data.total_unstaked_balance();
                let unstaked_available_balance = data.unstaked_available_balance();

                StakeAccountBalances {
                    total: staked_near_balance + unstaked_balance - unstaked_available_balance,
                    available: unstaked_available_balance,
                    staked: staked_near_balance,
                    unstaked: data.remove_available_balances(),
                }
            })
    }

    fn ops_stake(&mut self, amount: Option<StakeAmount>) -> StakeAccountBalances {
        assert_near_attached("at least 1 yoctoNEAR is required");
        let account_id = env::predecessor_account_id();

        let stake_amount = match amount {
            None => {
                ERR_ACCOUNT_NOT_REGISTERED
                    .assert(|| self.account_manager.account_exists(&account_id));
                env::attached_deposit().into()
            }
            Some(amount) => {
                self.update_account_balances_for_restaking(amount) + env::attached_deposit()
            }
        };

        unimplemented!()
    }

    fn ops_unstake(&mut self, amount: Option<YoctoNear>) -> StakeAccountBalances {
        unimplemented!()
    }

    fn ops_stake_withdraw(&mut self, amount: Option<YoctoNear>) -> StakeAccountBalances {
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

    /// tries to restake the specified amount from current unstaked and account storage available balances
    /// - if there are sufficient funds then the balances will be updated
    /// - restaked funds will be taken from the balances using the following precedence:
    ///   1. from unstaked balances starting with the most recently staked
    ///   2. account storage available balance
    /// - [`StakingPoolComponent::TOTAL_UNSTAKED_BALANCE`] near balance will be updated as well
    ///
    /// ## Panics
    /// - if there are insufficient funds
    fn update_account_balances_for_restaking(&mut self, amount: StakeAmount) -> YoctoNear {
        fn restake_all_unstaked(
            mut stake_account: Option<AccountDataObject<StakeAccountData>>,
        ) -> YoctoNear {
            let total_unstaked_balance = stake_account.map_or(ZERO_NEAR, |mut account| {
                let total_unstaked_balance = account.total_unstaked_balance();
                account.delete();
                total_unstaked_balance
            });
            if *total_unstaked_balance > 0 {
                ContractNearBalances::decr_balance(
                    StakingPoolComponent::TOTAL_UNSTAKED_BALANCE,
                    total_unstaked_balance,
                );
            }
            total_unstaked_balance
        }

        let (mut near_account, mut stake_account) = self
            .account_manager
            .registered_account(&env::predecessor_account_id());

        match amount {
            StakeAmount::All => {
                let account_storage_available_balance = {
                    let storage_balance_min = self.account_manager.storage_balance_bounds().min;
                    let account_storage_available_balance =
                        near_account.storage_balance(storage_balance_min).available;
                    near_account.dec_near_balance(account_storage_available_balance);
                    near_account.save();
                    account_storage_available_balance
                };

                let total_unstaked_balance = restake_all_unstaked(stake_account);
                account_storage_available_balance + total_unstaked_balance
            }
            StakeAmount::AllUnstaked => restake_all_unstaked(stake_account),
            StakeAmount::Total(amount) => {
                let unstaked_balance = {
                    let unstaked_balance = stake_account.map_or(ZERO_NEAR, |mut balance| {
                        let (mut updated_balance, amount) = balance.restake(amount);
                        if updated_balance.is_zero() {
                            balance.delete();
                        } else {
                            balance.save();
                        }
                        amount
                    });
                    if *unstaked_balance > 0 {
                        ContractNearBalances::decr_balance(
                            StakingPoolComponent::TOTAL_UNSTAKED_BALANCE,
                            unstaked_balance,
                        );
                    }
                    unstaked_balance
                };

                let gap = amount - unstaked_balance;
                if gap > ZERO_NEAR {
                    let storage_balance_min = self.account_manager.storage_balance_bounds().min;
                    let account_storage_available_balance =
                        near_account.storage_balance(storage_balance_min).available;
                    ERR_INSUFFICIENT_FUNDS.assert(|| account_storage_available_balance >= gap);
                    near_account.dec_near_balance(account_storage_available_balance - gap);
                    near_account.save();
                }
                amount
            }
            StakeAmount::Unstaked(amount) => {
                ERR_INSUFFICIENT_FUNDS.assert(|| stake_account.is_some());
                let mut stake_account = stake_account.unwrap();
                let (updated_balance, restake_amount) = stake_account.restake(amount);
                ERR_INSUFFICIENT_FUNDS.assert(|| restake_amount == amount);
                if updated_balance.is_zero() {
                    stake_account.delete();
                } else {
                    **stake_account = updated_balance;
                    stake_account.save();
                }
                ContractNearBalances::decr_balance(
                    StakingPoolComponent::TOTAL_UNSTAKED_BALANCE,
                    amount,
                );
                amount
            }
        }
    }

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

use crate::{
    StakeAccountBalances, StakeActionCallback, StakingPool, StakingPoolOperator, UnstakedBalances,
    ERR_STAKE_ACTION_FAILED,
};
use oysterpack_smart_account_management::{
    components::account_management::AccountManagementComponent, AccountRepository,
    StorageManagement,
};
use oysterpack_smart_contract::components::contract_metrics::ContractMetricsComponent;
use oysterpack_smart_contract::{
    components::contract_ownership::ContractOwnershipComponent, BalanceId, ContractNearBalances,
};
use oysterpack_smart_fungible_token::{
    components::fungible_token::FungibleTokenComponent, FungibleToken, TokenAmount, TokenService,
};
use oysterpack_smart_near::asserts::assert_near_attached;
use oysterpack_smart_near::data::numbers::U256;
use oysterpack_smart_near::data::Object;
use oysterpack_smart_near::domain::{
    ActionType, ByteLen, Gas, SenderIsReceiver, TransactionResource, ZERO_NEAR,
};
use oysterpack_smart_near::near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env, is_promise_success,
    json_types::ValidAccountId,
    Promise,
};
use oysterpack_smart_near::{
    component::{Component, ComponentState, Deploy},
    domain::{PublicKey, YoctoNear},
    json_function_callback, TERA, YOCTO,
};

type StakeAccountData = UnstakedBalances;

pub struct StakingPoolComponent {
    account_manager: AccountManagementComponent<StakeAccountData>,
    stake_token: FungibleTokenComponent<StakeAccountData>,
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
            stake_token: stake,
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
                    let stake_token_balance = self.stake_token.ft_balance_of(account_id);
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

    fn ops_stake(&mut self) -> StakeAccountBalances {
        assert_near_attached("deposit is required to stake");
        let mut account = self
            .account_manager
            .registered_account_near_data(&env::predecessor_account_id());

        // all of the account's storage available balance will be staked
        let account_storage_available_balance = account
            .storage_balance(self.account_manager.storage_balance_bounds().min)
            .available;
        account.dec_near_balance(account_storage_available_balance);

        let stakable_near = account_storage_available_balance + env::attached_deposit();
        let near_stake_value = self.near_stake_value(stakable_near);
        // because of rounding down we need to convert the STAKE value back to NEAR, which ensures
        // that the account will not be short changed
        let stake_near_value = self.stake_near_value(near_stake_value);
        // the unstaked remainder is credited back to the account storage balance
        account.incr_near_balance(stakable_near - stake_near_value);
        account.save();

        self.stake_token
            .ft_mint(&env::predecessor_account_id(), near_stake_value);

        self.stake(stake_near_value);

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
            let value = (total_staked_balance / *self.stake_token.ft_total_supply()).into();
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

const TRANSFER_CALLBACK_GAS_KEY: u128 = 1954996031748648118640176768985870873;
type CallbackGas = Object<u128, Gas>;

impl StakingPoolOperator for StakingPoolComponent {
    fn ops_stake_callback_gas(&self) -> Gas {
        let default_gas = || {
            Gas::compute(vec![
                (
                    TransactionResource::ActionReceipt(SenderIsReceiver(false)),
                    1,
                ),
                (
                    TransactionResource::Action(ActionType::Stake(SenderIsReceiver(false))),
                    1,
                ),
                (
                    TransactionResource::DataReceipt(SenderIsReceiver(false), ByteLen(200)),
                    1,
                ),
            ]) + TERA.into()
        };
        CallbackGas::load(&TRANSFER_CALLBACK_GAS_KEY).map_or_else(default_gas, |gas| *gas)
    }
}

impl StakeActionCallback for StakingPoolComponent {
    fn ops_stake_callback(&mut self) {
        if !is_promise_success() {
            if env::account_locked_balance() > 0 {
                ERR_STAKE_ACTION_FAILED.log(format!(
                    "unstaking current locked amount: {}",
                    env::account_locked_balance()
                ));
                let stake_public_key = Self::state();
                Promise::new(env::current_account_id())
                    .stake(0, stake_public_key.stake_public_key.into());
            } else {
                ERR_STAKE_ACTION_FAILED.log("current locked balance is zero");
            }
        }
    }
}

impl StakingPoolComponent {
    /// when there is an unstaked balance, delegators add liquidity when they deposit funds to stake
    /// - this enables accounts to withdraw against the unstaked liquidity on a first come first serve basis
    pub const UNSTAKED_LIQUIDITY: BalanceId = BalanceId(0);
    pub const TOTAL_UNSTAKED_BALANCE: BalanceId = BalanceId(1);
    /// used to temporarily store the staked NEAR balance under the following circumstances:
    /// - when the stake action fails, the contract will unstake all and move the locked balance to
    ///   this balance
    /// - when the staking pool is paused by the operator
    ///
    /// This is needed to compute the STAKE token NEAR value. It basically locks in the STAKE NEAR value
    /// while the staking pool is offline. Once the staking pool goes back on line, then the balance is
    /// staked and this balance is cleared.
    pub const TOTAL_STAKED_BALANCE: BalanceId = BalanceId(2);

    fn state() -> ComponentState<State> {
        Self::load_state().expect("component has not been deployed")
    }

    fn stake(&self, amount: YoctoNear) {
        let stake_public_key = Self::state();
        Promise::new(env::current_account_id())
            .stake(
                env::account_locked_balance() + *amount,
                stake_public_key.stake_public_key.into(),
            )
            .then(json_function_callback::<()>(
                "ops_stake_callback",
                None,
                ZERO_NEAR,
                self.ops_stake_callback_gas(),
            ));
    }

    fn stake_near_value(&self, stake: TokenAmount) -> YoctoNear {
        if *stake == 0 {
            return ZERO_NEAR;
        }

        let total_staked_near_balance = env::account_locked_balance();
        if total_staked_near_balance == 0 {
            return (*stake).into();
        }

        (U256::from(total_staked_near_balance) * U256::from(*stake)
            / U256::from(*self.stake_token.ft_total_supply()))
        .as_u128()
        .into()
    }

    fn near_stake_value(&self, amount: YoctoNear) -> TokenAmount {
        if *amount == 0 {
            return 0.into();
        }

        let total_staked_near_balance = env::account_locked_balance();
        if total_staked_near_balance == 0 {
            return amount.value().into();
        }

        (U256::from(*self.stake_token.ft_total_supply()) * U256::from(*amount)
            / U256::from(total_staked_near_balance))
        .as_u128()
        .into()
    }
}

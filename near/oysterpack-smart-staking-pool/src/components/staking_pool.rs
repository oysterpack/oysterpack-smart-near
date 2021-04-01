use crate::{
    OperatorCommand, StakeAccountBalances, StakeActionCallback, StakeBalance, StakingPool,
    StakingPoolOperator, ERR_STAKED_BALANCE_TOO_LOW_TO_UNSTAKE, ERR_STAKE_ACTION_FAILED,
    LOG_EVENT_NOT_ENOUGH_TO_STAKE, LOG_EVENT_STATUS_OFFLINE,
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
use oysterpack_smart_near::asserts::{assert_near_attached, ERR_INSUFFICIENT_FUNDS};
use oysterpack_smart_near::data::numbers::U256;
use oysterpack_smart_near::data::Object;
use oysterpack_smart_near::domain::{
    ActionType, ByteLen, Gas, SenderIsReceiver, TransactionResource, ZERO_NEAR,
};
use oysterpack_smart_near::near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env, is_promise_success,
    json_types::ValidAccountId,
    serde::{Deserialize, Serialize},
    Promise,
};
use oysterpack_smart_near::{
    component::{Component, ComponentState, Deploy},
    domain::{PublicKey, YoctoNear},
    json_function_callback, to_valid_account_id, TERA, YOCTO,
};

type StakeAccountData = ();

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
    status: Status,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    /// While offline, accounts can still stake, but the funds are held until are held until the
    /// staking pool goes online
    /// - when the pool goes back online, then the staked funds are staked
    Offline(OfflineReason),
    /// the pool is actively staking
    Online,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum OfflineReason {
    Paused,
    StakeActionFailed,
}

impl Deploy for StakingPoolComponent {
    type Config = StakingPoolComponentConfig;

    fn deploy(config: Self::Config) {
        let state = State {
            stake_public_key: config.stake_public_key,
            status: Status::Offline(OfflineReason::Paused),
        };
        let state = Self::new_state(state);
        state.save();
    }
}

pub struct StakingPoolComponentConfig {
    pub stake_public_key: PublicKey,
}

impl StakingPool for StakingPoolComponent {
    fn ops_stake_balance(&self, account_id: ValidAccountId) -> Option<StakeAccountBalances> {
        self.account_manager
            .storage_balance_of(account_id.clone())
            .map(|storage_balance| {
                let stake_token_balance = {
                    let token_balance = self.stake_token.ft_balance_of(account_id);
                    if *token_balance == 0 {
                        None
                    } else {
                        Some(StakeBalance {
                            stake: token_balance,
                            near_value: self.stake_near_value_rounded_down(token_balance),
                        })
                    }
                };

                StakeAccountBalances {
                    storage_balance,
                    stake_token_balance,
                }
            })
    }

    fn ops_stake(&mut self) -> StakeAccountBalances {
        assert_near_attached("deposit is required to stake");
        let account_id = env::predecessor_account_id();
        let mut account = self
            .account_manager
            .registered_account_near_data(&account_id);

        // all of the account's storage available balance will be staked
        let account_storage_available_balance = account
            .storage_balance(self.account_manager.storage_balance_bounds().min)
            .available;
        account.dec_near_balance(account_storage_available_balance);

        let near = account_storage_available_balance + env::attached_deposit();
        let stake = self.near_stake_value_rounded_down(near);

        // because of rounding down we need to convert the STAKE value back to NEAR, which ensures
        // that the account will not be short changed when they unstake
        let stake_near_value = self.stake_near_value_rounded_down(stake);
        // the unstaked remainder is credited back to the account storage balance
        account.incr_near_balance(near - stake_near_value);
        account.save();

        if *stake > 0 {
            self.stake_token.ft_mint(&account_id, stake);
            self.stake(StakeAmount::Stake(stake_near_value));
        } else {
            LOG_EVENT_NOT_ENOUGH_TO_STAKE.log("");
        }

        self.ops_stake_balance(to_valid_account_id(&account_id))
            .unwrap()
    }

    fn ops_unstake(&mut self, amount: Option<YoctoNear>) -> StakeAccountBalances {
        let account_id = env::predecessor_account_id();
        let mut account = self
            .account_manager
            .registered_account_near_data(&account_id);

        let stake_balance = self
            .stake_token
            .ft_balance_of(to_valid_account_id(&account_id));
        let stake_near_value = self.stake_near_value_rounded_down(stake_balance);
        // burn STAKE tokens
        match amount {
            None => {
                self.stake_token.ft_burn(&account_id, stake_balance);
            }
            Some(amount) => {
                ERR_INSUFFICIENT_FUNDS.assert(|| stake_near_value >= amount);
                let stake_burn_amount = self.near_stake_value_rounded_up(amount);
                ERR_STAKED_BALANCE_TOO_LOW_TO_UNSTAKE.assert(|| stake_balance >= stake_burn_amount);
                self.stake_token.ft_burn(&account_id, stake_burn_amount);
            }
        }

        // credit the account storage balance
        account.incr_near_balance(stake_near_value);
        account.save();

        self.stake(StakeAmount::Unstake(stake_near_value));

        self.ops_stake_balance(to_valid_account_id(&account_id))
            .unwrap()
    }

    fn ops_stake_token_value(&self) -> YoctoNear {
        let total_staked_balance = *Self::total_staked_balance();
        if total_staked_balance == 0 {
            YOCTO.into()
        } else {
            let value = (total_staked_balance / *self.stake_token.ft_total_supply()).into();
            // since we are rounding down, we need to make sure that the value of 1 STAKE is at least 1 NEAR
            std::cmp::max(value, YOCTO.into())
        }
    }

    fn ops_stake_status(&self) -> Status {
        Self::state().status
    }
}

const TRANSFER_CALLBACK_GAS_KEY: u128 = 1954996031748648118640176768985870873;
type CallbackGas = Object<u128, Gas>;

impl StakingPoolOperator for StakingPoolComponent {
    fn ops_stake_operator_command(&mut self, command: OperatorCommand) {
        self.account_manager.assert_operator();

        match command {
            OperatorCommand::Pause => {
                let mut state = Self::state();
                if let Status::Online = state.status {
                    // unstake all
                    {
                        let staked_balance = env::account_locked_balance();
                        if staked_balance > 0 {
                            ContractNearBalances::set_balance(
                                Self::TOTAL_STAKED_BALANCE,
                                staked_balance.into(),
                            );
                            Promise::new(env::current_account_id())
                                .stake(0, state.stake_public_key.into());
                        }
                    }
                    // update status
                    {
                        state.status = Status::Offline(OfflineReason::Paused);
                        state.save();
                    }
                    LOG_EVENT_STATUS_OFFLINE.log("")
                }
            }
            OperatorCommand::Resume => {
                unimplemented!()
            }
            OperatorCommand::SetStakeCallbackGas(_) => {
                unimplemented!()
            }
            OperatorCommand::ClearStakeCallbackGas => {
                unimplemented!()
            }
        }
    }

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
    fn ops_stake_callback(&mut self, staked_balance: YoctoNear) {
        if !is_promise_success() {
            ERR_STAKE_ACTION_FAILED.log("");
            ContractNearBalances::set_balance(Self::TOTAL_STAKED_BALANCE, staked_balance);
            if env::account_locked_balance() > 0 {
                Promise::new(env::current_account_id())
                    .stake(0, Self::state().stake_public_key.into());
                Self::set_status(Status::Offline(OfflineReason::StakeActionFailed));
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
struct StakeActionCallbackArgs {
    pub staked_balance: YoctoNear,
}

impl StakingPoolComponent {
    /// used to temporarily store the staked NEAR balance under the following circumstances:
    /// - when the stake action fails, the contract will unstake all and move the locked balance to
    ///   this balance
    /// - when the staking pool is paused by the operator
    ///
    /// This is needed to compute the STAKE token NEAR value. It basically locks in the STAKE NEAR value
    /// while the staking pool is offline. Once the staking pool goes back on line, then the balance is
    /// staked and this balance is cleared.
    pub const TOTAL_STAKED_BALANCE: BalanceId = BalanceId(0);

    fn state() -> ComponentState<State> {
        Self::load_state().expect("component has not been deployed")
    }

    fn set_status(status: Status) {
        let mut state = Self::state();
        state.status = status;
        state.save();
    }

    fn total_staked_balance() -> YoctoNear {
        ContractNearBalances::load_near_balances()
            .get(&Self::TOTAL_STAKED_BALANCE)
            .cloned()
            .unwrap_or_else(|| env::account_locked_balance().into())
    }

    fn stake(&self, amount: StakeAmount) {
        let state = Self::state();
        match state.status {
            Status::Offline(_) => {
                LOG_EVENT_STATUS_OFFLINE.log("");
                match amount {
                    StakeAmount::Stake(amount) => {
                        ContractNearBalances::incr_balance(Self::TOTAL_STAKED_BALANCE, amount);
                    }
                    StakeAmount::Unstake(amount) => {
                        ContractNearBalances::decr_balance(Self::TOTAL_STAKED_BALANCE, amount);
                    }
                }
            }
            Status::Online => {
                let stake_amount = match amount {
                    StakeAmount::Stake(amount) => *(Self::total_staked_balance() + amount),
                    StakeAmount::Unstake(amount) => *(Self::total_staked_balance() - amount),
                };
                Promise::new(env::current_account_id())
                    .stake(stake_amount, state.stake_public_key.into())
                    .then(json_function_callback(
                        "ops_stake_callback",
                        Some(StakeActionCallbackArgs {
                            staked_balance: YoctoNear::from(stake_amount),
                        }),
                        ZERO_NEAR,
                        self.ops_stake_callback_gas(),
                    ));
                ContractNearBalances::clear_balance(Self::TOTAL_STAKED_BALANCE);
            }
        }
    }

    fn stake_near_value_rounded_down(&self, stake: TokenAmount) -> YoctoNear {
        if *stake == 0 {
            return ZERO_NEAR;
        }

        let total_staked_near_balance = Self::total_staked_balance();
        if *total_staked_near_balance == 0 {
            return (*stake).into();
        }

        (U256::from(*total_staked_near_balance) * U256::from(*stake)
            / U256::from(*self.stake_token.ft_total_supply()))
        .as_u128()
        .into()
    }

    fn near_stake_value_rounded_down(&self, amount: YoctoNear) -> TokenAmount {
        if *amount == 0 {
            return 0.into();
        }

        let total_staked_near_balance = *Self::total_staked_balance();
        if total_staked_near_balance == 0 {
            return amount.value().into();
        }

        (U256::from(*self.stake_token.ft_total_supply()) * U256::from(*amount)
            / U256::from(total_staked_near_balance))
        .as_u128()
        .into()
    }

    fn near_stake_value_rounded_up(&self, amount: YoctoNear) -> TokenAmount {
        if *amount == 0 {
            return 0.into();
        }

        let total_staked_near_balance = *Self::total_staked_balance();
        if total_staked_near_balance == 0 {
            return amount.value().into();
        }

        ((U256::from(*self.stake_token.ft_total_supply()) * U256::from(*amount)
            + U256::from(total_staked_near_balance - 1))
            / U256::from(total_staked_near_balance))
        .as_u128()
        .into()
    }
}

enum StakeAmount {
    Stake(YoctoNear),
    Unstake(YoctoNear),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {}
}

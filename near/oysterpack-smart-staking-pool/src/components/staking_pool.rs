use crate::{
    OfflineReason, StakeAccountBalances, StakeActionCallbacks, StakedBalance, StakingPool,
    StakingPoolBalances, StakingPoolOperator, StakingPoolOperatorCommand, StakingPoolOwner, Status,
    Treasury, UnstakedBalances, ERR_STAKED_BALANCE_TOO_LOW_TO_UNSTAKE, ERR_STAKE_ACTION_FAILED,
    LOG_EVENT_LIQUIDITY, LOG_EVENT_NOT_ENOUGH_TO_STAKE, LOG_EVENT_STAKE, LOG_EVENT_STATUS_OFFLINE,
    LOG_EVENT_STATUS_ONLINE, LOG_EVENT_TREASURY_DIVIDEND, LOG_EVENT_UNSTAKE, PERMISSION_TREASURER,
};
use oysterpack_smart_account_management::{
    components::account_management::AccountManagementComponent, AccountDataObject,
    AccountRepository, Permission, StorageManagement, ERR_ACCOUNT_NOT_REGISTERED,
    ERR_NOT_AUTHORIZED,
};
use oysterpack_smart_contract::{
    components::contract_ownership::ContractOwnershipComponent, BalanceId, ContractNearBalances,
    ContractOwnerObject, ContractOwnership,
};
use oysterpack_smart_fungible_token::{
    components::fungible_token::FungibleTokenComponent, FungibleToken, TokenAmount, TokenService,
};
use oysterpack_smart_near::asserts::ERR_NEAR_DEPOSIT_REQUIRED;
use oysterpack_smart_near::domain::BasisPoints;
use oysterpack_smart_near::{
    asserts::{ERR_ILLEGAL_STATE, ERR_INSUFFICIENT_FUNDS, ERR_INVALID},
    component::{Component, ComponentState, Deploy},
    data::numbers::U256,
    domain::{ActionType, Gas, PublicKey, SenderIsReceiver, TransactionResource, YoctoNear},
    json_function_callback,
    near_sdk::{
        borsh::{self, BorshDeserialize, BorshSerialize},
        env, is_promise_success,
        json_types::ValidAccountId,
        serde::{Deserialize, Serialize},
        AccountId, Promise, PromiseOrValue,
    },
    to_valid_account_id, TERA, YOCTO,
};
use std::cmp::min;

pub type StakeAccountData = UnstakedBalances;

pub type AccountManager = AccountManagementComponent<StakeAccountData>;
pub type StakeFungibleToken = FungibleTokenComponent<StakeAccountData>;

/// Staking Pool Component
///
/// ## Deployment
/// - permissions: [`crate::PERMISSION_TREASURER`];
pub struct StakingPoolComponent {
    account_manager: AccountManager,
    stake_token: StakeFungibleToken,
    contract_ownership: ContractOwnershipComponent,
}

impl StakingPoolComponent {
    pub fn new(
        account_manager: AccountManagementComponent<StakeAccountData>,
        stake: StakeFungibleToken,
    ) -> Self {
        Self {
            account_manager,
            stake_token: stake,
            contract_ownership: ContractOwnershipComponent,
        }
    }
}

impl Component for StakingPoolComponent {
    type State = State;
    const STATE_KEY: u128 = 1954854625400732566949949714395710108;
}

#[derive(
    BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, Copy, PartialEq, Debug,
)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub struct State {
    /// validator public key used for staking
    pub stake_public_key: PublicKey,

    pub status: Status,

    // used to override the built in computed gas amount
    pub callback_gas: Option<Gas>,
    pub staking_fee: BasisPoints,

    /// NEAR deposited into the treasury is staked and stake earnings are used to boost the
    /// staking pool yield
    /// - this is used to track the treasury STAKE NEAR value
    /// - if it changes before staking fees are deposited, then it means the treasury received
    ///   staking rewards. The staking rewards will be burned before depositing the staking fee
    pub treasury_balance: YoctoNear,
}

impl State {
    /// used to temporarily store the staked NEAR balance under the following circumstances:
    /// - when the stake action fails, the contract will unstake all and move the locked balance to
    ///   this balance
    /// - when the staking pool is paused by the operator
    ///
    /// This is needed to compute the STAKE token NEAR value. It basically locks in the STAKE NEAR value
    /// while the staking pool is offline. Once the staking pool goes back on line, then the balance is
    /// staked and this balance is cleared.
    pub const TOTAL_STAKED_BALANCE: BalanceId = BalanceId(1955270816453372906764299513102112489);
    // tracks how funds have been staked and unstaked and pending the stake action
    pub const STAKED_BALANCE: BalanceId = BalanceId(1956222035569770585105442079419123289);
    pub const UNSTAKED_BALANCE: BalanceId = BalanceId(1956222069432890001326139573174433605);

    /// unstaked funds are locked for 4 epochs
    /// - we need to track unstaked funds that is owned by accounts separate from the account NEAR balances
    /// - accounts will need to withdraw unstaked balances against this balance
    pub const TOTAL_UNSTAKED_BALANCE: BalanceId = BalanceId(1955705469859818043123742456310621056);

    pub const UNSTAKED_LIQUIDITY_POOL: BalanceId = BalanceId(1955784487678443851622222785149485288);

    pub fn callback_gas(&self) -> Gas {
        self.callback_gas.unwrap_or_else(Self::min_callback_gas)
    }

    fn min_callback_gas() -> Gas {
        {
            Gas::compute(vec![
                (
                    TransactionResource::ActionReceipt(SenderIsReceiver(false)),
                    1,
                ),
                (
                    TransactionResource::Action(ActionType::Stake(SenderIsReceiver(false))),
                    1,
                ),
            ]) + TERA.into()
        }
    }

    fn total_staked_balance(self) -> YoctoNear {
        match self.status {
            Status::Online => {
                // if there are no stake actions in flight, then resync the total staked balance
                // to ensure any staking rewards are captured
                let staked = ContractNearBalances::near_balance(Self::STAKED_BALANCE);
                let unstaked = ContractNearBalances::near_balance(Self::UNSTAKED_BALANCE);
                if staked == YoctoNear::ZERO && unstaked == YoctoNear::ZERO {
                    ContractNearBalances::set_balance(
                        Self::TOTAL_STAKED_BALANCE,
                        env::account_locked_balance().into(),
                    );
                }
                ContractNearBalances::near_balance(Self::TOTAL_STAKED_BALANCE) + staked - unstaked
            }
            Status::Offline(_) => ContractNearBalances::near_balance(Self::TOTAL_STAKED_BALANCE),
        }
    }

    fn incr_total_staked_balance(amount: YoctoNear) {
        ContractNearBalances::incr_balance(State::TOTAL_STAKED_BALANCE, amount);
    }

    fn decr_total_staked_balance(amount: YoctoNear) {
        ContractNearBalances::decr_balance(State::TOTAL_STAKED_BALANCE, amount);
    }

    fn set_total_staked_balance(amount: YoctoNear) {
        ContractNearBalances::set_balance(State::TOTAL_STAKED_BALANCE, amount);
    }

    fn incr_total_unstaked_balance(amount: YoctoNear) {
        ContractNearBalances::incr_balance(Self::TOTAL_UNSTAKED_BALANCE, amount);
    }

    fn decr_total_unstaked_balance(mut amount: YoctoNear) {
        let liquidity = Self::liquidity();
        if liquidity > YoctoNear::ZERO {
            if amount <= liquidity {
                let total_liquidity =
                    ContractNearBalances::decr_balance(Self::UNSTAKED_LIQUIDITY_POOL, amount);
                LOG_EVENT_LIQUIDITY
                    .log(format!("removed={}, total={}", liquidity, total_liquidity));
                return;
            }
            amount -= liquidity;
            ContractNearBalances::clear_balance(Self::UNSTAKED_LIQUIDITY_POOL);
            LOG_EVENT_LIQUIDITY.log(format!("removed={}, total=0", liquidity));
            if amount > YoctoNear::ZERO {
                ContractNearBalances::decr_balance(Self::TOTAL_UNSTAKED_BALANCE, amount);
            }
        } else {
            ContractNearBalances::decr_balance(Self::TOTAL_UNSTAKED_BALANCE, amount);
        }
    }

    /// If there are unstaked balances, then transfer the specified amount to the liquidity pool
    fn add_liquidity(amount: YoctoNear) {
        if amount == YoctoNear::ZERO {
            return;
        }
        let total_unstaked_balance =
            ContractNearBalances::near_balance(Self::TOTAL_UNSTAKED_BALANCE);
        if total_unstaked_balance > YoctoNear::ZERO {
            let liquidity = min(amount, total_unstaked_balance);
            ContractNearBalances::decr_balance(Self::TOTAL_UNSTAKED_BALANCE, liquidity);
            let total_liquidity =
                ContractNearBalances::incr_balance(Self::UNSTAKED_LIQUIDITY_POOL, liquidity);
            LOG_EVENT_LIQUIDITY.log(format!("added={}, total={}", liquidity, total_liquidity));
        }
    }

    pub fn liquidity() -> YoctoNear {
        ContractNearBalances::near_balance(Self::UNSTAKED_LIQUIDITY_POOL)
    }

    pub fn total_unstaked_balance() -> YoctoNear {
        ContractNearBalances::near_balance(Self::TOTAL_UNSTAKED_BALANCE)
    }

    pub fn staked_balance() -> YoctoNear {
        ContractNearBalances::near_balance(Self::STAKED_BALANCE)
    }

    fn incr_staked_balance(amount: YoctoNear) {
        ContractNearBalances::incr_balance(Self::STAKED_BALANCE, amount);
    }

    fn decr_staked_balance(amount: YoctoNear) {
        ContractNearBalances::decr_balance(Self::STAKED_BALANCE, amount);
    }

    fn clear_staked_balance() {
        ContractNearBalances::clear_balance(Self::STAKED_BALANCE);
    }

    pub fn unstaked_balance() -> YoctoNear {
        ContractNearBalances::near_balance(Self::UNSTAKED_BALANCE)
    }

    fn incr_unstaked_balance(amount: YoctoNear) {
        ContractNearBalances::incr_balance(Self::UNSTAKED_BALANCE, amount);
    }

    fn decr_unstaked_balance(amount: YoctoNear) {
        ContractNearBalances::decr_balance(Self::UNSTAKED_BALANCE, amount);
    }

    fn clear_unstaked_balance() {
        ContractNearBalances::clear_balance(Self::UNSTAKED_BALANCE);
    }
}

impl Deploy for StakingPoolComponent {
    type Config = StakingPoolComponentConfig;

    fn deploy(config: Self::Config) {
        let state = State {
            stake_public_key: config.stake_public_key,
            status: Status::Offline(OfflineReason::Stopped),
            callback_gas: None,
            staking_fee: 80.into(),
            treasury_balance: YoctoNear::ZERO,
        };
        let state = Self::new_state(state);
        state.save();

        // the contract account serves as the treasury account
        // we need to register an account with storage management in order to deposit STAKE into the
        // treasury
        let treasury = env::current_account_id();
        AccountManager::register_account_if_not_exists(&treasury);
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
                    let token_balance = self.stake_token.ft_balance_of(account_id.clone());
                    if *token_balance == 0 {
                        None
                    } else {
                        Some(StakedBalance {
                            stake: token_balance,
                            near_value: self.stake_near_value_rounded_down(token_balance),
                        })
                    }
                };

                let data = self
                    .account_manager
                    .load_account_data(account_id.as_ref())
                    .map(|data| (**data).into());
                StakeAccountBalances {
                    storage_balance,
                    staked: stake_token_balance,
                    unstaked: data,
                }
            })
    }

    fn ops_stake(&mut self) -> PromiseOrValue<StakeAccountBalances> {
        self.stake_account_funds(
            &env::predecessor_account_id(),
            env::attached_deposit().into(),
        )
    }

    fn ops_unstake(&mut self, amount: Option<YoctoNear>) -> PromiseOrValue<StakeAccountBalances> {
        let account_id = env::predecessor_account_id();
        ERR_ACCOUNT_NOT_REGISTERED.assert(|| self.account_manager.account_exists(&account_id));

        let stake_balance = self
            .stake_token
            .ft_balance_of(to_valid_account_id(&account_id));
        let stake_near_value = self.stake_near_value_rounded_down(stake_balance);
        let (near_amount, stake_token_amount) = match amount {
            None => (stake_near_value, stake_balance),
            Some(near_amount) => {
                ERR_INSUFFICIENT_FUNDS.assert(|| stake_near_value >= near_amount);
                let stake_token_amount = self.near_stake_value_rounded_up(near_amount);
                ERR_STAKED_BALANCE_TOO_LOW_TO_UNSTAKE
                    .assert(|| stake_balance >= stake_token_amount);
                (near_amount, stake_token_amount)
            }
        };

        LOG_EVENT_UNSTAKE.log(format!(
            "near_amount={}, stake_token_amount={}",
            near_amount, stake_token_amount
        ));
        let state = Self::state();
        match state.status {
            Status::Online => {
                State::incr_unstaked_balance(near_amount);
                self.stake_token.ft_lock(&account_id, stake_token_amount);
                PromiseOrValue::Promise(Self::unstake_funds(
                    *state,
                    &account_id,
                    near_amount,
                    stake_token_amount,
                ))
            }
            Status::Offline(_) => {
                LOG_EVENT_STATUS_OFFLINE.log("");

                State::decr_total_staked_balance(near_amount);
                State::incr_total_unstaked_balance(near_amount);

                self.stake_token.ft_burn(&account_id, stake_token_amount);
                self.credit_unstaked_amount(&account_id, near_amount);

                self.registered_stake_account_balance(&account_id)
            }
        }
    }

    fn ops_restake(&mut self, amount: Option<YoctoNear>) -> PromiseOrValue<StakeAccountBalances> {
        let account_id = env::predecessor_account_id();
        ERR_ACCOUNT_NOT_REGISTERED.assert(|| self.account_manager.account_exists(&account_id));

        match self.account_manager.load_account_data(&account_id) {
            None => match amount {
                None => self.registered_stake_account_balance(&account_id),
                Some(_) => {
                    ERR_INSUFFICIENT_FUNDS.panic();
                    unreachable!()
                }
            },
            Some(mut account) => {
                let (near_amount, stake_token_amount) = {
                    let near = amount.unwrap_or_else(|| account.total());
                    let (stake, remainder) = self.convert_near_to_stake(near);
                    let stake_near_value = near - remainder;
                    account.debit_for_restaking(stake_near_value);
                    account.save();
                    State::decr_total_unstaked_balance(stake_near_value);
                    (stake_near_value, stake)
                };
                self.stake(&account_id, near_amount, stake_token_amount)
            }
        }
    }

    fn ops_stake_withdraw(&mut self, amount: Option<YoctoNear>) -> StakeAccountBalances {
        let account_id = env::predecessor_account_id();
        ERR_ACCOUNT_NOT_REGISTERED.assert(|| self.account_manager.account_exists(&account_id));

        fn debit_available_balance(
            mut unstaked_balances: AccountDataObject<StakeAccountData>,
            amount: YoctoNear,
        ) {
            unstaked_balances.debit_available_balance(amount);
            if unstaked_balances.total() == YoctoNear::ZERO {
                unstaked_balances.delete();
            } else {
                unstaked_balances.save();
            }
            State::decr_total_unstaked_balance(amount);
            Promise::new(env::predecessor_account_id()).transfer(*amount);
        }

        match amount {
            None => {
                if let Some(mut unstaked_balances) =
                    self.account_manager.load_account_data(&account_id)
                {
                    unstaked_balances.apply_liquidity();
                    let amount = unstaked_balances.available();
                    if amount > YoctoNear::ZERO {
                        debit_available_balance(unstaked_balances, amount);
                    }
                }
            }
            Some(amount) => {
                ERR_INVALID.assert(|| amount > YoctoNear::ZERO, || "amount must be > 0");
                if let Some(mut unstaked_balances) =
                    self.account_manager.load_account_data(&account_id)
                {
                    unstaked_balances.apply_liquidity();
                    debit_available_balance(unstaked_balances, amount);
                } else {
                    ERR_INSUFFICIENT_FUNDS.panic();
                }
            }
        }

        self.ops_stake_balance(to_valid_account_id(&account_id))
            .unwrap()
    }

    fn ops_stake_token_value(&self) -> YoctoNear {
        let total_staked_balance = *Self::state().total_staked_balance();
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

    fn ops_stake_pool_balances(&self) -> StakingPoolBalances {
        StakingPoolBalances::load()
    }
}

impl StakingPoolOperator for StakingPoolComponent {
    fn ops_stake_operator_command(&mut self, command: StakingPoolOperatorCommand) {
        self.account_manager.assert_operator();

        match command {
            StakingPoolOperatorCommand::StopStaking => Self::stop_staking(OfflineReason::Stopped),
            StakingPoolOperatorCommand::StartStaking => Self::start_staking(),
            StakingPoolOperatorCommand::SetStakeCallbackGas(gas) => {
                Self::set_stake_callback_gas(gas)
            }
            StakingPoolOperatorCommand::ResetStakeCallbackGas => Self::reset_stake_callback_gas(),
            StakingPoolOperatorCommand::UpdatePublicKey(public_key) => {
                Self::update_public_key(public_key)
            }
            StakingPoolOperatorCommand::UpdateStakingFee(fee) => Self::update_staking_fee(fee),
        }
    }

    fn ops_stake_callback_gas(&self) -> Gas {
        Self::state().callback_gas()
    }

    fn ops_stake_state(&self) -> State {
        *Self::state()
    }
}

impl StakingPoolComponent {
    fn stop_staking(reason: OfflineReason) {
        let mut state = Self::state();
        if let Status::Online = state.status {
            // update the status
            {
                state.status = Status::Offline(reason);
                state.save();
            }

            // unstake all
            {
                let total_staked_balance =
                    ContractNearBalances::near_balance(State::TOTAL_STAKED_BALANCE)
                        + State::staked_balance()
                        - State::unstaked_balance();
                State::set_total_staked_balance(total_staked_balance);
                State::clear_staked_balance();
                State::clear_unstaked_balance();
                if env::account_locked_balance() > 0 {
                    Promise::new(env::current_account_id())
                        .stake(0, state.stake_public_key.into())
                        .then(json_function_callback(
                            "ops_stake_stop_finalize",
                            Option::<()>::None,
                            YoctoNear::ZERO,
                            state.callback_gas(),
                        ));
                }
            }
            if let OfflineReason::StakeActionFailed = reason {
                ERR_STAKE_ACTION_FAILED.log("");
            }
            LOG_EVENT_STATUS_OFFLINE.log(reason);
        }
    }

    fn start_staking() {
        let mut state = Self::state();
        if let Status::Offline(_) = state.status {
            let total_staked_balance =
                ContractNearBalances::near_balance(State::TOTAL_STAKED_BALANCE);
            // update status
            {
                state.status = Status::Online;
                state.save();
            }
            LOG_EVENT_STATUS_ONLINE.log("");

            // stake
            {
                if total_staked_balance > YoctoNear::ZERO {
                    Promise::new(env::current_account_id())
                        .stake(*total_staked_balance, state.stake_public_key.into())
                        .then(json_function_callback(
                            "ops_stake_start_finalize",
                            Option::<()>::None,
                            YoctoNear::ZERO,
                            state.callback_gas(),
                        ));
                }
            }
        }
    }

    fn set_stake_callback_gas(gas: Gas) {
        let min_callback_gas = State::min_callback_gas();
        ERR_INVALID.assert(
            || gas >= min_callback_gas,
            || format!("minimum callback gas required is: {}", min_callback_gas),
        );
        let mut state = Self::state();
        state.callback_gas = Some(gas);
        state.save();
    }

    fn reset_stake_callback_gas() {
        let mut state = Self::state();
        state.callback_gas = None;
        state.save();
    }

    fn update_public_key(public_key: PublicKey) {
        let mut state = Self::state();
        ERR_ILLEGAL_STATE.assert(
            || !state.status.is_online(),
            || "staking pool must be paused to update the staking public key",
        );
        state.stake_public_key = public_key;
        state.save();
    }

    fn update_staking_fee(fee: BasisPoints) {
        let mut state = Self::state();
        state.staking_fee = fee;
        state.save();
    }
}

impl StakingPoolOwner for StakingPoolComponent {
    fn ops_stake_owner_balance(
        &mut self,
        amount: Option<YoctoNear>,
    ) -> PromiseOrValue<StakeAccountBalances> {
        ContractOwnerObject::assert_owner_access();
        let owner_account_id = env::predecessor_account_id();
        let mut owner = AccountManager::get_or_register_account(&owner_account_id);

        let balance = self.contract_ownership.ops_owner_balance();
        let amount = amount.unwrap_or_else(|| balance.available);
        ERR_INSUFFICIENT_FUNDS.assert(|| balance.available >= amount);
        // transfer owner balance to owner account's storage balance
        owner.incr_near_balance(amount);
        owner.save();

        self.stake_account_funds(&owner_account_id, YoctoNear::ZERO)
    }
}

impl StakeActionCallbacks for StakingPoolComponent {
    fn ops_stake_finalize(
        &mut self,
        account_id: AccountId,
        amount: YoctoNear,
        stake_token_amount: TokenAmount,
    ) -> StakeAccountBalances {
        if let Status::Online = Self::state().status {
            if Self::handle_stake_action_result() {
                State::decr_staked_balance(amount);
            }
        }
        self.pay_dividend_and_apply_staking_fees(&account_id, amount, stake_token_amount);
        self.ops_stake_balance(to_valid_account_id(&account_id))
            .unwrap()
    }

    fn ops_unstake_finalize(
        &mut self,
        account_id: AccountId,
        amount: YoctoNear,
        stake_token_amount: TokenAmount,
    ) -> StakeAccountBalances {
        // update state
        {
            if Self::handle_stake_action_result() {
                State::decr_unstaked_balance(amount);
            }
            State::incr_total_unstaked_balance(amount);
        }
        // update account balances
        {
            self.stake_token.ft_burn(&account_id, stake_token_amount);
            self.credit_unstaked_amount(&account_id, amount);
        }
        self.ops_stake_balance(to_valid_account_id(&account_id))
            .unwrap()
    }

    fn ops_stake_start_finalize(&mut self) {
        if is_promise_success() {
            ContractNearBalances::set_balance(
                State::TOTAL_STAKED_BALANCE,
                env::account_locked_balance().into(),
            );
        } else {
            Self::stop_staking(OfflineReason::StakeActionFailed)
        }
    }

    fn ops_stake_stop_finalize(&mut self) {
        if is_promise_success() {
            LOG_EVENT_STATUS_OFFLINE.log("all NEAR has been unstaked");
        } else {
            ERR_STAKE_ACTION_FAILED.log("failed to go offline");
        }
    }
}

impl Treasury for StakingPoolComponent {
    fn ops_stake_treasury_deposit(&mut self) -> PromiseOrValue<StakeAccountBalances> {
        let deposit = YoctoNear::from(env::attached_deposit());
        ERR_NEAR_DEPOSIT_REQUIRED.assert(|| deposit > YoctoNear::ZERO);

        State::add_liquidity(deposit);
        let stake = self.near_stake_value_rounded_down(deposit);
        self.stake(&env::current_account_id(), deposit, stake)
    }

    fn ops_stake_treasury_transfer_to_owner(&mut self, amount: Option<YoctoNear>) {
        ERR_NOT_AUTHORIZED.assert(|| {
            let account = self
                .account_manager
                .registered_account_near_data(&env::predecessor_account_id());
            account.contains_permissions(self.treasurer_permission().into())
        });

        let owner_account_id = ContractOwnershipComponent.ops_owner();
        AccountManager::register_account_if_not_exists(&owner_account_id);

        let mut state = Self::state();
        self.pay_dividend(state.treasury_balance);

        let treasury_account = env::current_account_id();
        let amount = {
            let treasury_balance = self
                .stake_token
                .ft_balance_of(to_valid_account_id(&treasury_account));
            if treasury_balance == TokenAmount::ZERO {
                return;
            }

            let treasury_near_balance = self.stake_near_value_rounded_down(treasury_balance);
            match amount {
                None => treasury_near_balance,
                Some(amount) => {
                    ERR_INSUFFICIENT_FUNDS.assert(|| treasury_near_balance >= amount);
                    amount
                }
            }
        };
        let stake = self.near_stake_value_rounded_down(amount);

        // transfer STAKE from treasury to owner account
        {
            self.stake_token.ft_burn(&treasury_account, stake);
            self.stake_token.ft_mint(&owner_account_id, stake);
        }

        // debit from the treasury balance
        {
            state.treasury_balance -= amount;
            state.save();
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
struct StakeActionCallbackArgs {
    account_id: AccountId,
    amount: YoctoNear,
    stake_token_amount: TokenAmount,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
struct ResumeFinalizeCallbackArgs {
    pub total_staked_balance: YoctoNear,
}

impl StakingPoolComponent {
    fn treasurer_permission(&self) -> Permission {
        self.account_manager
            .permission_by_name(PERMISSION_TREASURER)
            .unwrap()
    }

    fn pay_dividend(&mut self, treasury_balance: YoctoNear) -> YoctoNear {
        let treasury_stake_balance = self
            .stake_token
            .ft_balance_of(to_valid_account_id(&env::current_account_id()));
        let current_treasury_near_value =
            self.stake_near_value_rounded_down(treasury_stake_balance);
        if treasury_balance > YoctoNear::ZERO {
            let treasury_staking_earnings = current_treasury_near_value - treasury_balance;
            let treasury_staking_earnings_stake_value =
                self.near_stake_value_rounded_down(treasury_staking_earnings);
            if treasury_staking_earnings_stake_value > TokenAmount::ZERO {
                self.stake_token.ft_burn(
                    &env::current_account_id(),
                    treasury_staking_earnings_stake_value,
                );
                LOG_EVENT_TREASURY_DIVIDEND.log(format!(
                    "{} NEAR / {} STAKE",
                    treasury_staking_earnings, treasury_staking_earnings_stake_value
                ));
            }
        }
        current_treasury_near_value
    }

    /// if treasury has earned staking rewards then burn STAKE tokens to distribute earnings
    fn pay_dividend_and_apply_staking_fees(
        &mut self,
        account_id: &str,
        amount: YoctoNear,
        stake_token_amount: TokenAmount,
    ) -> TokenAmount {
        let mut state = Self::state();
        self.stake_token.ft_mint(&account_id, stake_token_amount);

        // if treasury received staking rewards, then pay out the dividend
        let current_treasury_near_value = self.pay_dividend(state.treasury_balance);

        // apply staking fee
        let staking_fee = if account_id == &env::current_account_id()
            || account_id == &ContractOwnershipComponent.ops_owner()
        {
            // treasury and owner accounts do not get charged staking fees
            TokenAmount::ZERO
        } else {
            self.near_stake_value_rounded_down(amount * state.staking_fee)
        };
        if staking_fee > TokenAmount::ZERO {
            self.stake_token.ft_burn(&account_id, staking_fee);
            let treasury_stake_balance = self
                .stake_token
                .ft_mint(&env::current_account_id(), staking_fee);
            state.treasury_balance = self.stake_near_value_rounded_down(treasury_stake_balance);
        } else {
            state.treasury_balance = current_treasury_near_value;
        }

        state.save();
        staking_fee
    }

    /// return true if stake action succeeded
    fn handle_stake_action_result() -> bool {
        if is_promise_success() {
            ContractNearBalances::set_balance(
                State::TOTAL_STAKED_BALANCE,
                env::account_locked_balance().into(),
            );
            true
        } else {
            Self::stop_staking(OfflineReason::StakeActionFailed);
            false
        }
    }

    fn registered_stake_account_balance(
        &self,
        account_id: &str,
    ) -> PromiseOrValue<StakeAccountBalances> {
        PromiseOrValue::Value(
            self.ops_stake_balance(to_valid_account_id(account_id))
                .unwrap(),
        )
    }

    fn state() -> ComponentState<State> {
        Self::load_state().expect("component has not been deployed")
    }

    fn credit_unstaked_amount(&self, account_id: &str, amount: YoctoNear) {
        let mut account = self.account_manager.registered_account_data(&account_id);
        account.credit_unstaked(amount);
        account.save();
    }

    fn stake_account_funds(
        &mut self,
        account_id: &str,
        deposit: YoctoNear,
    ) -> PromiseOrValue<StakeAccountBalances> {
        let mut account = self
            .account_manager
            .registered_account_near_data(&account_id);

        // all of the account's storage available balance will be staked
        let (near_amount, stake_token_amount) = {
            let account_storage_available_balance = account
                .storage_balance(self.account_manager.storage_balance_bounds().min)
                .available;
            account.dec_near_balance(account_storage_available_balance);

            let near = account_storage_available_balance + deposit;
            let (stake, remainder) = self.convert_near_to_stake(near);
            account.incr_near_balance(remainder);
            account.save();

            (near - remainder, stake)
        };

        State::add_liquidity(near_amount);
        self.stake(account_id, near_amount, stake_token_amount)
    }

    fn stake(
        &mut self,
        account_id: &str,
        near_amount: YoctoNear,
        stake_token_amount: TokenAmount,
    ) -> PromiseOrValue<StakeAccountBalances> {
        if near_amount == YoctoNear::ZERO {
            // INVARIANT CHECK: if `near_amount` is zero, then `stake_token_amount` should be zero
            assert_eq!(stake_token_amount, TokenAmount::ZERO);
            LOG_EVENT_NOT_ENOUGH_TO_STAKE.log("");
            return self.registered_stake_account_balance(account_id);
        }

        let state = Self::state();
        LOG_EVENT_STAKE.log(format!(
            "near_amount={}, stake_token_amount={}",
            near_amount, stake_token_amount
        ));
        match state.status {
            Status::Online => {
                State::incr_staked_balance(near_amount);

                PromiseOrValue::Promise(Self::stake_funds(
                    *state,
                    account_id,
                    near_amount,
                    stake_token_amount,
                ))
            }
            Status::Offline(_) => {
                LOG_EVENT_STATUS_OFFLINE.log("");
                State::incr_total_staked_balance(near_amount);
                self.pay_dividend_and_apply_staking_fees(
                    account_id,
                    near_amount,
                    stake_token_amount,
                );
                self.registered_stake_account_balance(account_id)
            }
        }
    }

    fn stake_funds(
        state: State,
        account_id: &str,
        amount: YoctoNear,
        stake_token_amount: TokenAmount,
    ) -> Promise {
        Self::create_stake_workflow(
            state,
            account_id,
            amount,
            stake_token_amount,
            "ops_stake_finalize",
        )
    }

    fn unstake_funds(
        state: State,
        account_id: &str,
        amount: YoctoNear,
        stake_token_amount: TokenAmount,
    ) -> Promise {
        Self::create_stake_workflow(
            state,
            account_id,
            amount,
            stake_token_amount,
            "ops_unstake_finalize",
        )
    }

    fn create_stake_workflow(
        state: State,
        account_id: &str,
        amount: YoctoNear,
        stake_token_amount: TokenAmount,
        callback: &str,
    ) -> Promise {
        let total_staked_balance = state.total_staked_balance();
        let stake = Promise::new(env::current_account_id())
            .stake(*total_staked_balance, state.stake_public_key.into());
        let finalize = json_function_callback(
            callback,
            Some(StakeActionCallbackArgs {
                account_id: account_id.to_string(),
                amount,
                stake_token_amount,
            }),
            YoctoNear::ZERO,
            state.callback_gas(),
        );
        stake.then(finalize)
    }

    fn stake_near_value_rounded_down(&self, stake: TokenAmount) -> YoctoNear {
        if *stake == 0 {
            return YoctoNear::ZERO;
        }

        let total_staked_near_balance = Self::state().total_staked_balance();
        let ft_total_supply = *self.stake_token.ft_total_supply();
        if *total_staked_near_balance == 0 || ft_total_supply == 0 {
            return (*stake).into();
        }
        (U256::from(*total_staked_near_balance) * U256::from(*stake) / U256::from(ft_total_supply))
            .as_u128()
            .into()
    }

    /// converts the specified NEAR amount to STAKE and returns the STAKE equivalent and any NEAR
    /// remainder that could not be converted into STAKE because of rounding
    ///
    /// ## Notes
    /// because of rounding down we need to convert the STAKE value back to NEAR, which ensures that
    /// the account will not be short changed when they unstake
    fn convert_near_to_stake(&self, amount: YoctoNear) -> (TokenAmount, YoctoNear) {
        let stake = self.near_stake_value_rounded_down(amount);
        let stake_near_value = self.stake_near_value_rounded_down(stake);
        (stake, amount - stake_near_value)
    }

    fn near_stake_value_rounded_down(&self, amount: YoctoNear) -> TokenAmount {
        if *amount == 0 {
            return 0.into();
        }

        let total_staked_balance = *Self::state().total_staked_balance();
        let ft_total_supply = *self.stake_token.ft_total_supply();
        if total_staked_balance == 0 || ft_total_supply == 0 {
            return (*amount).into();
        }

        (U256::from(ft_total_supply) * U256::from(*amount) / U256::from(total_staked_balance))
            .as_u128()
            .into()
    }

    fn near_stake_value_rounded_up(&self, amount: YoctoNear) -> TokenAmount {
        if *amount == 0 {
            return 0.into();
        }

        let total_staked_balance = *Self::state().total_staked_balance();
        let ft_total_supply = *self.stake_token.ft_total_supply();
        if total_staked_balance == 0 || ft_total_supply == 0 {
            return amount.value().into();
        }

        ((U256::from(ft_total_supply) * U256::from(*amount) + U256::from(total_staked_balance - 1))
            / U256::from(total_staked_balance))
        .as_u128()
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;
    use oysterpack_smart_account_management::{
        components::account_management::AccountManagementComponentConfig, *,
    };
    use oysterpack_smart_fungible_token::components::fungible_token::FungibleTokenConfig;
    use oysterpack_smart_fungible_token::*;
    use oysterpack_smart_near::near_sdk::{env, serde_json, test_utils, VMContext};
    use oysterpack_smart_near::{component::*, *};
    use oysterpack_smart_near_test::*;
    use std::collections::HashMap;
    use std::convert::*;

    const TREASURER_PERMISSION_BIT: u8 = 0;

    fn account_manager() -> AccountManager {
        StakeFungibleToken::register_storage_management_event_handler();
        let mut permissions = HashMap::new();
        permissions.insert(TREASURER_PERMISSION_BIT, PERMISSION_TREASURER);
        let contract_permissions = ContractPermissions(Some(permissions));
        AccountManager::new(&contract_permissions)
    }

    fn ft_stake() -> StakeFungibleToken {
        StakeFungibleToken::new(account_manager())
    }

    fn staking_public_key() -> PublicKey {
        let key = [0_u8; 33];
        let key: PublicKey = key[..].try_into().unwrap();
        key
    }

    fn staking_public_key_as_string() -> String {
        let pk_bytes: Vec<u8> = staking_public_key().into();
        bs58::encode(pk_bytes).into_string()
    }

    fn deploy_with_registered_account() -> (VMContext, StakingPoolComponent) {
        deploy(OWNER, ADMIN, ACCOUNT, true)
    }

    fn deploy_with_unregistered_account() -> (VMContext, StakingPoolComponent) {
        deploy(OWNER, ADMIN, ACCOUNT, false)
    }

    fn deploy(
        owner: &str,
        admin: &str,
        account: &str,
        register_account: bool,
    ) -> (VMContext, StakingPoolComponent) {
        let ctx = new_context(account);
        testing_env!(ctx.clone());

        ContractOwnershipComponent::deploy(to_valid_account_id(owner));

        AccountManager::deploy(AccountManagementComponentConfig {
            storage_usage_bounds: None,
            admin_account: to_valid_account_id(admin),
            component_account_storage_mins: Some(vec![StakeFungibleToken::account_storage_min]),
        });

        StakeFungibleToken::deploy(FungibleTokenConfig {
            metadata: Metadata {
                spec: Spec(FT_METADATA_SPEC.to_string()),
                name: Name("STAKE".to_string()),
                symbol: Symbol("STAKE".to_string()),
                decimals: 24,
                icon: None,
                reference: None,
                reference_hash: None,
            },
            token_supply: 0,
        });

        StakingPoolComponent::deploy(StakingPoolComponentConfig {
            stake_public_key: staking_public_key(),
        });
        assert!(AccountNearDataObject::exists(env::current_account_id().as_str()),
                "staking pool deployment should have registered an account for itself to serve as the treasury");

        if register_account {
            let mut ctx = ctx.clone();
            ctx.attached_deposit = YOCTO;
            testing_env!(ctx);
            account_manager().storage_deposit(None, Some(true));
        }

        (
            ctx,
            StakingPoolComponent::new(account_manager(), ft_stake()),
        )
    }

    fn bring_pool_online(mut ctx: VMContext, staking_pool: &mut StakingPoolComponent) {
        ctx.predecessor_account_id = ADMIN.to_string();
        testing_env!(ctx.clone());
        staking_pool.ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
        assert!(staking_pool.ops_stake_status().is_online());
    }

    const OWNER: &str = "owner";
    const ADMIN: &str = "admin";
    const ACCOUNT: &str = "bob";

    #[test]
    fn basic_workflow() {
        let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);

        // the staking pool is initially offline after deployment
        let state = staking_pool.ops_stake_state();
        let state_json = serde_json::to_string_pretty(&state).unwrap();
        println!("{}", state_json);
        assert!(!state.status.is_online());

        // Assert - account has zero STAKE balance to start with
        let account_manager = account_manager();
        assert_eq!(
            staking_pool.ops_stake_balance(to_valid_account_id(ACCOUNT)),
            Some(StakeAccountBalances {
                storage_balance: StorageBalance {
                    total: account_manager.storage_balance_bounds().min,
                    available: YoctoNear::ZERO
                },
                staked: None,
                unstaked: None
            })
        );

        // Act - accounts can stake while the pool is offline
        {
            let mut ctx = ctx.clone();
            ctx.attached_deposit = YOCTO;
            testing_env!(ctx.clone());
            assert_eq!(env::account_locked_balance(), 0);
            if let PromiseOrValue::Value(balance) = staking_pool.ops_stake() {
                let staking_fee = state.staking_fee * YOCTO.into();
                assert_eq!(
                    balance.staked,
                    Some(StakedBalance {
                        stake: (YOCTO - *staking_fee).into(),
                        near_value: (YOCTO - *staking_fee).into()
                    })
                );
                assert_eq!(env::account_locked_balance(), 0);
                assert_eq!(
                    ContractNearBalances::near_balance(State::TOTAL_STAKED_BALANCE),
                    YOCTO.into()
                );
            } else {
                panic!("expected value")
            }
        }

        // Act - bring the pool online
        {
            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ADMIN.to_string();
            testing_env!(ctx);
            staking_pool.ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
            assert!(staking_pool.ops_stake_status().is_online());
            let state = staking_pool.ops_stake_state();
            println!(
                "after pool is online {}",
                serde_json::to_string_pretty(&state).unwrap()
            );
            assert!(state.status.is_online());

            let logs = test_utils::get_logs();
            println!("{:#?}", logs);
            assert_eq!(logs, vec!["[INFO] [STATUS_ONLINE] ",]);

            let receipts = deserialize_receipts();
            assert_eq!(receipts.len(), 2);
            {
                let receipt = &receipts[0];
                assert_eq!(receipt.receiver_id, env::current_account_id());
                assert_eq!(receipt.actions.len(), 1);
                let action = &receipt.actions[0];

                match action {
                    Action::Stake(action) => {
                        assert_eq!(action.stake, YOCTO);

                        assert_eq!(staking_public_key_as_string(), action.public_key);
                    }
                    _ => panic!("expected StakeAction"),
                }
            }
            {
                let receipt = &receipts[1];
                assert_eq!(receipt.receiver_id, env::current_account_id());
                assert_eq!(receipt.actions.len(), 1);
                let action = &receipt.actions[0];

                match action {
                    Action::FunctionCall(action) => {
                        assert_eq!(action.method_name, "ops_stake_start_finalize");
                        assert!(action.args.is_empty());
                        assert_eq!(action.deposit, 0);
                        assert_eq!(action.gas, *staking_pool.ops_stake_callback_gas());
                    }
                    _ => panic!("expected FunctionCall"),
                }
            }
        }
    }

    #[cfg(test)]
    mod tests_stake_online {
        use super::*;

        #[test]
        fn stake_with_zero_storage_available_balance() {
            let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);

            // Arrange
            bring_pool_online(ctx.clone(), &mut staking_pool);

            assert_eq!(State::staked_balance(), 0.into());
            assert_eq!(State::unstaked_balance(), 0.into());
            {
                let mut ctx = ctx.clone();
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.attached_deposit = 1000;
                testing_env!(ctx);
                // Act
                if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
                    panic!("expected Promise")
                }
                // Assert
                assert_eq!(State::liquidity(), YoctoNear::ZERO);
                let state = staking_pool.ops_stake_state();
                println!(
                    "staked 1000 {}",
                    serde_json::to_string_pretty(&state).unwrap()
                );
                assert_eq!(State::staked_balance(), 1000.into());

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(
                    logs,
                    vec!["[INFO] [STAKE] near_amount=1000, stake_token_amount=1000",]
                );

                let receipts = deserialize_receipts();
                assert_eq!(receipts.len(), 2);
                {
                    let receipt = &receipts[0];
                    assert_eq!(receipt.receiver_id, env::current_account_id());
                    assert_eq!(receipt.actions.len(), 1);
                    let action = &receipt.actions[0];
                    match action {
                        Action::Stake(action) => {
                            assert_eq!(action.stake, 1000);
                        }
                        _ => panic!("expected StakeAction"),
                    }
                }
                {
                    let receipt = &receipts[1];
                    assert_eq!(receipt.receiver_id, env::current_account_id());
                    assert_eq!(receipt.actions.len(), 1);
                    let action = &receipt.actions[0];
                    match action {
                        Action::FunctionCall(f) => {
                            assert_eq!(f.method_name, "ops_stake_finalize");
                            let args: StakeActionCallbackArgs =
                                serde_json::from_str(&f.args).unwrap();
                            assert_eq!(args.account_id, ACCOUNT);
                            assert_eq!(args.amount, 1000.into());
                            assert_eq!(args.stake_token_amount, 1000.into());
                        }
                        _ => panic!("expected FunctionCall"),
                    }
                }
            }
            {
                let mut ctx = ctx.clone();
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.attached_deposit = 1000;
                testing_env!(ctx);
                // Act
                if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
                    panic!("expected Promise")
                }

                // Assert
                let state = staking_pool.ops_stake_state();
                println!(
                    "staked 1000 {}",
                    serde_json::to_string_pretty(&state).unwrap()
                );
                assert_eq!(State::staked_balance(), 2000.into());
                let logs = test_utils::get_logs();
                println!("{:#?}", logs);

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(
                    logs,
                    vec!["[INFO] [STAKE] near_amount=1000, stake_token_amount=1000",]
                );

                let receipts = deserialize_receipts();
                assert_eq!(receipts.len(), 2);
                {
                    let receipt = &receipts[0];
                    assert_eq!(receipt.receiver_id, env::current_account_id());
                    assert_eq!(receipt.actions.len(), 1);
                    let action = &receipt.actions[0];
                    match action {
                        Action::Stake(action) => {
                            assert_eq!(action.stake, 2000);
                        }
                        _ => panic!("expected StakeAction"),
                    }
                }
                {
                    let receipt = &receipts[1];
                    assert_eq!(receipt.receiver_id, env::current_account_id());
                    assert_eq!(receipt.actions.len(), 1);
                    let action = &receipt.actions[0];
                    match action {
                        Action::FunctionCall(f) => {
                            assert_eq!(f.method_name, "ops_stake_finalize");
                            let args: StakeActionCallbackArgs =
                                serde_json::from_str(&f.args).unwrap();
                            assert_eq!(args.account_id, ACCOUNT);
                            assert_eq!(args.amount, 1000.into());
                            assert_eq!(args.stake_token_amount, 1000.into());
                        }
                        _ => panic!("expected FunctionCall"),
                    }
                }
            }
        }

        #[test]
        fn stake_with_storage_available_balance() {
            let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);

            // Arrange
            bring_pool_online(ctx.clone(), &mut staking_pool);

            // deposit some funds into account's storage balance
            {
                let mut ctx = ctx.clone();
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx.clone());
                let mut account_manager = account_manager();
                account_manager.storage_deposit(None, None);
            }

            {
                let mut ctx = ctx.clone();
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.attached_deposit = 1000;
                testing_env!(ctx);
                // Act
                if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
                    panic!("expected Promise")
                }
                // Assert
                assert_eq!(State::liquidity(), YoctoNear::ZERO);
                let state = staking_pool.ops_stake_state();
                println!(
                    "staked 1000 {}",
                    serde_json::to_string_pretty(&state).unwrap()
                );
                assert_eq!(State::staked_balance(), (YOCTO + 1000).into());

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(
                    logs,
                    vec![
                        "[INFO] [ACCOUNT_STORAGE_CHANGED] Withdrawal(YoctoNear(1000000000000000000000000))",
                        "[INFO] [STAKE] near_amount=1000000000000000000001000, stake_token_amount=1000000000000000000001000",
                    ]
                );

                let receipts = deserialize_receipts();
                assert_eq!(receipts.len(), 2);
                {
                    let receipt = &receipts[0];
                    assert_eq!(receipt.receiver_id, env::current_account_id());
                    assert_eq!(receipt.actions.len(), 1);
                    let action = &receipt.actions[0];
                    match action {
                        Action::Stake(action) => {
                            assert_eq!(action.stake, 1000000000000000000001000);
                        }
                        _ => panic!("expected StakeAction"),
                    }
                }
                {
                    let receipt = &receipts[1];
                    assert_eq!(receipt.receiver_id, env::current_account_id());
                    assert_eq!(receipt.actions.len(), 1);
                    let action = &receipt.actions[0];
                    match action {
                        Action::FunctionCall(f) => {
                            assert_eq!(f.method_name, "ops_stake_finalize");
                            let args: StakeActionCallbackArgs =
                                serde_json::from_str(&f.args).unwrap();
                            assert_eq!(args.account_id, ACCOUNT);
                            assert_eq!(args.amount, 1000000000000000000001000.into());
                            assert_eq!(args.stake_token_amount, 1000000000000000000001000.into());
                        }
                        _ => panic!("expected FunctionCall"),
                    }
                }
            }
        }

        #[test]
        fn staked_amount_has_near_remainder() {
            let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);

            // Arrange
            bring_pool_online(ctx.clone(), &mut staking_pool);

            let total_supply: TokenAmount = 1000.into();
            let total_staked_balance: YoctoNear = 1005.into();
            {
                testing_env!(ctx.clone());
                let mut ft_stake = ft_stake();
                ft_stake.ft_mint(ACCOUNT, total_supply);
                assert_eq!(ft_stake.ft_total_supply(), total_supply);

                ContractNearBalances::set_balance(State::STAKED_BALANCE, total_staked_balance);
            }
            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            ctx.attached_deposit = 100;
            testing_env!(ctx);
            let account = account_manager().registered_account_near_data(ACCOUNT);
            // Act
            if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
                panic!("expected Value")
            }
            let state = staking_pool.ops_stake_state();
            let logs = test_utils::get_logs();
            println!(
                "{}\n{:#?}",
                serde_json::to_string_pretty(&state).unwrap(),
                logs
            );
            assert_eq!(
                logs,
                vec![
                    "[INFO] [ACCOUNT_STORAGE_CHANGED] Deposit(YoctoNear(1))",
                    "[INFO] [STAKE] near_amount=99, stake_token_amount=99",
                ]
            );
            let account_after_staking = account_manager().registered_account_near_data(ACCOUNT);
            assert_eq!(
                account_after_staking.near_balance(),
                account.near_balance() + 1
            );

            let receipts = deserialize_receipts();
            assert_eq!(receipts.len(), 2);
            {
                let receipt = &receipts[0];
                assert_eq!(receipt.receiver_id, env::current_account_id());
                assert_eq!(receipt.actions.len(), 1);
                let action = &receipt.actions[0];
                match action {
                    Action::Stake(action) => {
                        assert_eq!(action.stake, *total_staked_balance + 99);
                    }
                    _ => panic!("expected StakeAction"),
                }
            }
            {
                let receipt = &receipts[1];
                assert_eq!(receipt.receiver_id, env::current_account_id());
                assert_eq!(receipt.actions.len(), 1);
                let action = &receipt.actions[0];
                match action {
                    Action::FunctionCall(f) => {
                        assert_eq!(f.method_name, "ops_stake_finalize");
                        let args: StakeActionCallbackArgs = serde_json::from_str(&f.args).unwrap();
                        assert_eq!(args.account_id, ACCOUNT);
                        assert_eq!(args.amount, 99.into());
                        assert_eq!(args.stake_token_amount, 99.into());
                    }
                    _ => panic!("expected FunctionCall"),
                }
            }
        }

        #[test]
        fn with_zero_stake_amount() {
            let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);

            // Arrange
            bring_pool_online(ctx.clone(), &mut staking_pool);

            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx);
            match staking_pool.ops_stake() {
                PromiseOrValue::Value(balance) => {
                    assert!(balance.staked.is_none());
                    assert_eq!(balance.storage_balance.available, YoctoNear::ZERO);
                }
                _ => panic!("expected Value"),
            }
        }

        #[test]
        fn not_enough_to_stake() {
            let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);

            // Arrange
            bring_pool_online(ctx.clone(), &mut staking_pool);

            let total_supply: TokenAmount = 1000.into();
            let total_staked_balance: YoctoNear = 1005.into();
            {
                testing_env!(ctx.clone());
                let mut ft_stake = ft_stake();
                ft_stake.ft_mint(ACCOUNT, total_supply);
                assert_eq!(ft_stake.ft_total_supply(), total_supply);

                ContractNearBalances::set_balance(State::STAKED_BALANCE, total_staked_balance);
            }
            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            ctx.attached_deposit = 1;
            testing_env!(ctx);
            let account = account_manager().registered_account_near_data(ACCOUNT);
            // Act
            match staking_pool.ops_stake() {
                PromiseOrValue::Value(balance) => {
                    assert_eq!(balance.staked.unwrap().stake, total_supply);
                    assert_eq!(balance.storage_balance.available, 1.into());
                }
                _ => panic!("expected Value"),
            }
            let state = staking_pool.ops_stake_state();
            let logs = test_utils::get_logs();
            println!(
                "{}\n{:#?}",
                serde_json::to_string_pretty(&state).unwrap(),
                logs
            );
            assert_eq!(
                logs,
                vec![
                    "[INFO] [ACCOUNT_STORAGE_CHANGED] Deposit(YoctoNear(1))",
                    "[INFO] [NOT_ENOUGH_TO_STAKE] ",
                ]
            );
            let account_after_staking = account_manager().registered_account_near_data(ACCOUNT);
            assert_eq!(
                account_after_staking.near_balance(),
                account.near_balance() + 1
            );

            let receipts = deserialize_receipts();
            assert!(receipts.is_empty());
        }

        #[test]
        fn with_liquidity_needed() {
            let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);

            // Arrange
            bring_pool_online(ctx.clone(), &mut staking_pool);
            // simulate some unstaked balance
            testing_env!(ctx.clone());
            State::incr_total_unstaked_balance((10 * YOCTO).into());

            // Act
            {
                let mut ctx = ctx.clone();
                ctx.attached_deposit = YOCTO;
                ctx.predecessor_account_id = ACCOUNT.to_string();
                testing_env!(ctx.clone());
                if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
                    panic!("expected Promise")
                }
            }
            // Assert
            assert_eq!(State::liquidity(), YOCTO.into());
            assert_eq!(State::total_unstaked_balance(), (9 * YOCTO).into());
            let logs = test_utils::get_logs();
            println!("{:#?}", logs);
            assert_eq!(logs, vec![
                "[INFO] [LIQUIDITY] added=1000000000000000000000000, total=1000000000000000000000000",
                "[INFO] [STAKE] near_amount=1000000000000000000000000, stake_token_amount=1000000000000000000000000",
            ]);

            // Act
            {
                let mut ctx = ctx.clone();
                ctx.attached_deposit = YOCTO;
                ctx.predecessor_account_id = ACCOUNT.to_string();
                testing_env!(ctx);
                if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
                    panic!("expected Promise")
                }
            }
            // Assert
            let logs = test_utils::get_logs();
            println!("{:#?}", logs);
            assert_eq!(logs, vec![
                "[INFO] [LIQUIDITY] added=1000000000000000000000000, total=2000000000000000000000000",
                "[INFO] [STAKE] near_amount=1000000000000000000000000, stake_token_amount=1000000000000000000000000",
            ]);

            assert_eq!(State::liquidity(), (2 * YOCTO).into());
            assert_eq!(State::total_unstaked_balance(), (8 * YOCTO).into());
        }
    }

    #[cfg(test)]
    mod tests_stake_offline {
        use super::*;

        #[test]
        fn stake_with_zero_storage_available_balance() {
            let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);

            // Arrange
            assert_eq!(State::staked_balance(), 0.into());
            assert_eq!(State::unstaked_balance(), 0.into());
            {
                let mut ctx = ctx.clone();
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.attached_deposit = 1000;
                testing_env!(ctx);
                // Act
                let state = staking_pool.ops_stake_state();
                println!(
                    "before staking {}\ncurrent treasury stake balance: {}",
                    serde_json::to_string_pretty(&state).unwrap(),
                    ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()))
                );
                if let PromiseOrValue::Value(balances) = staking_pool.ops_stake() {
                    println!("{:#?}", balances);
                    let staked_balance = balances.staked.unwrap();
                    let staking_fee = state.staking_fee * 1000.into();
                    assert_eq!(staked_balance.stake, (1000 - *staking_fee).into());
                    assert_eq!(staked_balance.near_value, (1000 - *staking_fee).into());
                } else {
                    panic!("expected Value");
                }
                // Assert
                assert_eq!(State::liquidity(), YoctoNear::ZERO);
                let state = staking_pool.ops_stake_state();
                println!(
                    "staked 1000 {}",
                    serde_json::to_string_pretty(&state).unwrap()
                );
                assert_eq!(State::staked_balance(), 0.into());
                assert_eq!(state.treasury_balance, state.staking_fee * 1000.into());

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(
                    logs,
                    vec![
                        "[INFO] [STAKE] near_amount=1000, stake_token_amount=1000",
                        "[WARN] [STATUS_OFFLINE] ",
                        "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
                        "[INFO] [FT_MINT] account: bob, amount: 1000",
                        "[INFO] [FT_BURN] account: bob, amount: 8",
                        "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
                        "[INFO] [FT_MINT] account: contract.near, amount: 8",
                    ]
                );

                let receipts = deserialize_receipts();
                assert!(receipts.is_empty());
            }
            {
                let mut ctx = ctx.clone();
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.attached_deposit = 1000;
                testing_env!(ctx);
                // Act
                if let PromiseOrValue::Promise(_) = staking_pool.ops_stake() {
                    panic!("expected Value")
                }

                // Assert
                let state = staking_pool.ops_stake_state();
                println!(
                    "staked another 1000 {}",
                    serde_json::to_string_pretty(&state).unwrap()
                );
                assert_eq!(State::staked_balance(), 0.into());
                let logs = test_utils::get_logs();
                println!("{:#?}", logs);

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(
                    logs,
                    vec![
                        "[INFO] [STAKE] near_amount=1000, stake_token_amount=1000",
                        "[WARN] [STATUS_OFFLINE] ",
                        "[INFO] [FT_MINT] account: bob, amount: 1000",
                        "[INFO] [FT_BURN] account: bob, amount: 8",
                        "[INFO] [FT_MINT] account: contract.near, amount: 8",
                    ]
                );

                let receipts = deserialize_receipts();
                assert!(receipts.is_empty());
            }
        }

        #[test]
        fn stake_with_storage_available_balance() {
            let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);

            // Arrange

            // deposit some funds into account's storage balance
            {
                let mut ctx = ctx.clone();
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx.clone());
                let mut account_manager = account_manager();
                account_manager.storage_deposit(None, None);
            }

            {
                let mut ctx = ctx.clone();
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.attached_deposit = 1000;
                testing_env!(ctx);
                // Act
                let state = staking_pool.ops_stake_state();
                if let PromiseOrValue::Value(balances) = staking_pool.ops_stake() {
                    println!("{:#?}", balances);
                    let staked_balances = balances.staked.unwrap();
                    let staking_fee = state.staking_fee * (YOCTO + 1000).into();
                    assert_eq!(staked_balances.stake, (YOCTO + 1000 - *staking_fee).into());
                    assert_eq!(
                        staked_balances.near_value,
                        (YOCTO + 1000 - *staking_fee).into()
                    );
                } else {
                    panic!("expected Promise")
                }
                // Assert
                assert_eq!(State::liquidity(), YoctoNear::ZERO);
                let state = staking_pool.ops_stake_state();
                println!(
                    "staked 1000 {}",
                    serde_json::to_string_pretty(&state).unwrap()
                );
                assert_eq!(State::staked_balance(), 0.into());

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(
                    logs,
                    vec![
                        "[INFO] [ACCOUNT_STORAGE_CHANGED] Withdrawal(YoctoNear(1000000000000000000000000))",
                        "[INFO] [STAKE] near_amount=1000000000000000000001000, stake_token_amount=1000000000000000000001000",
                        "[WARN] [STATUS_OFFLINE] ",
                        "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
                        "[INFO] [FT_MINT] account: bob, amount: 1000000000000000000001000",
                        "[INFO] [FT_BURN] account: bob, amount: 8000000000000000000008",
                        "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
                        "[INFO] [FT_MINT] account: contract.near, amount: 8000000000000000000008",
                    ]
                );

                assert!(deserialize_receipts().is_empty());
            }
        }

        #[test]
        fn staked_amount_has_near_remainder() {
            let (mut ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);

            // Arrange
            let total_supply: TokenAmount = 1000.into();
            let total_staked_balance: YoctoNear = 1005.into();
            {
                testing_env!(ctx.clone());
                let mut ft_stake = ft_stake();
                ft_stake.ft_mint(ACCOUNT, total_supply);
                assert_eq!(ft_stake.ft_total_supply(), total_supply);

                ContractNearBalances::set_balance(
                    State::TOTAL_STAKED_BALANCE,
                    total_staked_balance,
                );
            }
            ctx.predecessor_account_id = ACCOUNT.to_string();
            ctx.attached_deposit = 100;
            testing_env!(ctx.clone());
            let account = account_manager().registered_account_near_data(ACCOUNT);
            // Act
            let state = staking_pool.ops_stake_state();
            println!(
                "before staking: {}",
                serde_json::to_string_pretty(&state).unwrap()
            );
            if let PromiseOrValue::Value(balances) = staking_pool.ops_stake() {
                println!("{:#?}", balances);
                let staked_balances = balances.staked.unwrap();
                assert_eq!(staked_balances.stake, (1000 + (1000 * 100 / 1005)).into());
                assert_eq!(
                    staked_balances.near_value,
                    ((1000 + (1000 * 100 / 1005)) * 1005 / 1000).into()
                );
            } else {
                panic!("expected Value")
            }
            let state = staking_pool.ops_stake_state();
            let logs = test_utils::get_logs();
            println!(
                "{}\n{:#?}",
                serde_json::to_string_pretty(&state).unwrap(),
                logs
            );
            assert_eq!(
                logs,
                vec![
                    "[INFO] [ACCOUNT_STORAGE_CHANGED] Deposit(YoctoNear(1))",
                    "[INFO] [STAKE] near_amount=99, stake_token_amount=99",
                    "[WARN] [STATUS_OFFLINE] ",
                    "[INFO] [FT_MINT] account: bob, amount: 99",
                ]
            );
            let account_after_staking = account_manager().registered_account_near_data(ACCOUNT);
            assert_eq!(
                account_after_staking.near_balance(),
                account.near_balance() + 1
            );

            assert!(deserialize_receipts().is_empty());
        }

        #[test]
        fn with_zero_stake_amount() {
            let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);

            // Arrange
            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx);
            match staking_pool.ops_stake() {
                PromiseOrValue::Value(balance) => {
                    assert!(balance.staked.is_none());
                    assert_eq!(balance.storage_balance.available, YoctoNear::ZERO);
                }
                _ => panic!("expected Value"),
            }
        }

        #[test]
        fn not_enough_to_stake() {
            let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);

            // Arrange
            let total_supply: TokenAmount = 1000.into();
            let total_staked_balance: YoctoNear = 1005.into();
            {
                testing_env!(ctx.clone());
                let mut ft_stake = ft_stake();
                ft_stake.ft_mint(ACCOUNT, total_supply);
                assert_eq!(ft_stake.ft_total_supply(), total_supply);

                ContractNearBalances::set_balance(
                    State::TOTAL_STAKED_BALANCE,
                    total_staked_balance,
                );
            }
            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            ctx.attached_deposit = 1;
            testing_env!(ctx);
            let account = account_manager().registered_account_near_data(ACCOUNT);
            // Act
            match staking_pool.ops_stake() {
                PromiseOrValue::Value(balance) => {
                    assert_eq!(balance.staked.unwrap().stake, total_supply);
                    assert_eq!(balance.storage_balance.available, 1.into());
                }
                _ => panic!("expected Value"),
            }
            let state = staking_pool.ops_stake_state();
            let logs = test_utils::get_logs();
            println!(
                "{}\n{:#?}",
                serde_json::to_string_pretty(&state).unwrap(),
                logs
            );
            assert_eq!(
                logs,
                vec![
                    "[INFO] [ACCOUNT_STORAGE_CHANGED] Deposit(YoctoNear(1))",
                    "[INFO] [NOT_ENOUGH_TO_STAKE] ",
                ]
            );
            let account_after_staking = account_manager().registered_account_near_data(ACCOUNT);
            assert_eq!(
                account_after_staking.near_balance(),
                account.near_balance() + 1
            );

            let receipts = deserialize_receipts();
            assert!(receipts.is_empty());
        }

        #[test]
        fn with_liquidity_needed() {
            let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);

            // Arrange
            // simulate some unstaked balance
            testing_env!(ctx.clone());
            State::incr_total_unstaked_balance((10 * YOCTO).into());

            // Act
            {
                let mut ctx = ctx.clone();
                ctx.attached_deposit = YOCTO;
                ctx.predecessor_account_id = ACCOUNT.to_string();
                testing_env!(ctx.clone());
                if let PromiseOrValue::Value(balances) = staking_pool.ops_stake() {
                    println!("{:#?}", balances);
                    let state = staking_pool.ops_stake_state();
                    let staking_fee = state.staking_fee * YOCTO.into();
                    assert_eq!(
                        balances.staked.unwrap(),
                        StakedBalance {
                            stake: (YOCTO - *staking_fee).into(),
                            near_value: (YOCTO - *staking_fee).into()
                        }
                    );
                } else {
                    panic!("expected Promise")
                }
            }
            // Assert
            assert_eq!(State::liquidity(), YOCTO.into());
            assert_eq!(State::total_unstaked_balance(), (9 * YOCTO).into());
            let logs = test_utils::get_logs();
            println!("{:#?}", logs);
            assert_eq!(logs, vec![
                "[INFO] [LIQUIDITY] added=1000000000000000000000000, total=1000000000000000000000000",
                "[INFO] [STAKE] near_amount=1000000000000000000000000, stake_token_amount=1000000000000000000000000",
                "[WARN] [STATUS_OFFLINE] ",
                "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
                "[INFO] [FT_MINT] account: bob, amount: 1000000000000000000000000",
                "[INFO] [FT_BURN] account: bob, amount: 8000000000000000000000",
                "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
                "[INFO] [FT_MINT] account: contract.near, amount: 8000000000000000000000",
            ]);

            // Act
            {
                let mut ctx = ctx.clone();
                ctx.attached_deposit = YOCTO;
                ctx.predecessor_account_id = ACCOUNT.to_string();
                testing_env!(ctx);
                if let PromiseOrValue::Value(balances) = staking_pool.ops_stake() {
                    let state = staking_pool.ops_stake_state();
                    let staking_fee = state.staking_fee * YOCTO.into();
                    assert_eq!(
                        balances.staked.unwrap(),
                        StakedBalance {
                            stake: (2 * YOCTO - (2 * *staking_fee)).into(),
                            near_value: (2 * YOCTO - (2 * *staking_fee)).into()
                        }
                    );
                } else {
                    panic!("expected Promise")
                }
            }
            // Assert
            let logs = test_utils::get_logs();
            println!("{:#?}", logs);
            assert_eq!(logs, vec![
                "[INFO] [LIQUIDITY] added=1000000000000000000000000, total=2000000000000000000000000",
                "[INFO] [STAKE] near_amount=1000000000000000000000000, stake_token_amount=1000000000000000000000000",
                "[WARN] [STATUS_OFFLINE] ",
                "[INFO] [FT_MINT] account: bob, amount: 1000000000000000000000000",
                "[INFO] [FT_BURN] account: bob, amount: 8000000000000000000000",
                "[INFO] [FT_MINT] account: contract.near, amount: 8000000000000000000000",
            ]);

            assert_eq!(State::liquidity(), (2 * YOCTO).into());
            assert_eq!(State::total_unstaked_balance(), (8 * YOCTO).into());
        }
    }

    #[cfg(test)]
    mod tests_stake_finalize {
        use super::*;

        #[test]
        fn stake_action_success() {
            let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);

            // Arrange
            bring_pool_online(ctx.clone(), &mut staking_pool);
            // stake
            {
                let mut ctx = ctx.clone();
                ctx.attached_deposit = YOCTO;
                ctx.predecessor_account_id = ACCOUNT.to_string();
                testing_env!(ctx.clone());
                if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
                    panic!("expected Promise")
                }
            }
            let logs = test_utils::get_logs();
            println!("{:#?}", logs);

            // Act
            {
                let state = StakingPoolComponent::state();
                let mut ctx = ctx.clone();
                ctx.attached_deposit = 0;
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.account_locked_balance = *state.total_staked_balance();
                testing_env_with_promise_result_success(ctx.clone());
                let balances = staking_pool.ops_stake_finalize(
                    ACCOUNT.to_string(),
                    YOCTO.into(),
                    YOCTO.into(),
                );
                // Assert
                let logs = test_utils::get_logs();
                println!("ops_stake_finalize: {:#?}", logs);
                println!("{:#?}", balances);
                {
                    let staked_balance = balances.staked.unwrap();
                    let expected_amount = YOCTO - *(state.staking_fee * YOCTO.into());
                    assert_eq!(staked_balance.stake, expected_amount.into());
                    assert_eq!(staked_balance.near_value, expected_amount.into());
                }
                assert!(balances.unstaked.is_none());
                let state = staking_pool.ops_stake_state();
                println!("{}", serde_json::to_string_pretty(&state).unwrap());
                assert_eq!(State::staked_balance(), YoctoNear::ZERO);
                assert_eq!(state.total_staked_balance(), YOCTO.into());
            }
        }

        #[test]
        fn stake_action_success_with_dividend_payout() {
            // Arrange
            let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);
            bring_pool_online(ctx.clone(), &mut staking_pool);

            // stake
            {
                let mut ctx = ctx.clone();
                ctx.attached_deposit = YOCTO;
                ctx.predecessor_account_id = ACCOUNT.to_string();
                testing_env!(ctx.clone());
                if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
                    panic!("expected Promise")
                }
                println!("{:#?}", test_utils::get_logs());
            }
            // finalize stake
            {
                let state = StakingPoolComponent::state();
                let mut ctx = ctx.clone();
                ctx.attached_deposit = 0;
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.account_locked_balance = *state.total_staked_balance();
                testing_env_with_promise_result_success(ctx.clone());
                staking_pool.ops_stake_finalize(ACCOUNT.to_string(), YOCTO.into(), YOCTO.into());
                println!("{:#?}", test_utils::get_logs());
            }

            let state = staking_pool.ops_stake_state();
            println!(
                "after stake finalized {}",
                serde_json::to_string_pretty(&state).unwrap()
            );
            assert_eq!(
                state.treasury_balance,
                state.staking_fee * state.total_staked_balance()
            );
            println!("finalized stake {:#?}", StakingPoolBalances::load());

            // stake - with staking rewards issued - 50 yoctoNEAR staking rewards
            const STAKING_REWARDS: u128 = 500;
            {
                let mut ctx = ctx.clone();
                ctx.attached_deposit = YOCTO;
                ctx.account_locked_balance = *state.total_staked_balance() + STAKING_REWARDS;
                ctx.predecessor_account_id = ACCOUNT.to_string();
                testing_env!(ctx.clone());
                if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
                    panic!("expected Promise")
                }
                println!("with staking rewards {:#?}", test_utils::get_logs());
                deserialize_receipts();
            }

            println!("stake with rewards {:#?}", StakingPoolBalances::load());

            let state = staking_pool.ops_stake_state();
            println!(
                "before finalize dividend payout {}",
                serde_json::to_string_pretty(&state).unwrap()
            );
            // Act - finalize stake
            {
                let staking_pool_balances = StakingPoolBalances::load();
                println!(
                    "staked with earned staking rewards {:#?}",
                    staking_pool_balances
                );
                let state = StakingPoolComponent::state();
                let mut ctx = ctx.clone();
                ctx.attached_deposit = 0;
                ctx.account_locked_balance = *state.total_staked_balance();
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.account_locked_balance = *state.total_staked_balance();
                testing_env_with_promise_result_success(ctx.clone());
                staking_pool.ops_stake_finalize(
                    ACCOUNT.to_string(),
                    State::staked_balance(),
                    999999999999999999999500.into(),
                );
                let staking_pool_balances = StakingPoolBalances::load();
                println!("ops_stake_finalize {:#?}", staking_pool_balances);
                let logs = test_utils::get_logs();
                println!("ops_stake_finalize: {:#?}", logs);
                assert_eq!(
                    logs,
                    vec![
                        "[INFO] [FT_MINT] account: bob, amount: 999999999999999999999500",
                        "[INFO] [FT_BURN] account: contract.near, amount: 2",
                        "[INFO] [TREASURY_DIVIDEND] 3 NEAR / 2 STAKE",
                        "[INFO] [FT_BURN] account: bob, amount: 7999999999999999999994",
                        "[INFO] [FT_MINT] account: contract.near, amount: 7999999999999999999994",
                    ]
                );
                let state = staking_pool.ops_stake_state();
                println!("{}", serde_json::to_string_pretty(&state).unwrap());
            }
            let state_after_stake_finalized = staking_pool.ops_stake_state();
            println!(
                "after finalize dividend payout {}",
                serde_json::to_string_pretty(&state_after_stake_finalized).unwrap()
            );
            assert_eq!(
                state_after_stake_finalized.treasury_balance,
                state.staking_fee * (2 * YOCTO).into()
            )
        }

        #[test]
        fn stake_action_failed() {
            let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);

            // Arrange
            bring_pool_online(ctx.clone(), &mut staking_pool);
            // stake
            {
                let mut ctx = ctx.clone();
                ctx.attached_deposit = YOCTO;
                ctx.predecessor_account_id = ACCOUNT.to_string();
                testing_env!(ctx.clone());
                if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
                    panic!("expected Promise")
                }
            }
            let logs = test_utils::get_logs();
            println!("{:#?}", logs);

            // Act
            {
                let state = StakingPoolComponent::state();
                let mut ctx = ctx.clone();
                ctx.attached_deposit = YOCTO;
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.account_locked_balance = *state.total_staked_balance();
                testing_env_with_promise_result_failure(ctx);
                let balances = staking_pool.ops_stake_finalize(
                    ACCOUNT.to_string(),
                    YOCTO.into(),
                    YOCTO.into(),
                );
                // Assert
                let logs = test_utils::get_logs();
                println!("ops_stake_finalize: {:#?}", logs);
                assert_eq!(
                    logs,
                    vec![
                        "[ERR] [STAKE_ACTION_FAILED] ",
                        "[WARN] [STATUS_OFFLINE] StakeActionFailed",
                        "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
                        "[INFO] [FT_MINT] account: bob, amount: 1000000000000000000000000",
                        "[INFO] [FT_BURN] account: bob, amount: 8000000000000000000000",
                        "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
                        "[INFO] [FT_MINT] account: contract.near, amount: 8000000000000000000000",
                    ]
                );
                assert_eq!(
                    staking_pool.ops_stake_status(),
                    Status::Offline(OfflineReason::StakeActionFailed)
                );
                println!("{:#?}", balances);
                {
                    let staked_balance = balances.staked.unwrap();
                    let expected_amount = YOCTO - *(state.staking_fee * YOCTO.into());
                    assert_eq!(staked_balance.stake, expected_amount.into());
                    assert_eq!(staked_balance.near_value, expected_amount.into());
                }
                assert!(balances.unstaked.is_none());

                assert_eq!(
                    ContractNearBalances::near_balance(State::TOTAL_STAKED_BALANCE),
                    YOCTO.into()
                );

                let state = staking_pool.ops_stake_state();
                println!("{}", serde_json::to_string_pretty(&state).unwrap());
                assert_eq!(State::staked_balance(), YoctoNear::ZERO);
                assert_eq!(state.total_staked_balance(), YOCTO.into());

                let receipts = deserialize_receipts();
                assert_eq!(receipts.len(), 2);
                {
                    let receipt = &receipts[0];
                    assert_eq!(receipt.receiver_id, env::current_account_id());
                    assert_eq!(receipt.actions.len(), 1);
                    let action = &receipt.actions[0];
                    match action {
                        Action::Stake(action) => {
                            assert_eq!(action.stake, 0);
                        }
                        _ => panic!("expected stake action"),
                    };
                }
                {
                    let receipt = &receipts[1];
                    assert_eq!(receipt.receiver_id, env::current_account_id());
                    assert_eq!(receipt.actions.len(), 1);
                    let action = &receipt.actions[0];
                    match action {
                        Action::FunctionCall(function_call) => {
                            assert_eq!(function_call.method_name, "ops_stake_stop_finalize");
                        }
                        _ => panic!("expected FunctionCallAction"),
                    };
                }
            }
        }
    }

    #[cfg(test)]
    mod tests_unstake_online {
        use super::*;

        #[test]
        fn specified_amount() {
            let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);

            // Arrange
            bring_pool_online(ctx.clone(), &mut staking_pool);
            // stake
            {
                let mut ctx = ctx.clone();
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx);
                // Act
                if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
                    panic!("expected Promise")
                }
                println!("staked {:#?}", StakingPoolBalances::load());
            }
            // finalize stake
            {
                let state = StakingPoolComponent::state();
                let mut ctx = ctx.clone();
                ctx.attached_deposit = 0;
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.account_locked_balance = *state.total_staked_balance();
                testing_env_with_promise_result_success(ctx.clone());
                let balances = staking_pool.ops_stake_finalize(
                    ACCOUNT.to_string(),
                    YOCTO.into(),
                    YOCTO.into(),
                );
                println!("{:#?}", balances);
                println!("ops_stake_finalize {:#?}", StakingPoolBalances::load());
            }
            let state = staking_pool.ops_stake_state();
            println!(
                "before unstaking {}",
                serde_json::to_string_pretty(&state).unwrap()
            );
            // Act
            {
                let mut ctx = ctx.clone();
                ctx.account_locked_balance = *StakingPoolBalances::load().total_staked;
                testing_env!(ctx);
                if let PromiseOrValue::Value(_) = staking_pool.ops_unstake(Some(1000.into())) {
                    panic!("expected Promise");
                }
                println!("ops_unstake {:#?}", StakingPoolBalances::load());
                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(
                    logs,
                    vec![
                        "[INFO] [UNSTAKE] near_amount=1000, stake_token_amount=1000",
                        "[INFO] [FT_LOCK] account: bob, amount: 1000",
                    ]
                );
                assert_eq!(ft_stake().ft_locked_balance(ACCOUNT).unwrap(), 1000.into());
                let stake_balance = YOCTO - *(state.staking_fee * YOCTO.into());
                assert_eq!(
                    ft_stake().ft_balance_of(to_valid_account_id(ACCOUNT)),
                    (stake_balance - 1000).into()
                );
                let state = staking_pool.ops_stake_state();
                println!("{}", serde_json::to_string_pretty(&state).unwrap());
                assert_eq!(state.total_staked_balance(), (YOCTO - 1000).into());
                assert_eq!(State::unstaked_balance(), 1000.into());

                let receipts = deserialize_receipts();
                assert_eq!(receipts.len(), 2);
                {
                    let receipt = &receipts[0];
                    assert_eq!(receipt.receiver_id, env::current_account_id());
                    assert_eq!(receipt.actions.len(), 1);
                    let action = &receipt.actions[0];
                    match action {
                        Action::Stake(action) => {
                            assert_eq!(action.stake, YOCTO - 1000);
                        }
                        _ => panic!("expected StakeAction"),
                    }
                }
                {
                    let receipt = &receipts[1];
                    assert_eq!(receipt.receiver_id, env::current_account_id());
                    assert_eq!(receipt.actions.len(), 1);
                    let action = &receipt.actions[0];
                    match action {
                        Action::FunctionCall(f) => {
                            assert_eq!(f.method_name, "ops_unstake_finalize");
                            let args: StakeActionCallbackArgs =
                                serde_json::from_str(&f.args).unwrap();
                            assert_eq!(args.account_id, ACCOUNT);
                            assert_eq!(args.amount, 1000.into());
                            assert_eq!(args.stake_token_amount, 1000.into());
                        }
                        _ => panic!("expected FunctionCall"),
                    }
                }
            }
        }

        #[test]
        fn total_account_staked_balance() {
            let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);

            // Arrange
            bring_pool_online(ctx.clone(), &mut staking_pool);
            // stake
            {
                let mut ctx = ctx.clone();
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx);
                // Act
                if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
                    panic!("expected Promise")
                }
            }
            // finalize stake
            {
                let state = StakingPoolComponent::state();
                let mut ctx = ctx.clone();
                ctx.attached_deposit = 0;
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.account_locked_balance = *state.total_staked_balance();
                testing_env_with_promise_result_success(ctx.clone());
                let balances = staking_pool.ops_stake_finalize(
                    ACCOUNT.to_string(),
                    YOCTO.into(),
                    YOCTO.into(),
                );
                println!("{:#?}", balances);
            }
            let state = staking_pool.ops_stake_state();
            println!(
                "before unstaking {}",
                serde_json::to_string_pretty(&state).unwrap()
            );
            // Act
            {
                let mut ctx = ctx.clone();
                ctx.account_locked_balance = *StakingPoolBalances::load().total_staked;
                testing_env!(ctx);
                if let PromiseOrValue::Value(_) = staking_pool.ops_unstake(None) {
                    panic!("expected Promise");
                }
                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(
                    logs,
                    vec![
                        "[INFO] [UNSTAKE] near_amount=992000000000000000000000, stake_token_amount=992000000000000000000000",
                        "[INFO] [FT_LOCK] account: bob, amount: 992000000000000000000000",
                    ]
                );
                let expected_locked_balance = YOCTO - *(state.staking_fee * YOCTO.into());
                assert_eq!(
                    ft_stake().ft_locked_balance(ACCOUNT).unwrap(),
                    expected_locked_balance.into()
                );
                assert_eq!(
                    ft_stake().ft_balance_of(to_valid_account_id(ACCOUNT)),
                    0.into()
                );
                let state = staking_pool.ops_stake_state();
                println!("{}", serde_json::to_string_pretty(&state).unwrap());
                assert_eq!(state.total_staked_balance(), state.treasury_balance);
                assert_eq!(State::unstaked_balance(), expected_locked_balance.into());

                let receipts = deserialize_receipts();
                assert_eq!(receipts.len(), 2);
                {
                    let receipt = &receipts[0];
                    assert_eq!(receipt.receiver_id, env::current_account_id());
                    assert_eq!(receipt.actions.len(), 1);
                    let action = &receipt.actions[0];
                    match action {
                        Action::Stake(action) => {
                            assert_eq!(action.stake, *(state.staking_fee * YOCTO.into()));
                        }
                        _ => panic!("expected StakeAction"),
                    }
                }
                {
                    let receipt = &receipts[1];
                    assert_eq!(receipt.receiver_id, env::current_account_id());
                    assert_eq!(receipt.actions.len(), 1);
                    let action = &receipt.actions[0];
                    match action {
                        Action::FunctionCall(f) => {
                            assert_eq!(f.method_name, "ops_unstake_finalize");
                            let args: StakeActionCallbackArgs =
                                serde_json::from_str(&f.args).unwrap();
                            assert_eq!(args.account_id, ACCOUNT);
                            assert_eq!(args.amount, expected_locked_balance.into());
                            assert_eq!(args.stake_token_amount, expected_locked_balance.into());
                        }
                        _ => panic!("expected FunctionCall"),
                    }
                }
            }
        }

        #[test]
        #[should_panic(expected = "[ERR] [INSUFFICIENT_FUNDS]")]
        fn insufficient_staked_funds() {
            let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);
            bring_pool_online(ctx.clone(), &mut staking_pool);

            // Act
            testing_env!(ctx.clone());
            staking_pool.ops_unstake(Some(YOCTO.into()));
        }

        #[test]
        #[should_panic(expected = "[ERR] [ACCOUNT_NOT_REGISTERED]")]
        fn account_not_registered() {
            let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, false);
            bring_pool_online(ctx.clone(), &mut staking_pool);
            testing_env!(ctx.clone());
            staking_pool.ops_unstake(None);
        }
    }

    #[cfg(test)]
    mod tests_unstake_finalize {
        use super::*;

        #[test]
        fn stake_action_success() {
            let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);

            // Arrange
            bring_pool_online(ctx.clone(), &mut staking_pool);
            // stake 3 NEAR
            {
                let mut ctx = ctx.clone();
                ctx.attached_deposit = 3 * YOCTO;
                ctx.predecessor_account_id = ACCOUNT.to_string();
                testing_env!(ctx.clone());
                if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
                    panic!("expected Promise")
                }
            }
            let logs = test_utils::get_logs();
            println!("{:#?}", logs);

            // finalize stake
            {
                let state = StakingPoolComponent::state();
                let mut ctx = ctx.clone();
                ctx.attached_deposit = 0;
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.account_locked_balance = 3 * YOCTO;
                testing_env_with_promise_result_success(ctx.clone());
                let balances = staking_pool.ops_stake_finalize(
                    ACCOUNT.to_string(),
                    (3 * YOCTO).into(),
                    (3 * YOCTO).into(),
                );
                // Assert
                let logs = test_utils::get_logs();
                println!("ops_stake_finalize: {:#?}", logs);
                println!("{:#?}", balances);
                {
                    let staked_balance = balances.staked.unwrap();
                    let expected_amount = (3 * YOCTO) - *(state.staking_fee * (3 * YOCTO).into());
                    assert_eq!(staked_balance.stake, expected_amount.into());
                    assert_eq!(staked_balance.near_value, expected_amount.into());
                }
                assert!(balances.unstaked.is_none());
                let state = staking_pool.ops_stake_state();
                println!("{}", serde_json::to_string_pretty(&state).unwrap());
                assert_eq!(State::staked_balance(), YoctoNear::ZERO);
                assert_eq!(state.total_staked_balance(), (3 * YOCTO).into());
            }

            // unstake 2 NEAR
            {
                let mut ctx = ctx.clone();
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.account_locked_balance = *staking_pool.ops_stake_pool_balances().total_staked;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Value(_) = staking_pool.ops_unstake(Some((2 * YOCTO).into()))
                {
                    panic!("expected Promise")
                }
                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
            }

            // finalize the unstaked funds
            {
                let mut ctx = ctx.clone();
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.account_locked_balance = *staking_pool.ops_stake_pool_balances().total_staked;
                testing_env_with_promise_result_success(ctx);
                let stake_account_balances = staking_pool.ops_unstake_finalize(
                    ACCOUNT.to_string(),
                    (2 * YOCTO).into(),
                    (2 * YOCTO).into(),
                );
                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(
                    logs,
                    vec![
                        "[INFO] [FT_BURN] account: bob, amount: 2000000000000000000000000",
                        "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(116)",
                    ]
                );
                assert_eq!(
                    stake_account_balances.staked.as_ref().unwrap().stake,
                    (YOCTO - *staking_pool.ops_stake_state().treasury_balance).into() // subtract staking fees
                );
                assert_eq!(
                    stake_account_balances.unstaked.as_ref().unwrap().total,
                    (2 * YOCTO).into()
                );
                assert_eq!(
                    stake_account_balances.unstaked.as_ref().unwrap().available,
                    YoctoNear::ZERO
                );
            }
        }

        #[test]
        fn stake_action_failed() {
            let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);

            // Arrange
            bring_pool_online(ctx.clone(), &mut staking_pool);
            // stake 3 NEAR
            {
                let mut ctx = ctx.clone();
                ctx.attached_deposit = 3 * YOCTO;
                ctx.predecessor_account_id = ACCOUNT.to_string();
                testing_env!(ctx.clone());
                if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
                    panic!("expected Promise")
                }
            }
            let logs = test_utils::get_logs();
            println!("{:#?}", logs);

            // finalize stake
            {
                let mut ctx = ctx.clone();
                ctx.attached_deposit = 0;
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.account_locked_balance = 3 * YOCTO;
                testing_env_with_promise_result_success(ctx.clone());
                staking_pool.ops_stake_finalize(
                    ACCOUNT.to_string(),
                    (3 * YOCTO).into(),
                    (3 * YOCTO).into(),
                );
                // Assert
                let logs = test_utils::get_logs();
                println!("ops_stake_finalize: {:#?}", logs);
            }

            // unstake 2 NEAR
            {
                let mut ctx = ctx.clone();
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.account_locked_balance = *staking_pool.ops_stake_pool_balances().total_staked;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Value(_) = staking_pool.ops_unstake(Some((2 * YOCTO).into()))
                {
                    panic!("expected Promise")
                }
                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
            }

            // finalize the unstaked funds
            {
                let mut ctx = ctx.clone();
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.account_locked_balance = *staking_pool.ops_stake_pool_balances().total_staked;
                testing_env_with_promise_result_failure(ctx);
                let stake_account_balances = staking_pool.ops_unstake_finalize(
                    ACCOUNT.to_string(),
                    (2 * YOCTO).into(),
                    (2 * YOCTO).into(),
                );
                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(
                    logs,
                    vec![
                        "[ERR] [STAKE_ACTION_FAILED] ",
                        "[WARN] [STATUS_OFFLINE] StakeActionFailed",
                        "[INFO] [FT_BURN] account: bob, amount: 2000000000000000000000000",
                        "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(116)",
                    ]
                );
                assert_eq!(
                    stake_account_balances.staked.as_ref().unwrap().stake,
                    (YOCTO - *staking_pool.ops_stake_state().treasury_balance).into() // subtract staking fees
                );
                assert_eq!(
                    stake_account_balances.unstaked.as_ref().unwrap().total,
                    (2 * YOCTO).into()
                );
                assert_eq!(
                    stake_account_balances.unstaked.as_ref().unwrap().available,
                    YoctoNear::ZERO
                );
                assert_eq!(
                    staking_pool.ops_stake_status(),
                    Status::Offline(OfflineReason::StakeActionFailed)
                );
            }
        }
    }

    #[cfg(test)]
    mod tests_restake_online {
        use super::*;

        #[test]
        fn all_with_unstaked_funds() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();
            bring_pool_online(ctx.clone(), &mut staking_pool);
            // stake
            {
                let mut ctx = ctx.clone();
                ctx.attached_deposit = YOCTO;
                ctx.account_locked_balance = 0;
                testing_env!(ctx);
                println!("before staking {:#?}", StakingPoolBalances::load());
                staking_pool.ops_stake();
                println!("staked {:#?}", StakingPoolBalances::load());
            }

            // finalize stake
            {
                let state = staking_pool.ops_stake_state();
                println!("{}", serde_json::to_string_pretty(&state).unwrap());
                let mut ctx = ctx.clone();
                ctx.attached_deposit = 0;
                ctx.account_locked_balance = *state.total_staked_balance();
                testing_env_with_promise_result_success(ctx.clone());
                staking_pool.ops_stake_finalize(
                    ctx.predecessor_account_id,
                    State::staked_balance(),
                    staking_pool.near_stake_value_rounded_down(State::staked_balance()),
                );
                println!("finalize stake {:#?}", StakingPoolBalances::load());
                let logs = test_utils::get_logs();
                println!("finalized stake: {:#?}", logs);
            }

            // unstake all
            {
                let state = staking_pool.ops_stake_state();
                println!("{}", serde_json::to_string_pretty(&state).unwrap());
                let mut ctx = ctx.clone();
                ctx.account_locked_balance = *state.total_staked_balance();
                testing_env!(ctx);
                staking_pool.ops_unstake(None);
                println!("unstaked {:#?}", StakingPoolBalances::load());
                let logs = test_utils::get_logs();
                println!("unstake: {:#?}", logs);
            }

            // finalize unstaking
            {
                let state = staking_pool.ops_stake_state();
                println!("{}", serde_json::to_string_pretty(&state).unwrap());
                let mut ctx = ctx.clone();
                ctx.account_locked_balance = *state.treasury_balance;
                testing_env_with_promise_result_success(ctx.clone());
                staking_pool.ops_unstake_finalize(
                    ctx.predecessor_account_id,
                    State::unstaked_balance(),
                    (*State::unstaked_balance()).into(),
                );
                println!("finalized unstaking {:#?}", StakingPoolBalances::load());
                let logs = test_utils::get_logs();
                println!("finalized unstake {:#?}", logs);
                let state = staking_pool.ops_stake_state();
                println!("{}", serde_json::to_string_pretty(&state).unwrap());
            }

            assert_eq!(
                ft_stake().ft_balance_of(to_valid_account_id(ACCOUNT)),
                TokenAmount::ZERO
            );

            // Act - restake
            {
                let mut ctx = ctx.clone();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx);
                let account = account_manager().registered_account_data(ACCOUNT);
                assert!(account.locked().is_some());
                staking_pool.ops_restake(None);
                println!("restaked {:#?}", StakingPoolBalances::load());
                let logs = test_utils::get_logs();
                println!("restaked all: {:#?}", logs);
                assert_eq!(logs, vec![
                    "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(-24)", // storage for unstaked balances was freed up
                    "[INFO] [STAKE] near_amount=992000000000000000000000, stake_token_amount=992000000000000000000000",
                ]);
                let account = account_manager().registered_account_data(ACCOUNT);
                assert!(account.locked().is_none());
            }
        }

        #[test]
        fn all_with_zero_unstaked_funds() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();
            bring_pool_online(ctx.clone(), &mut staking_pool);

            // stake
            {
                let mut ctx = ctx.clone();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx);
                staking_pool.ops_stake();
            }
            // finalize stake
            let balances = {
                let state = staking_pool.ops_stake_state();
                println!("{}", serde_json::to_string_pretty(&state).unwrap());
                let mut ctx = ctx.clone();
                ctx.attached_deposit = 0;
                ctx.account_locked_balance = *state.total_staked_balance();
                testing_env_with_promise_result_success(ctx.clone());
                let balances = staking_pool.ops_stake_finalize(
                    ctx.predecessor_account_id,
                    State::staked_balance(),
                    staking_pool.near_stake_value_rounded_down(State::staked_balance()),
                );
                let logs = test_utils::get_logs();
                println!("finalized stake: {:#?}", logs);
                balances
            };

            // Act
            {
                let mut ctx = ctx.clone();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx);
                if let PromiseOrValue::Value(balances_after_restaking) =
                    staking_pool.ops_restake(None)
                {
                    assert_eq!(balances_after_restaking, balances);
                } else {
                    panic!("expected Value");
                }
                assert!(test_utils::get_logs().is_empty());
            }
        }

        #[test]
        fn all_with_zero_staked_and_unstaked_funds() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();
            bring_pool_online(ctx.clone(), &mut staking_pool);
            if let PromiseOrValue::Value(balance) = staking_pool.ops_restake(None) {
                assert!(balance.staked.is_none());
                assert!(balance.unstaked.is_none());
            } else {
                panic!("expected Value");
            }
        }

        #[test]
        fn partial_with_unstaked_funds() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();
            bring_pool_online(ctx.clone(), &mut staking_pool);
            // stake
            {
                let mut ctx = ctx.clone();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx);
                staking_pool.ops_stake();
            }
            // finalize stake
            {
                let state = staking_pool.ops_stake_state();
                println!("{}", serde_json::to_string_pretty(&state).unwrap());
                let mut ctx = ctx.clone();
                ctx.attached_deposit = 0;
                ctx.account_locked_balance = *state.total_staked_balance();
                testing_env_with_promise_result_success(ctx.clone());
                staking_pool.ops_stake_finalize(
                    ctx.predecessor_account_id,
                    State::staked_balance(),
                    staking_pool.near_stake_value_rounded_down(State::staked_balance()),
                );
                let logs = test_utils::get_logs();
                println!("finalized stake: {:#?}", logs);
            }
            // unstake all
            {
                let state = staking_pool.ops_stake_state();
                println!("{}", serde_json::to_string_pretty(&state).unwrap());
                let mut ctx = ctx.clone();
                ctx.account_locked_balance = *state.total_staked_balance();
                testing_env!(ctx);
                staking_pool.ops_unstake(None);
                let logs = test_utils::get_logs();
                println!("unstake: {:#?}", logs);
            }
            // finalize unstaking
            {
                let state = staking_pool.ops_stake_state();
                println!("{}", serde_json::to_string_pretty(&state).unwrap());
                let mut ctx = ctx.clone();
                ctx.account_locked_balance = *state.treasury_balance;
                testing_env_with_promise_result_success(ctx.clone());
                staking_pool.ops_unstake_finalize(
                    ctx.predecessor_account_id,
                    State::unstaked_balance(),
                    (*State::unstaked_balance()).into(),
                );
                let logs = test_utils::get_logs();
                println!("finalized unstake {:#?}", logs);
                let state = staking_pool.ops_stake_state();
                println!("{}", serde_json::to_string_pretty(&state).unwrap());
            }
            assert_eq!(
                ft_stake().ft_balance_of(to_valid_account_id(ACCOUNT)),
                TokenAmount::ZERO
            );

            // Act - restake
            {
                let mut ctx = ctx.clone();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx);
                let account_before_restaking = account_manager().registered_account_data(ACCOUNT);
                assert!(account_before_restaking.locked().is_some());
                staking_pool.ops_restake(Some(10000.into()));
                let logs = test_utils::get_logs();
                println!("restaked: {:#?}", logs);
                assert_eq!(
                    logs,
                    vec!["[INFO] [STAKE] near_amount=10000, stake_token_amount=10000",]
                );
                let account = account_manager().registered_account_data(ACCOUNT);
                assert_eq!(
                    account.locked_balance(),
                    account_before_restaking.locked_balance() - 10000
                );
            }
        }

        #[test]
        #[should_panic(expected = "[ERR] [INSUFFICIENT_FUNDS]")]
        fn partial_insufficient_unstaked_funds() {
            let (mut ctx, mut staking_pool) = deploy_with_registered_account();
            bring_pool_online(ctx.clone(), &mut staking_pool);

            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx);
            staking_pool.ops_restake(Some(YOCTO.into()));
        }

        #[test]
        #[should_panic(expected = "[ERR] [ACCOUNT_NOT_REGISTERED]")]
        fn all_with_unregistered_account() {
            let (mut ctx, mut staking_pool) = deploy_with_unregistered_account();
            bring_pool_online(ctx.clone(), &mut staking_pool);

            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx);
            staking_pool.ops_restake(None);
        }

        #[test]
        #[should_panic(expected = "[ERR] [ACCOUNT_NOT_REGISTERED]")]
        fn partial_with_unregistered_account() {
            let (mut ctx, mut staking_pool) = deploy_with_unregistered_account();
            bring_pool_online(ctx.clone(), &mut staking_pool);

            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx);
            staking_pool.ops_restake(Some(YOCTO.into()));
        }
    }

    #[cfg(test)]
    mod tests_restake_offline {
        use super::*;

        #[test]
        fn all_with_unstaked_funds() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();
            // stake
            {
                let mut ctx = ctx.clone();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx);
                staking_pool.ops_stake();
            }
            // unstake all
            {
                let state = staking_pool.ops_stake_state();
                println!("{}", serde_json::to_string_pretty(&state).unwrap());
                let mut ctx = ctx.clone();
                ctx.account_locked_balance = *state.total_staked_balance();
                testing_env!(ctx);
                staking_pool.ops_unstake(None);
                let logs = test_utils::get_logs();
                println!("unstake: {:#?}", logs);
            }

            // Act - restake
            {
                let mut ctx = ctx.clone();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx);
                let account = account_manager().registered_account_data(ACCOUNT);
                assert!(account.locked().is_some());
                staking_pool.ops_restake(None);
                let account = account_manager().registered_account_data(ACCOUNT);
                assert!(account.locked().is_none());

                let logs = test_utils::get_logs();
                println!("restaked all: {:#?}", logs);
                assert_eq!(logs, vec![
                    "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(-24)",
                    "[INFO] [STAKE] near_amount=992000000000000000000000, stake_token_amount=992000000000000000000000",
                    "[WARN] [STATUS_OFFLINE] ",
                    "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
                    "[INFO] [FT_MINT] account: bob, amount: 992000000000000000000000",
                    "[INFO] [FT_BURN] account: bob, amount: 7936000000000000000000",
                    "[INFO] [FT_MINT] account: contract.near, amount: 7936000000000000000000",
                ]);
            }
        }

        #[test]
        fn all_with_zero_unstaked_funds() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();

            // stake
            let balances = {
                let mut ctx = ctx.clone();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx);
                if let PromiseOrValue::Value(balances) = staking_pool.ops_stake() {
                    balances
                } else {
                    unreachable!();
                }
            };

            // Act
            {
                let mut ctx = ctx.clone();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx);
                if let PromiseOrValue::Value(balances_after_restaking) =
                    staking_pool.ops_restake(None)
                {
                    assert_eq!(balances_after_restaking, balances);
                } else {
                    panic!("expected Value");
                }
                assert!(test_utils::get_logs().is_empty());
            }
        }

        #[test]
        fn all_with_zero_staked_and_unstaked_funds() {
            let (_ctx, mut staking_pool) = deploy_with_registered_account();
            if let PromiseOrValue::Value(balance) = staking_pool.ops_restake(None) {
                assert!(balance.staked.is_none());
                assert!(balance.unstaked.is_none());
            } else {
                panic!("expected Value");
            }
        }

        #[test]
        fn partial_with_unstaked_funds() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();
            // stake
            {
                let mut ctx = ctx.clone();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx);
                staking_pool.ops_stake();
            }
            // unstake all
            {
                let state = staking_pool.ops_stake_state();
                println!("{}", serde_json::to_string_pretty(&state).unwrap());
                let mut ctx = ctx.clone();
                ctx.account_locked_balance = *state.total_staked_balance();
                testing_env!(ctx);
                staking_pool.ops_unstake(None);
                let logs = test_utils::get_logs();
                println!("unstake: {:#?}", logs);
            }
            assert_eq!(
                ft_stake().ft_balance_of(to_valid_account_id(ACCOUNT)),
                TokenAmount::ZERO
            );

            // Act - restake
            {
                let mut ctx = ctx.clone();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx);
                let account_before_restaking = account_manager().registered_account_data(ACCOUNT);
                assert!(account_before_restaking.locked().is_some());
                staking_pool.ops_restake(Some(10000.into()));
                let logs = test_utils::get_logs();
                println!("restaked: {:#?}", logs);
                assert_eq!(
                    logs,
                    vec![
                        "[INFO] [STAKE] near_amount=10000, stake_token_amount=10000",
                        "[WARN] [STATUS_OFFLINE] ",
                        "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
                        "[INFO] [FT_MINT] account: bob, amount: 10000",
                        "[INFO] [FT_BURN] account: bob, amount: 80",
                        "[INFO] [FT_MINT] account: contract.near, amount: 80",
                    ]
                );
                let account = account_manager().registered_account_data(ACCOUNT);
                assert_eq!(
                    account.locked_balance(),
                    account_before_restaking.locked_balance() - 10000
                );
            }
        }

        #[test]
        #[should_panic(expected = "[ERR] [INSUFFICIENT_FUNDS]")]
        fn partial_insufficient_unstaked_funds() {
            let (mut ctx, mut staking_pool) = deploy_with_registered_account();

            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx);
            staking_pool.ops_restake(Some(YOCTO.into()));
        }

        #[test]
        #[should_panic(expected = "[ERR] [ACCOUNT_NOT_REGISTERED]")]
        fn all_with_unregistered_account() {
            let (mut ctx, mut staking_pool) = deploy_with_unregistered_account();

            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx);
            staking_pool.ops_restake(None);
        }

        #[test]
        #[should_panic(expected = "[ERR] [ACCOUNT_NOT_REGISTERED]")]
        fn partial_with_unregistered_account() {
            let (mut ctx, mut staking_pool) = deploy_with_unregistered_account();

            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx);
            staking_pool.ops_restake(Some(YOCTO.into()));
        }
    }

    #[cfg(test)]
    mod tests_withdraw_online {
        use super::*;

        fn stake_unstake(
            ctx: VMContext,
            staking_pool: &mut StakingPoolComponent,
            stake_amount: YoctoNear,
            unstake_amount: YoctoNear,
        ) {
            let stake_amount = *stake_amount;
            if stake_amount == 0 {
                return;
            }
            let unstake_amount = *unstake_amount;
            // stake
            {
                let mut ctx = ctx.clone();
                ctx.attached_deposit = stake_amount;
                testing_env!(ctx);
                staking_pool.ops_stake();
            }
            // finalize stake
            {
                let state = staking_pool.ops_stake_state();
                println!("{}", serde_json::to_string_pretty(&state).unwrap());
                let mut ctx = ctx.clone();
                ctx.attached_deposit = 0;
                ctx.account_locked_balance = *state.total_staked_balance();
                testing_env_with_promise_result_success(ctx.clone());
                staking_pool.ops_stake_finalize(
                    ctx.predecessor_account_id,
                    State::staked_balance(),
                    staking_pool.near_stake_value_rounded_down(State::staked_balance()),
                );
                let logs = test_utils::get_logs();
                println!("finalized stake: {:#?}", logs);
            }
            if unstake_amount > 0 {
                // unstake
                {
                    let state = staking_pool.ops_stake_state();
                    println!("{}", serde_json::to_string_pretty(&state).unwrap());
                    let mut ctx = ctx.clone();
                    ctx.account_locked_balance = *state.total_staked_balance();
                    testing_env!(ctx);
                    staking_pool.ops_unstake(Some((unstake_amount).into()));
                    let logs = test_utils::get_logs();
                    println!("unstake: {:#?}", logs);
                }
                // finalize unstaking
                {
                    let state = staking_pool.ops_stake_state();
                    println!("{}", serde_json::to_string_pretty(&state).unwrap());
                    let mut ctx = ctx.clone();
                    let total_staked_balance = state.treasury_balance
                        + (stake_amount - *state.treasury_balance - unstake_amount);
                    ctx.account_locked_balance = *total_staked_balance;
                    testing_env_with_promise_result_success(ctx.clone());
                    staking_pool.ops_unstake_finalize(
                        ctx.predecessor_account_id,
                        State::unstaked_balance(),
                        (*State::unstaked_balance()).into(),
                    );
                    let logs = test_utils::get_logs();
                    println!("finalized unstake {:#?}", logs);
                    let state = staking_pool.ops_stake_state();
                    println!("{}", serde_json::to_string_pretty(&state).unwrap());
                }
            }
        }

        #[test]
        fn all_with_available_unstaked_funds() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();
            bring_pool_online(ctx.clone(), &mut staking_pool);

            stake_unstake(
                ctx.clone(),
                &mut staking_pool,
                YOCTO.into(),
                (YOCTO / 2).into(),
            );
            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            ctx.epoch_height = ctx.epoch_height + 4;
            testing_env!(ctx.clone());
            let starting_balances = staking_pool
                .ops_stake_balance(to_valid_account_id(ACCOUNT))
                .unwrap();
            assert_eq!(
                starting_balances.unstaked.as_ref().unwrap().total,
                starting_balances.unstaked.as_ref().unwrap().available
            );
            let balances = staking_pool.ops_stake_withdraw(None);
            println!("{:#?}", balances);
            assert!(balances.unstaked.is_none());

            let receipts = deserialize_receipts();
            assert_eq!(receipts.len(), 1);
            let receipt = &receipts[0];
            assert_eq!(receipt.receiver_id, ACCOUNT.to_string());
            assert_eq!(receipt.actions.len(), 1);
            let action = &receipt.actions[0];
            if let Action::Transfer(transfer) = action {
                assert_eq!(transfer.deposit, YOCTO / 2);
            } else {
                panic!("expected transfer action");
            }
        }

        #[test]
        fn all_with_locked_unstaked_funds_and_zero_available() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();
            bring_pool_online(ctx.clone(), &mut staking_pool);

            stake_unstake(
                ctx.clone(),
                &mut staking_pool,
                YOCTO.into(),
                (YOCTO / 2).into(),
            );
            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx.clone());
            let starting_balances = staking_pool
                .ops_stake_balance(to_valid_account_id(ACCOUNT))
                .unwrap();
            assert_eq!(
                starting_balances.unstaked.as_ref().unwrap().available,
                YoctoNear::ZERO
            );
            let balances = staking_pool.ops_stake_withdraw(None);
            assert_eq!(balances, starting_balances);
            assert!(deserialize_receipts().is_empty())
        }

        #[test]
        fn all_with_locked_unstaked_funds_with_liquidity_fully_available() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();
            bring_pool_online(ctx.clone(), &mut staking_pool);

            stake_unstake(
                ctx.clone(),
                &mut staking_pool,
                YOCTO.into(),
                (YOCTO / 2).into(),
            );
            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx.clone());
            State::add_liquidity((YOCTO * 3 / 4).into());
            assert_eq!(State::liquidity(), (YOCTO / 2).into());
            let starting_balances = staking_pool
                .ops_stake_balance(to_valid_account_id(ACCOUNT))
                .unwrap();
            assert_eq!(
                starting_balances.unstaked.as_ref().unwrap().available,
                YoctoNear::ZERO
            );
            let balances = staking_pool.ops_stake_withdraw(None);
            println!("{:#?}", balances);
            assert!(balances.unstaked.is_none());

            let receipts = deserialize_receipts();
            assert_eq!(receipts.len(), 1);
            let receipt = &receipts[0];
            assert_eq!(receipt.receiver_id, ACCOUNT.to_string());
            assert_eq!(receipt.actions.len(), 1);
            let action = &receipt.actions[0];
            if let Action::Transfer(transfer) = action {
                assert_eq!(transfer.deposit, YOCTO / 2);
            } else {
                panic!("expected transfer action");
            }
            assert_eq!(State::liquidity(), YoctoNear::ZERO);
        }

        #[test]
        fn all_with_locked_unstaked_funds_with_liquidity_partially_available_with_unlocked_funds() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();
            bring_pool_online(ctx.clone(), &mut staking_pool);

            stake_unstake(
                ctx.clone(),
                &mut staking_pool,
                YOCTO.into(),
                (YOCTO / 2).into(),
            );
            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx.clone());
            State::add_liquidity((YOCTO / 4).into());
            assert_eq!(State::liquidity(), (YOCTO / 4).into());
            let starting_balances = staking_pool
                .ops_stake_balance(to_valid_account_id(ACCOUNT))
                .unwrap();
            assert_eq!(
                starting_balances.unstaked.as_ref().unwrap().available,
                YoctoNear::ZERO
            );

            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            ctx.epoch_height += 4;
            testing_env!(ctx.clone());
            let balances = staking_pool.ops_stake_withdraw(None);
            println!("{:#?}", balances);
            assert!(balances.unstaked.is_none());

            let receipts = deserialize_receipts();
            assert_eq!(receipts.len(), 1);
            let receipt = &receipts[0];
            assert_eq!(receipt.receiver_id, ACCOUNT.to_string());
            assert_eq!(receipt.actions.len(), 1);
            let action = &receipt.actions[0];
            if let Action::Transfer(transfer) = action {
                assert_eq!(transfer.deposit, YOCTO / 2);
            } else {
                panic!("expected transfer action");
            }
            assert_eq!(State::liquidity(), YoctoNear::ZERO);
        }

        #[test]
        fn all_with_zero_unstaked_funds() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();
            bring_pool_online(ctx.clone(), &mut staking_pool);

            stake_unstake(ctx.clone(), &mut staking_pool, YOCTO.into(), 0.into());
            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx.clone());

            let starting_balances = staking_pool
                .ops_stake_balance(to_valid_account_id(ACCOUNT))
                .unwrap();

            let balances = staking_pool.ops_stake_withdraw(None);
            assert_eq!(balances, starting_balances);

            assert!(test_utils::get_logs().is_empty());
            assert!(deserialize_receipts().is_empty());
        }

        #[test]
        fn all_with_zero_staked_and_unstaked_funds() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();
            bring_pool_online(ctx.clone(), &mut staking_pool);

            stake_unstake(ctx.clone(), &mut staking_pool, 0.into(), 0.into());
            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx.clone());

            let starting_balances = staking_pool
                .ops_stake_balance(to_valid_account_id(ACCOUNT))
                .unwrap();

            let balances = staking_pool.ops_stake_withdraw(None);
            assert_eq!(balances, starting_balances);

            assert!(test_utils::get_logs().is_empty());
            assert!(deserialize_receipts().is_empty());
        }

        #[test]
        #[should_panic(expected = "[ERR] [ACCOUNT_NOT_REGISTERED]")]
        fn all_with_unregistered_account() {
            let (ctx, mut staking_pool) = deploy_with_unregistered_account();
            bring_pool_online(ctx.clone(), &mut staking_pool);

            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx);
            staking_pool.ops_stake_withdraw(None);
        }

        #[test]
        fn partial_with_available_unstaked_funds() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();
            bring_pool_online(ctx.clone(), &mut staking_pool);

            stake_unstake(
                ctx.clone(),
                &mut staking_pool,
                YOCTO.into(),
                (YOCTO / 2).into(),
            );
            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            ctx.epoch_height = ctx.epoch_height + 4;
            testing_env!(ctx.clone());
            let starting_balances = staking_pool
                .ops_stake_balance(to_valid_account_id(ACCOUNT))
                .unwrap();
            assert_eq!(
                starting_balances.unstaked.as_ref().unwrap().total,
                starting_balances.unstaked.as_ref().unwrap().available
            );
            let balances = staking_pool.ops_stake_withdraw(Some((YOCTO / 8).into()));
            println!("{:#?}", balances);
            assert_eq!(balances.unstaked.unwrap().available, (YOCTO * 3 / 8).into());

            let receipts = deserialize_receipts();
            assert_eq!(receipts.len(), 1);
            let receipt = &receipts[0];
            assert_eq!(receipt.receiver_id, ACCOUNT.to_string());
            assert_eq!(receipt.actions.len(), 1);
            let action = &receipt.actions[0];
            if let Action::Transfer(transfer) = action {
                assert_eq!(transfer.deposit, YOCTO / 8);
            } else {
                panic!("expected transfer action");
            }
        }

        #[test]
        #[should_panic(expected = "[ERR] [INSUFFICIENT_FUNDS]")]
        fn partial_with_locked_unstaked_funds_and_zero_available() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();
            bring_pool_online(ctx.clone(), &mut staking_pool);

            stake_unstake(
                ctx.clone(),
                &mut staking_pool,
                YOCTO.into(),
                (YOCTO / 2).into(),
            );
            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx.clone());
            staking_pool.ops_stake_withdraw(Some((YOCTO / 8).into()));
        }

        #[test]
        fn partial_with_locked_unstaked_funds_with_liquidity_fully_available() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();
            bring_pool_online(ctx.clone(), &mut staking_pool);

            stake_unstake(
                ctx.clone(),
                &mut staking_pool,
                YOCTO.into(),
                (YOCTO / 2).into(),
            );
            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx.clone());
            State::add_liquidity(YOCTO.into());
            assert_eq!(State::liquidity(), (YOCTO / 2).into());
            let starting_balances = staking_pool
                .ops_stake_balance(to_valid_account_id(ACCOUNT))
                .unwrap();
            assert_eq!(
                starting_balances.unstaked.as_ref().unwrap().available,
                YoctoNear::ZERO
            );
            let balances = staking_pool.ops_stake_withdraw(Some((YOCTO / 8).into()));
            println!("{:#?}", balances);
            assert_eq!(balances.unstaked.unwrap().total, (YOCTO * 3 / 8).into());

            let receipts = deserialize_receipts();
            assert_eq!(receipts.len(), 1);
            let receipt = &receipts[0];
            assert_eq!(receipt.receiver_id, ACCOUNT.to_string());
            assert_eq!(receipt.actions.len(), 1);
            let action = &receipt.actions[0];
            if let Action::Transfer(transfer) = action {
                assert_eq!(transfer.deposit, YOCTO / 8);
            } else {
                panic!("expected transfer action");
            }
            assert_eq!(State::liquidity(), (YOCTO * 3 / 8).into());
        }

        #[test]
        fn partial_with_locked_unstaked_funds_with_liquidity_partially_available_and_unlocked_funds(
        ) {
            let (ctx, mut staking_pool) = deploy_with_registered_account();
            bring_pool_online(ctx.clone(), &mut staking_pool);

            stake_unstake(
                ctx.clone(),
                &mut staking_pool,
                YOCTO.into(),
                (YOCTO / 2).into(),
            );
            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            ctx.epoch_height += 4;
            testing_env!(ctx.clone());
            State::add_liquidity((YOCTO / 16).into());
            assert_eq!(State::liquidity(), (YOCTO / 16).into());
            let starting_balances = staking_pool
                .ops_stake_balance(to_valid_account_id(ACCOUNT))
                .unwrap();
            assert_eq!(
                starting_balances.unstaked.as_ref().unwrap().available,
                (YOCTO / 2).into()
            );
            let balances = staking_pool.ops_stake_withdraw(Some((YOCTO / 8).into()));
            println!("{:#?}", balances);
            assert_eq!(balances.unstaked.unwrap().total, (YOCTO * 3 / 8).into());

            let receipts = deserialize_receipts();
            assert_eq!(receipts.len(), 1);
            let receipt = &receipts[0];
            assert_eq!(receipt.receiver_id, ACCOUNT.to_string());
            assert_eq!(receipt.actions.len(), 1);
            let action = &receipt.actions[0];
            if let Action::Transfer(transfer) = action {
                assert_eq!(transfer.deposit, YOCTO / 8);
            } else {
                panic!("expected transfer action");
            }
            assert_eq!(State::liquidity(), YoctoNear::ZERO);
        }

        #[test]
        #[should_panic(expected = "[ERR] [INSUFFICIENT_FUNDS]")]
        fn partial_with_insufficient_funds() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();
            bring_pool_online(ctx.clone(), &mut staking_pool);

            stake_unstake(
                ctx.clone(),
                &mut staking_pool,
                YOCTO.into(),
                (YOCTO / 8).into(),
            );
            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx.clone());
            staking_pool.ops_stake_withdraw(Some(YOCTO.into()));
        }

        #[test]
        #[should_panic(expected = "[ERR] [INSUFFICIENT_FUNDS]")]
        fn partial_with_zero_unstaked_funds() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();
            bring_pool_online(ctx.clone(), &mut staking_pool);

            stake_unstake(
                ctx.clone(),
                &mut staking_pool,
                YOCTO.into(),
                YoctoNear::ZERO,
            );
            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx.clone());
            staking_pool.ops_stake_withdraw(Some((YOCTO / 8).into()));
        }

        #[test]
        #[should_panic(expected = "[ERR] [INSUFFICIENT_FUNDS]")]
        fn partial_with_zero_staked_and_unstaked_funds() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();
            bring_pool_online(ctx.clone(), &mut staking_pool);

            stake_unstake(
                ctx.clone(),
                &mut staking_pool,
                YoctoNear::ZERO,
                YoctoNear::ZERO,
            );
            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx.clone());
            staking_pool.ops_stake_withdraw(Some((YOCTO / 8).into()));
        }

        #[test]
        #[should_panic(expected = "[ERR] [ACCOUNT_NOT_REGISTERED]")]
        fn partial_with_unregistered_account() {
            let (ctx, mut staking_pool) = deploy_with_unregistered_account();
            bring_pool_online(ctx.clone(), &mut staking_pool);

            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx.clone());
            staking_pool.ops_stake_withdraw(Some((YOCTO / 8).into()));
        }
    }

    #[cfg(test)]
    mod tests_withdraw_offline {
        use super::*;

        fn stake_unstake(
            ctx: VMContext,
            staking_pool: &mut StakingPoolComponent,
            stake_amount: YoctoNear,
            unstake_amount: YoctoNear,
        ) {
            let stake_amount = *stake_amount;
            if stake_amount == 0 {
                return;
            }
            let unstake_amount = *unstake_amount;
            // stake
            {
                let mut ctx = ctx.clone();
                ctx.attached_deposit = stake_amount;
                testing_env!(ctx);
                staking_pool.ops_stake();
            }
            if unstake_amount > 0 {
                // unstake
                {
                    let state = staking_pool.ops_stake_state();
                    println!("{}", serde_json::to_string_pretty(&state).unwrap());
                    let mut ctx = ctx.clone();
                    ctx.account_locked_balance = *state.total_staked_balance();
                    testing_env!(ctx);
                    staking_pool.ops_unstake(Some((unstake_amount).into()));
                    let logs = test_utils::get_logs();
                    println!("unstake: {:#?}", logs);
                }
            }
        }

        #[test]
        fn all_with_available_unstaked_funds() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();

            stake_unstake(
                ctx.clone(),
                &mut staking_pool,
                YOCTO.into(),
                (YOCTO / 2).into(),
            );
            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            ctx.epoch_height = ctx.epoch_height + 4;
            testing_env!(ctx.clone());
            let starting_balances = staking_pool
                .ops_stake_balance(to_valid_account_id(ACCOUNT))
                .unwrap();
            assert_eq!(
                starting_balances.unstaked.as_ref().unwrap().total,
                starting_balances.unstaked.as_ref().unwrap().available
            );
            let balances = staking_pool.ops_stake_withdraw(None);
            println!("{:#?}", balances);
            assert!(balances.unstaked.is_none());

            let receipts = deserialize_receipts();
            assert_eq!(receipts.len(), 1);
            let receipt = &receipts[0];
            assert_eq!(receipt.receiver_id, ACCOUNT.to_string());
            assert_eq!(receipt.actions.len(), 1);
            let action = &receipt.actions[0];
            if let Action::Transfer(transfer) = action {
                assert_eq!(transfer.deposit, YOCTO / 2);
            } else {
                panic!("expected transfer action");
            }
        }

        #[test]
        fn all_with_locked_unstaked_funds_and_zero_available() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();

            stake_unstake(
                ctx.clone(),
                &mut staking_pool,
                YOCTO.into(),
                (YOCTO / 2).into(),
            );
            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx.clone());
            let starting_balances = staking_pool
                .ops_stake_balance(to_valid_account_id(ACCOUNT))
                .unwrap();
            assert_eq!(
                starting_balances.unstaked.as_ref().unwrap().available,
                YoctoNear::ZERO
            );
            let balances = staking_pool.ops_stake_withdraw(None);
            assert_eq!(balances, starting_balances);
            assert!(deserialize_receipts().is_empty())
        }

        #[test]
        fn all_with_locked_unstaked_funds_with_liquidity_fully_available() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();

            stake_unstake(
                ctx.clone(),
                &mut staking_pool,
                YOCTO.into(),
                (YOCTO / 2).into(),
            );
            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx.clone());
            State::add_liquidity((YOCTO * 3 / 4).into());
            assert_eq!(State::liquidity(), (YOCTO / 2).into());
            let starting_balances = staking_pool
                .ops_stake_balance(to_valid_account_id(ACCOUNT))
                .unwrap();
            assert_eq!(
                starting_balances.unstaked.as_ref().unwrap().available,
                YoctoNear::ZERO
            );
            let balances = staking_pool.ops_stake_withdraw(None);
            println!("{:#?}", balances);
            assert!(balances.unstaked.is_none());

            let receipts = deserialize_receipts();
            assert_eq!(receipts.len(), 1);
            let receipt = &receipts[0];
            assert_eq!(receipt.receiver_id, ACCOUNT.to_string());
            assert_eq!(receipt.actions.len(), 1);
            let action = &receipt.actions[0];
            if let Action::Transfer(transfer) = action {
                assert_eq!(transfer.deposit, YOCTO / 2);
            } else {
                panic!("expected transfer action");
            }
            assert_eq!(State::liquidity(), YoctoNear::ZERO);
        }

        #[test]
        fn all_with_locked_unstaked_funds_with_liquidity_partially_available_with_unlocked_funds() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();

            stake_unstake(
                ctx.clone(),
                &mut staking_pool,
                YOCTO.into(),
                (YOCTO / 2).into(),
            );
            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx.clone());
            State::add_liquidity((YOCTO / 4).into());
            assert_eq!(State::liquidity(), (YOCTO / 4).into());
            let starting_balances = staking_pool
                .ops_stake_balance(to_valid_account_id(ACCOUNT))
                .unwrap();
            assert_eq!(
                starting_balances.unstaked.as_ref().unwrap().available,
                YoctoNear::ZERO
            );

            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            ctx.epoch_height += 4;
            testing_env!(ctx.clone());
            let balances = staking_pool.ops_stake_withdraw(None);
            println!("{:#?}", balances);
            assert!(balances.unstaked.is_none());

            let receipts = deserialize_receipts();
            assert_eq!(receipts.len(), 1);
            let receipt = &receipts[0];
            assert_eq!(receipt.receiver_id, ACCOUNT.to_string());
            assert_eq!(receipt.actions.len(), 1);
            let action = &receipt.actions[0];
            if let Action::Transfer(transfer) = action {
                assert_eq!(transfer.deposit, YOCTO / 2);
            } else {
                panic!("expected transfer action");
            }
            assert_eq!(State::liquidity(), YoctoNear::ZERO);
        }

        #[test]
        fn all_with_zero_unstaked_funds() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();

            stake_unstake(ctx.clone(), &mut staking_pool, YOCTO.into(), 0.into());
            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx.clone());

            let starting_balances = staking_pool
                .ops_stake_balance(to_valid_account_id(ACCOUNT))
                .unwrap();

            let balances = staking_pool.ops_stake_withdraw(None);
            assert_eq!(balances, starting_balances);

            assert!(test_utils::get_logs().is_empty());
            assert!(deserialize_receipts().is_empty());
        }

        #[test]
        fn all_with_zero_staked_and_unstaked_funds() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();

            stake_unstake(ctx.clone(), &mut staking_pool, 0.into(), 0.into());
            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx.clone());

            let starting_balances = staking_pool
                .ops_stake_balance(to_valid_account_id(ACCOUNT))
                .unwrap();

            let balances = staking_pool.ops_stake_withdraw(None);
            assert_eq!(balances, starting_balances);

            assert!(test_utils::get_logs().is_empty());
            assert!(deserialize_receipts().is_empty());
        }

        #[test]
        #[should_panic(expected = "[ERR] [ACCOUNT_NOT_REGISTERED]")]
        fn all_with_unregistered_account() {
            let (ctx, mut staking_pool) = deploy_with_unregistered_account();

            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx);
            staking_pool.ops_stake_withdraw(None);
        }

        #[test]
        fn partial_with_available_unstaked_funds() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();

            stake_unstake(
                ctx.clone(),
                &mut staking_pool,
                YOCTO.into(),
                (YOCTO / 2).into(),
            );
            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            ctx.epoch_height = ctx.epoch_height + 4;
            testing_env!(ctx.clone());
            let starting_balances = staking_pool
                .ops_stake_balance(to_valid_account_id(ACCOUNT))
                .unwrap();
            assert_eq!(
                starting_balances.unstaked.as_ref().unwrap().total,
                starting_balances.unstaked.as_ref().unwrap().available
            );
            let balances = staking_pool.ops_stake_withdraw(Some((YOCTO / 8).into()));
            println!("{:#?}", balances);
            assert_eq!(balances.unstaked.unwrap().available, (YOCTO * 3 / 8).into());

            let receipts = deserialize_receipts();
            assert_eq!(receipts.len(), 1);
            let receipt = &receipts[0];
            assert_eq!(receipt.receiver_id, ACCOUNT.to_string());
            assert_eq!(receipt.actions.len(), 1);
            let action = &receipt.actions[0];
            if let Action::Transfer(transfer) = action {
                assert_eq!(transfer.deposit, YOCTO / 8);
            } else {
                panic!("expected transfer action");
            }
        }

        #[test]
        #[should_panic(expected = "[ERR] [INSUFFICIENT_FUNDS]")]
        fn partial_with_locked_unstaked_funds_and_zero_available() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();

            stake_unstake(
                ctx.clone(),
                &mut staking_pool,
                YOCTO.into(),
                (YOCTO / 2).into(),
            );
            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx.clone());
            staking_pool.ops_stake_withdraw(Some((YOCTO / 8).into()));
        }

        #[test]
        fn partial_with_locked_unstaked_funds_with_liquidity_fully_available() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();

            stake_unstake(
                ctx.clone(),
                &mut staking_pool,
                YOCTO.into(),
                (YOCTO / 2).into(),
            );
            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx.clone());
            State::add_liquidity(YOCTO.into());
            assert_eq!(State::liquidity(), (YOCTO / 2).into());
            let starting_balances = staking_pool
                .ops_stake_balance(to_valid_account_id(ACCOUNT))
                .unwrap();
            assert_eq!(
                starting_balances.unstaked.as_ref().unwrap().available,
                YoctoNear::ZERO
            );
            let balances = staking_pool.ops_stake_withdraw(Some((YOCTO / 8).into()));
            println!("{:#?}", balances);
            assert_eq!(balances.unstaked.unwrap().total, (YOCTO * 3 / 8).into());

            let receipts = deserialize_receipts();
            assert_eq!(receipts.len(), 1);
            let receipt = &receipts[0];
            assert_eq!(receipt.receiver_id, ACCOUNT.to_string());
            assert_eq!(receipt.actions.len(), 1);
            let action = &receipt.actions[0];
            if let Action::Transfer(transfer) = action {
                assert_eq!(transfer.deposit, YOCTO / 8);
            } else {
                panic!("expected transfer action");
            }
            assert_eq!(State::liquidity(), (YOCTO * 3 / 8).into());
        }

        #[test]
        fn partial_with_locked_unstaked_funds_with_liquidity_partially_available_and_unlocked_funds(
        ) {
            let (ctx, mut staking_pool) = deploy_with_registered_account();

            stake_unstake(
                ctx.clone(),
                &mut staking_pool,
                YOCTO.into(),
                (YOCTO / 2).into(),
            );
            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            ctx.epoch_height += 4;
            testing_env!(ctx.clone());
            State::add_liquidity((YOCTO / 16).into());
            assert_eq!(State::liquidity(), (YOCTO / 16).into());
            let starting_balances = staking_pool
                .ops_stake_balance(to_valid_account_id(ACCOUNT))
                .unwrap();
            assert_eq!(
                starting_balances.unstaked.as_ref().unwrap().available,
                (YOCTO / 2).into()
            );
            let balances = staking_pool.ops_stake_withdraw(Some((YOCTO / 8).into()));
            println!("{:#?}", balances);
            assert_eq!(balances.unstaked.unwrap().total, (YOCTO * 3 / 8).into());

            let receipts = deserialize_receipts();
            assert_eq!(receipts.len(), 1);
            let receipt = &receipts[0];
            assert_eq!(receipt.receiver_id, ACCOUNT.to_string());
            assert_eq!(receipt.actions.len(), 1);
            let action = &receipt.actions[0];
            if let Action::Transfer(transfer) = action {
                assert_eq!(transfer.deposit, YOCTO / 8);
            } else {
                panic!("expected transfer action");
            }
            assert_eq!(State::liquidity(), YoctoNear::ZERO);
        }

        #[test]
        #[should_panic(expected = "[ERR] [INSUFFICIENT_FUNDS]")]
        fn partial_with_insufficient_funds() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();

            stake_unstake(
                ctx.clone(),
                &mut staking_pool,
                YOCTO.into(),
                (YOCTO / 8).into(),
            );
            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx.clone());
            staking_pool.ops_stake_withdraw(Some(YOCTO.into()));
        }

        #[test]
        #[should_panic(expected = "[ERR] [INSUFFICIENT_FUNDS]")]
        fn partial_with_zero_unstaked_funds() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();

            stake_unstake(
                ctx.clone(),
                &mut staking_pool,
                YOCTO.into(),
                YoctoNear::ZERO,
            );
            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx.clone());
            staking_pool.ops_stake_withdraw(Some((YOCTO / 8).into()));
        }

        #[test]
        #[should_panic(expected = "[ERR] [INSUFFICIENT_FUNDS]")]
        fn partial_with_zero_staked_and_unstaked_funds() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();

            stake_unstake(
                ctx.clone(),
                &mut staking_pool,
                YoctoNear::ZERO,
                YoctoNear::ZERO,
            );
            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx.clone());
            staking_pool.ops_stake_withdraw(Some((YOCTO / 8).into()));
        }

        #[test]
        #[should_panic(expected = "[ERR] [ACCOUNT_NOT_REGISTERED]")]
        fn partial_with_unregistered_account() {
            let (ctx, mut staking_pool) = deploy_with_unregistered_account();

            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx.clone());
            staking_pool.ops_stake_withdraw(Some((YOCTO / 8).into()));
        }
    }

    #[cfg(test)]
    mod tests_owner_online {
        use super::*;

        #[test]
        fn stake_all_available_balance() {
            let (ctx, mut staking_pool) = deploy_with_unregistered_account();
            bring_pool_online(ctx.clone(), &mut staking_pool);

            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = OWNER.to_string();
            testing_env!(ctx);

            let initial_owner_balance = ContractOwnershipComponent.ops_owner_balance();
            println!("initial {:?}", initial_owner_balance);

            let initial_staking_pool_balances = staking_pool.ops_stake_pool_balances();
            println!(
                "initial_staking_pool_balances {:#?}",
                initial_staking_pool_balances
            );

            let state = staking_pool.ops_stake_state();
            println!(
                "initial state: {}",
                serde_json::to_string_pretty(&state).unwrap()
            );

            // Act
            if let PromiseOrValue::Value(_) = staking_pool.ops_stake_owner_balance(None) {
                panic!("expected Promise");
            }

            let owner_balance = ContractOwnershipComponent.ops_owner_balance();
            println!("after staking {:#?}", owner_balance);
            println!("{:#?}", StakingPoolBalances::load());
            assert_eq!(owner_balance.available, YoctoNear::ZERO);

            // Assert
            let logs = test_utils::get_logs();
            println!("{:#?}", logs);

            let receipts = deserialize_receipts();
            let action = &receipts[0].actions[0];
            if let Action::Stake(stake) = action {
                assert_eq!(stake.stake, *State::staked_balance());
            } else {
                panic!("expected stake action")
            }

            let action = &receipts[1].actions[0];
            if let Action::FunctionCall(function_call) = action {
                assert_eq!(function_call.method_name, "ops_stake_finalize");
                let args: StakeActionCallbackArgs =
                    serde_json::from_str(function_call.args.as_str()).unwrap();
                assert_eq!(args.account_id, ContractOwnershipComponent.ops_owner());
            } else {
                panic!("expected stake action")
            }
        }

        #[test]
        fn stake_partial_available_balance() {
            let (ctx, mut staking_pool) = deploy_with_unregistered_account();
            bring_pool_online(ctx.clone(), &mut staking_pool);

            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = OWNER.to_string();
            testing_env!(ctx);

            let initial_owner_balance = ContractOwnershipComponent.ops_owner_balance();
            println!("initial {:?}", initial_owner_balance);

            let initial_staking_pool_balances = staking_pool.ops_stake_pool_balances();
            println!(
                "initial_staking_pool_balances {:#?}",
                initial_staking_pool_balances
            );

            let state = staking_pool.ops_stake_state();
            println!(
                "initial state: {}",
                serde_json::to_string_pretty(&state).unwrap()
            );

            // Act
            if let PromiseOrValue::Value(_) =
                staking_pool.ops_stake_owner_balance(Some(YOCTO.into()))
            {
                panic!("expected Promise");
            }

            let owner_balance = ContractOwnershipComponent.ops_owner_balance();
            println!("after staking {:#?}", owner_balance);
            println!("{:#?}", StakingPoolBalances::load());
            assert!(
                owner_balance.available
                    < initial_owner_balance.available
                        - YOCTO
                        - account_manager()
                            .storage_balance_of(to_valid_account_id(
                                ContractOwnershipComponent.ops_owner().as_str()
                            ))
                            .unwrap()
                            .total
            );

            // Assert
            let logs = test_utils::get_logs();
            println!("{:#?}", logs);

            let receipts = deserialize_receipts();
            let action = &receipts[0].actions[0];
            if let Action::Stake(stake) = action {
                assert_eq!(stake.stake, *State::staked_balance());
            } else {
                panic!("expected stake action")
            }

            let action = &receipts[1].actions[0];
            if let Action::FunctionCall(function_call) = action {
                assert_eq!(function_call.method_name, "ops_stake_finalize");
                let args: StakeActionCallbackArgs =
                    serde_json::from_str(function_call.args.as_str()).unwrap();
                assert_eq!(args.account_id, ContractOwnershipComponent.ops_owner());
            } else {
                panic!("expected stake action")
            }
        }

        #[test]
        #[should_panic(expected = "[ERR] [INSUFFICIENT_FUNDS]")]
        fn insufficient_funds() {
            let (ctx, mut staking_pool) = deploy_with_unregistered_account();
            bring_pool_online(ctx.clone(), &mut staking_pool);

            let initial_owner_balance = ContractOwnershipComponent.ops_owner_balance();

            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = OWNER.to_string();
            testing_env!(ctx);
            staking_pool.ops_stake_owner_balance(Some(initial_owner_balance.total));
        }

        #[test]
        #[should_panic(expected = "[ERR] [OWNER_ACCESS_REQUIRED]")]
        fn not_owner() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();
            bring_pool_online(ctx.clone(), &mut staking_pool);

            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx);
            staking_pool.ops_stake_owner_balance(None);
        }
    }

    #[cfg(test)]
    mod tests_owner_offline {
        use super::*;

        #[test]
        fn stake_all_available_balance() {
            let (ctx, mut staking_pool) = deploy_with_unregistered_account();

            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = OWNER.to_string();
            testing_env!(ctx);

            let initial_owner_balance = ContractOwnershipComponent.ops_owner_balance();
            println!("initial {:?}", initial_owner_balance);

            let initial_staking_pool_balances = staking_pool.ops_stake_pool_balances();
            println!(
                "initial_staking_pool_balances {:#?}",
                initial_staking_pool_balances
            );

            let state = staking_pool.ops_stake_state();
            println!(
                "initial state: {}",
                serde_json::to_string_pretty(&state).unwrap()
            );

            // Act
            if let PromiseOrValue::Value(balances) = staking_pool.ops_stake_owner_balance(None) {
                let owner_balance = ContractOwnershipComponent.ops_owner_balance();
                println!("after staking {:#?}", owner_balance);
                println!("{:#?}", StakingPoolBalances::load());
                assert_eq!(owner_balance.available, YoctoNear::ZERO);
                assert_eq!(
                    balances.staked.unwrap().near_value,
                    9996920990000000000000000000.into()
                );

                // Assert
                let logs = test_utils::get_logs();
                println!("{:#?}", logs);

                assert!(deserialize_receipts().is_empty());
            } else {
                panic!("expected value");
            }
        }

        #[test]
        fn stake_partial_available_balance() {
            let (ctx, mut staking_pool) = deploy_with_unregistered_account();

            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = OWNER.to_string();
            testing_env!(ctx);

            let initial_owner_balance = ContractOwnershipComponent.ops_owner_balance();
            println!("initial {:?}", initial_owner_balance);

            let initial_staking_pool_balances = staking_pool.ops_stake_pool_balances();
            println!(
                "initial_staking_pool_balances {:#?}",
                initial_staking_pool_balances
            );

            let state = staking_pool.ops_stake_state();
            println!(
                "initial state: {}",
                serde_json::to_string_pretty(&state).unwrap()
            );

            // Act
            if let PromiseOrValue::Value(balances) =
                staking_pool.ops_stake_owner_balance(Some(YOCTO.into()))
            {
                let logs = test_utils::get_logs();
                println!("{:#?}", logs);

                println!("{:#?}", balances);

                let owner_balance = ContractOwnershipComponent.ops_owner_balance();
                println!("after staking {:#?}", owner_balance);
                println!("{:#?}", StakingPoolBalances::load());
                assert!(
                    owner_balance.available
                        < initial_owner_balance.available
                            - YOCTO
                            - account_manager()
                                .storage_balance_of(to_valid_account_id(
                                    ContractOwnershipComponent.ops_owner().as_str()
                                ))
                                .unwrap()
                                .total
                );
                assert!(deserialize_receipts().is_empty());
            } else {
                panic!("expected Promise");
            }
        }

        #[test]
        #[should_panic(expected = "[ERR] [INSUFFICIENT_FUNDS]")]
        fn insufficient_funds() {
            let (ctx, mut staking_pool) = deploy_with_unregistered_account();

            let initial_owner_balance = ContractOwnershipComponent.ops_owner_balance();

            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = OWNER.to_string();
            testing_env!(ctx);
            staking_pool.ops_stake_owner_balance(Some(initial_owner_balance.total));
        }

        #[test]
        #[should_panic(expected = "[ERR] [OWNER_ACCESS_REQUIRED]")]
        fn not_owner() {
            let (ctx, mut staking_pool) = deploy_with_registered_account();

            let mut ctx = ctx.clone();
            ctx.predecessor_account_id = ACCOUNT.to_string();
            testing_env!(ctx);
            staking_pool.ops_stake_owner_balance(None);
        }
    }

    #[cfg(test)]
    mod tests_treasury_online {
        use super::*;

        #[cfg(test)]
        mod tests_ops_stake_treasury_deposit {
            use super::*;

            #[test]
            fn with_attached_deposit() {
                let (ctx, mut staking_pool) = deploy_with_unregistered_account();
                bring_pool_online(ctx.clone(), &mut staking_pool);

                let state = staking_pool.ops_stake_state();
                assert_eq!(state.treasury_balance, YoctoNear::ZERO);

                // Act
                {
                    let mut ctx = ctx.clone();
                    ctx.attached_deposit = YOCTO;
                    testing_env!(ctx);
                    if let PromiseOrValue::Value(_) = staking_pool.ops_stake_treasury_deposit() {
                        panic!("expected Promise")
                    }
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                    let staking_pool_balances = staking_pool.ops_stake_pool_balances();
                    println!("{:#?}", staking_pool_balances);
                    assert_eq!(staking_pool_balances.staked, YOCTO.into());
                    let state = staking_pool.ops_stake_state();
                    println!("{}", serde_json::to_string_pretty(&state).unwrap());
                    assert_eq!(state.treasury_balance, YoctoNear::ZERO);

                    let treasury_stake_balance =
                        ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
                    assert_eq!(treasury_stake_balance, TokenAmount::ZERO);

                    deserialize_receipts();
                }
                // finalize the staked treasury deposit
                {
                    let staking_pool_balances = staking_pool.ops_stake_pool_balances();

                    let mut ctx = ctx.clone();
                    ctx.attached_deposit = 0;
                    ctx.account_locked_balance = *staking_pool_balances.staked;
                    testing_env_with_promise_result_success(ctx);
                    let balances = staking_pool.ops_stake_finalize(
                        env::current_account_id(),
                        staking_pool_balances.staked,
                        (*staking_pool_balances.staked).into(),
                    );
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);

                    println!("{:#?}", balances);
                    assert_eq!(balances.staked.unwrap().near_value, YOCTO.into());

                    let treasury_stake_balance =
                        ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
                    assert_eq!(treasury_stake_balance, YOCTO.into());

                    assert_eq!(
                        staking_pool.ops_stake_state().treasury_balance,
                        YOCTO.into()
                    );
                }
            }

            #[test]
            #[should_panic(expected = "[ERR] [NEAR_DEPOSIT_REQUIRED]")]
            fn with_zero_attached_deposit() {
                let (ctx, mut staking_pool) = deploy_with_unregistered_account();
                bring_pool_online(ctx.clone(), &mut staking_pool);

                let mut ctx = ctx.clone();
                ctx.attached_deposit = 0;
                testing_env!(ctx);
                staking_pool.ops_stake_treasury_deposit();
            }
        }

        #[cfg(test)]
        mod ops_stake_treasury_transfer_to_owner {
            use super::*;

            #[test]
            fn transfer_all_as_admin() {
                let (ctx, mut staking_pool) = deploy_with_unregistered_account();
                bring_pool_online(ctx.clone(), &mut staking_pool);

                let state = staking_pool.ops_stake_state();
                assert_eq!(state.treasury_balance, YoctoNear::ZERO);

                {
                    let mut ctx = ctx.clone();
                    ctx.attached_deposit = YOCTO;
                    testing_env!(ctx);
                    if let PromiseOrValue::Value(_) = staking_pool.ops_stake_treasury_deposit() {
                        panic!("expected Promise")
                    }
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                    let staking_pool_balances = staking_pool.ops_stake_pool_balances();
                    println!("{:#?}", staking_pool_balances);
                    assert_eq!(staking_pool_balances.staked, YOCTO.into());
                    let state = staking_pool.ops_stake_state();
                    println!("{}", serde_json::to_string_pretty(&state).unwrap());
                    assert_eq!(state.treasury_balance, YoctoNear::ZERO);

                    let treasury_stake_balance =
                        ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
                    assert_eq!(treasury_stake_balance, TokenAmount::ZERO);

                    deserialize_receipts();
                }
                {
                    let staking_pool_balances = staking_pool.ops_stake_pool_balances();

                    let mut ctx = ctx.clone();
                    ctx.attached_deposit = 0;
                    ctx.account_locked_balance = *staking_pool_balances.staked;
                    testing_env_with_promise_result_success(ctx);
                    let balances = staking_pool.ops_stake_finalize(
                        env::current_account_id(),
                        staking_pool_balances.staked,
                        (*staking_pool_balances.staked).into(),
                    );
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);

                    println!("{:#?}", balances);
                    assert_eq!(balances.staked.unwrap().near_value, YOCTO.into());

                    let treasury_stake_balance =
                        ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
                    assert_eq!(treasury_stake_balance, YOCTO.into());

                    assert_eq!(
                        staking_pool.ops_stake_state().treasury_balance,
                        YOCTO.into()
                    );
                }

                // Act
                {
                    let mut ctx = ctx.clone();
                    ctx.predecessor_account_id = ADMIN.to_string();
                    testing_env!(ctx.clone());
                    staking_pool.ops_stake_treasury_transfer_to_owner(None);

                    let treasury_stake_balance =
                        ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
                    assert_eq!(treasury_stake_balance, TokenAmount::ZERO);

                    let owner_stake_balance = ft_stake().ft_balance_of(to_valid_account_id(
                        &ContractOwnershipComponent.ops_owner(),
                    ));
                    assert_eq!(owner_stake_balance, YOCTO.into());

                    let state = staking_pool.ops_stake_state();
                    assert_eq!(state.treasury_balance, YoctoNear::ZERO);
                }
            }

            #[test]
            fn transfer_all_as_treasurer() {
                let (ctx, mut staking_pool) = deploy_with_registered_account();
                bring_pool_online(ctx.clone(), &mut staking_pool);

                let state = staking_pool.ops_stake_state();
                assert_eq!(state.treasury_balance, YoctoNear::ZERO);

                {
                    let mut ctx = ctx.clone();
                    ctx.attached_deposit = YOCTO;
                    testing_env!(ctx);
                    if let PromiseOrValue::Value(_) = staking_pool.ops_stake_treasury_deposit() {
                        panic!("expected Promise")
                    }
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                    let staking_pool_balances = staking_pool.ops_stake_pool_balances();
                    println!("{:#?}", staking_pool_balances);
                    assert_eq!(staking_pool_balances.staked, YOCTO.into());
                    let state = staking_pool.ops_stake_state();
                    println!("{}", serde_json::to_string_pretty(&state).unwrap());
                    assert_eq!(state.treasury_balance, YoctoNear::ZERO);

                    let treasury_stake_balance =
                        ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
                    assert_eq!(treasury_stake_balance, TokenAmount::ZERO);

                    deserialize_receipts();
                }
                {
                    let staking_pool_balances = staking_pool.ops_stake_pool_balances();

                    let mut ctx = ctx.clone();
                    ctx.attached_deposit = 0;
                    ctx.account_locked_balance = *staking_pool_balances.staked;
                    testing_env_with_promise_result_success(ctx);
                    let balances = staking_pool.ops_stake_finalize(
                        env::current_account_id(),
                        staking_pool_balances.staked,
                        (*staking_pool_balances.staked).into(),
                    );
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);

                    println!("{:#?}", balances);
                    assert_eq!(balances.staked.unwrap().near_value, YOCTO.into());

                    let treasury_stake_balance =
                        ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
                    assert_eq!(treasury_stake_balance, YOCTO.into());

                    assert_eq!(
                        staking_pool.ops_stake_state().treasury_balance,
                        YOCTO.into()
                    );
                }

                // Arrange - grant account treasurer
                {
                    let mut ctx = ctx.clone();
                    ctx.predecessor_account_id = ADMIN.to_string();
                    testing_env!(ctx.clone());
                    account_manager().ops_permissions_grant(
                        to_valid_account_id(ACCOUNT),
                        (1 << TREASURER_PERMISSION_BIT).into(),
                    );
                }

                // Act
                {
                    let mut ctx = ctx.clone();
                    ctx.predecessor_account_id = ACCOUNT.to_string();
                    testing_env!(ctx.clone());
                    staking_pool.ops_stake_treasury_transfer_to_owner(None);

                    let treasury_stake_balance =
                        ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
                    assert_eq!(treasury_stake_balance, TokenAmount::ZERO);

                    let owner_stake_balance = ft_stake().ft_balance_of(to_valid_account_id(
                        &ContractOwnershipComponent.ops_owner(),
                    ));
                    assert_eq!(owner_stake_balance, YOCTO.into());

                    let state = staking_pool.ops_stake_state();
                    assert_eq!(state.treasury_balance, YoctoNear::ZERO);
                }
            }

            #[test]
            fn transfer_partial_as_admin() {
                let (ctx, mut staking_pool) = deploy_with_registered_account();
                bring_pool_online(ctx.clone(), &mut staking_pool);

                let state = staking_pool.ops_stake_state();
                assert_eq!(state.treasury_balance, YoctoNear::ZERO);

                const TREASURY_DEPOSIT: YoctoNear = YoctoNear(3 * YOCTO);
                {
                    let mut ctx = ctx.clone();
                    ctx.attached_deposit = *TREASURY_DEPOSIT;
                    testing_env!(ctx);
                    if let PromiseOrValue::Value(_) = staking_pool.ops_stake_treasury_deposit() {
                        panic!("expected Promise")
                    }
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                    let staking_pool_balances = staking_pool.ops_stake_pool_balances();
                    println!("{:#?}", staking_pool_balances);
                    assert_eq!(staking_pool_balances.staked, TREASURY_DEPOSIT);
                    let state = staking_pool.ops_stake_state();
                    println!("{}", serde_json::to_string_pretty(&state).unwrap());
                    assert_eq!(state.treasury_balance, YoctoNear::ZERO);

                    let treasury_stake_balance =
                        ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
                    assert_eq!(treasury_stake_balance, TokenAmount::ZERO);

                    deserialize_receipts();
                }
                {
                    let staking_pool_balances = staking_pool.ops_stake_pool_balances();

                    let mut ctx = ctx.clone();
                    ctx.attached_deposit = 0;
                    ctx.account_locked_balance = *staking_pool_balances.staked;
                    testing_env_with_promise_result_success(ctx);
                    let balances = staking_pool.ops_stake_finalize(
                        env::current_account_id(),
                        staking_pool_balances.staked,
                        (*staking_pool_balances.staked).into(),
                    );
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);

                    println!("{:#?}", balances);
                    assert_eq!(balances.staked.unwrap().near_value, TREASURY_DEPOSIT);

                    let treasury_stake_balance =
                        ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
                    assert_eq!(treasury_stake_balance, (*TREASURY_DEPOSIT).into());

                    assert_eq!(
                        staking_pool.ops_stake_state().treasury_balance,
                        TREASURY_DEPOSIT
                    );
                }

                // Act
                {
                    let mut ctx = ctx.clone();
                    ctx.predecessor_account_id = ADMIN.to_string();
                    testing_env!(ctx.clone());
                    staking_pool.ops_stake_treasury_transfer_to_owner(Some(YOCTO.into()));

                    let treasury_stake_balance =
                        ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
                    assert_eq!(treasury_stake_balance, (2 * YOCTO).into());

                    let owner_stake_balance = ft_stake().ft_balance_of(to_valid_account_id(
                        &ContractOwnershipComponent.ops_owner(),
                    ));
                    assert_eq!(owner_stake_balance, YOCTO.into());

                    let state = staking_pool.ops_stake_state();
                    assert_eq!(state.treasury_balance, (2 * YOCTO).into());
                }
            }

            #[test]
            #[should_panic(expected = "[ERR] [NOT_AUTHORIZED]")]
            fn as_unauthorized_account() {
                let (ctx, mut staking_pool) = deploy_with_registered_account();
                bring_pool_online(ctx.clone(), &mut staking_pool);

                let mut ctx = ctx.clone();
                ctx.predecessor_account_id = ACCOUNT.to_string();
                testing_env!(ctx.clone());
                staking_pool.ops_stake_treasury_transfer_to_owner(None);
            }
        }
    }

    #[cfg(test)]
    mod tests_treasury_offline {
        use super::*;

        #[cfg(test)]
        mod tests_ops_stake_treasury_deposit {
            use super::*;

            #[test]
            fn with_attached_deposit() {
                let (ctx, mut staking_pool) = deploy_with_unregistered_account();

                let state = staking_pool.ops_stake_state();
                assert_eq!(state.treasury_balance, YoctoNear::ZERO);

                // Act
                {
                    let mut ctx = ctx.clone();
                    ctx.attached_deposit = YOCTO;
                    testing_env!(ctx.clone());
                    if let PromiseOrValue::Value(balances) =
                        staking_pool.ops_stake_treasury_deposit()
                    {
                        println!("{:#?}", balances);
                        assert_eq!(balances.staked.as_ref().unwrap().near_value, YOCTO.into());

                        assert_eq!(
                            balances,
                            staking_pool
                                .ops_stake_balance(to_valid_account_id(
                                    env::current_account_id().as_str()
                                ))
                                .unwrap()
                        );
                    } else {
                        panic!("expected Promise")
                    }
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);

                    let treasury_stake_balance =
                        ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
                    assert_eq!(treasury_stake_balance, YOCTO.into());

                    assert_eq!(
                        staking_pool.ops_stake_state().treasury_balance,
                        YOCTO.into()
                    );

                    assert!(deserialize_receipts().is_empty());
                }
            }

            #[test]
            #[should_panic(expected = "[ERR] [NEAR_DEPOSIT_REQUIRED]")]
            fn with_zero_attached_deposit() {
                let (ctx, mut staking_pool) = deploy_with_unregistered_account();

                let mut ctx = ctx.clone();
                ctx.attached_deposit = 0;
                testing_env!(ctx);
                staking_pool.ops_stake_treasury_deposit();
            }
        }

        #[cfg(test)]
        mod ops_stake_treasury_transfer_to_owner {
            use super::*;

            #[test]
            fn transfer_all_as_admin() {
                let (ctx, mut staking_pool) = deploy_with_unregistered_account();

                {
                    let mut ctx = ctx.clone();
                    ctx.attached_deposit = YOCTO;
                    testing_env!(ctx);
                    if let PromiseOrValue::Promise(_) = staking_pool.ops_stake_treasury_deposit() {
                        panic!("expected Value")
                    }
                }

                // Act
                {
                    let mut ctx = ctx.clone();
                    ctx.predecessor_account_id = ADMIN.to_string();
                    testing_env!(ctx.clone());
                    staking_pool.ops_stake_treasury_transfer_to_owner(None);

                    let treasury_stake_balance =
                        ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
                    assert_eq!(treasury_stake_balance, TokenAmount::ZERO);

                    let owner_stake_balance = ft_stake().ft_balance_of(to_valid_account_id(
                        &ContractOwnershipComponent.ops_owner(),
                    ));
                    assert_eq!(owner_stake_balance, YOCTO.into());

                    let state = staking_pool.ops_stake_state();
                    assert_eq!(state.treasury_balance, YoctoNear::ZERO);
                }
            }

            #[test]
            fn transfer_all_as_treasurer() {
                let (ctx, mut staking_pool) = deploy_with_registered_account();

                {
                    let mut ctx = ctx.clone();
                    ctx.attached_deposit = YOCTO;
                    testing_env!(ctx);
                    if let PromiseOrValue::Promise(_) = staking_pool.ops_stake_treasury_deposit() {
                        panic!("expected Value")
                    }
                }

                // Arrange - grant account treasurer
                {
                    let mut ctx = ctx.clone();
                    ctx.predecessor_account_id = ADMIN.to_string();
                    testing_env!(ctx.clone());
                    account_manager().ops_permissions_grant(
                        to_valid_account_id(ACCOUNT),
                        (1 << TREASURER_PERMISSION_BIT).into(),
                    );
                }

                // Act
                {
                    let mut ctx = ctx.clone();
                    ctx.predecessor_account_id = ACCOUNT.to_string();
                    testing_env!(ctx.clone());
                    staking_pool.ops_stake_treasury_transfer_to_owner(None);

                    let treasury_stake_balance =
                        ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
                    assert_eq!(treasury_stake_balance, TokenAmount::ZERO);

                    let owner_stake_balance = ft_stake().ft_balance_of(to_valid_account_id(
                        &ContractOwnershipComponent.ops_owner(),
                    ));
                    assert_eq!(owner_stake_balance, YOCTO.into());

                    let state = staking_pool.ops_stake_state();
                    assert_eq!(state.treasury_balance, YoctoNear::ZERO);
                }
            }

            #[test]
            fn transfer_partial_as_admin() {
                let (ctx, mut staking_pool) = deploy_with_registered_account();

                const TREASURY_DEPOSIT: YoctoNear = YoctoNear(3 * YOCTO);
                {
                    let mut ctx = ctx.clone();
                    ctx.attached_deposit = *TREASURY_DEPOSIT;
                    testing_env!(ctx);
                    if let PromiseOrValue::Promise(_) = staking_pool.ops_stake_treasury_deposit() {
                        panic!("expected Value")
                    }
                }

                // Act
                {
                    let mut ctx = ctx.clone();
                    ctx.predecessor_account_id = ADMIN.to_string();
                    testing_env!(ctx.clone());
                    staking_pool.ops_stake_treasury_transfer_to_owner(Some(YOCTO.into()));

                    let treasury_stake_balance =
                        ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
                    assert_eq!(treasury_stake_balance, (2 * YOCTO).into());

                    let owner_stake_balance = ft_stake().ft_balance_of(to_valid_account_id(
                        &ContractOwnershipComponent.ops_owner(),
                    ));
                    assert_eq!(owner_stake_balance, YOCTO.into());

                    let state = staking_pool.ops_stake_state();
                    assert_eq!(state.treasury_balance, (2 * YOCTO).into());
                }
            }

            #[test]
            #[should_panic(expected = "[ERR] [NOT_AUTHORIZED]")]
            fn as_unauthorized_account() {
                let (ctx, mut staking_pool) = deploy_with_registered_account();

                let mut ctx = ctx.clone();
                ctx.predecessor_account_id = ACCOUNT.to_string();
                testing_env!(ctx.clone());
                staking_pool.ops_stake_treasury_transfer_to_owner(None);
            }
        }
    }

    #[cfg(test)]
    mod tests_operator {
        use super::*;

        #[cfg(test)]
        mod start_and_stop_staking_commands {
            use super::*;

            #[test]
            fn starting_stopping_staking_with_nothing_staked() {
                let (ctx, mut staking_pool) = deploy_with_registered_account();

                assert_eq!(
                    staking_pool.ops_stake_status(),
                    Status::Offline(OfflineReason::Stopped)
                );

                // Act - start staking
                {
                    let mut ctx = ctx.clone();
                    ctx.predecessor_account_id = ADMIN.to_string();
                    testing_env!(ctx);
                    staking_pool
                        .ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                    assert_eq!(logs, vec!["[INFO] [STATUS_ONLINE] ",]);
                    // Assert
                    assert_eq!(staking_pool.ops_stake_status(), Status::Online);
                    assert_eq!(
                        staking_pool.ops_stake_pool_balances().total_unstaked,
                        YoctoNear::ZERO
                    );
                    // since there is nothing staked, then we expect no stake actions
                    assert!(deserialize_receipts().is_empty());
                }
                // Act - start staking while already staking
                {
                    let mut ctx = ctx.clone();
                    ctx.predecessor_account_id = ADMIN.to_string();
                    testing_env!(ctx);
                    staking_pool
                        .ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
                    assert!(test_utils::get_logs().is_empty());
                    // Assert
                    assert_eq!(staking_pool.ops_stake_status(), Status::Online);
                    assert!(deserialize_receipts().is_empty());
                }
                // Act - stop staking
                {
                    let mut ctx = ctx.clone();
                    ctx.predecessor_account_id = ADMIN.to_string();
                    testing_env!(ctx);
                    staking_pool
                        .ops_stake_operator_command(StakingPoolOperatorCommand::StopStaking);
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                    assert_eq!(logs, vec!["[WARN] [STATUS_OFFLINE] Stopped",]);
                    // Assert
                    assert_eq!(
                        staking_pool.ops_stake_status(),
                        Status::Offline(OfflineReason::Stopped)
                    );
                    assert_eq!(
                        staking_pool.ops_stake_pool_balances().total_unstaked,
                        YoctoNear::ZERO
                    );
                    // since there is nothing staked, then we expect no stake actions
                    assert!(deserialize_receipts().is_empty());
                }
                // Act - stop staking - when already stopped
                {
                    let mut ctx = ctx.clone();
                    ctx.predecessor_account_id = ADMIN.to_string();
                    testing_env!(ctx);
                    staking_pool
                        .ops_stake_operator_command(StakingPoolOperatorCommand::StopStaking);
                    assert!(test_utils::get_logs().is_empty());
                    // Assert
                    assert_eq!(
                        staking_pool.ops_stake_status(),
                        Status::Offline(OfflineReason::Stopped)
                    );
                    assert_eq!(
                        staking_pool.ops_stake_pool_balances().total_unstaked,
                        YoctoNear::ZERO
                    );
                    // since there is nothing staked, then we expect no stake actions
                    assert!(deserialize_receipts().is_empty());
                }
                // Act - start staking after being stopped by operator
                {
                    let mut ctx = ctx.clone();
                    ctx.predecessor_account_id = ADMIN.to_string();
                    testing_env!(ctx);
                    staking_pool
                        .ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                    assert_eq!(logs, vec!["[INFO] [STATUS_ONLINE] ",]);
                    // Assert
                    assert_eq!(staking_pool.ops_stake_status(), Status::Online);
                    assert_eq!(
                        staking_pool.ops_stake_pool_balances().total_unstaked,
                        YoctoNear::ZERO
                    );
                    // since there is nothing staked, then we expect no stake actions
                    assert!(deserialize_receipts().is_empty());
                }
            }

            #[test]
            #[should_panic(expected = "[ERR] [NOT_AUTHORIZED]")]
            fn not_authorized() {
                let (_ctx, mut staking_pool) = deploy_with_registered_account();
                staking_pool.ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
            }

            /// - deploy the staking pool
            /// - stake 1 NEAR while pool is offline
            /// - start staking pool
            /// - stake 1 NEAR while pool is online
            ///   - total staked balance should be 2 NEAR
            ///   - treasury balance should contain staking fees for total staked balance
            #[test]
            fn stake_while_stopped_then_start() {
                let (ctx, mut staking_pool) = deploy_with_registered_account();

                {
                    let mut ctx = ctx.clone();
                    ctx.predecessor_account_id = ADMIN.to_string();
                    ctx.attached_deposit = YOCTO;
                    testing_env!(ctx);
                    let state = staking_pool.ops_stake_state();
                    if let PromiseOrValue::Value(balances) = staking_pool.ops_stake() {
                        assert_eq!(
                            balances.staked.as_ref().unwrap().stake,
                            (YOCTO - *(state.staking_fee * YOCTO.into())).into()
                        );
                    } else {
                        panic!("expected Value")
                    }
                }
                {
                    let mut ctx = ctx.clone();
                    ctx.predecessor_account_id = ADMIN.to_string();
                    ctx.attached_deposit = 0;
                    testing_env!(ctx);
                    staking_pool
                        .ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                    assert_eq!(logs, vec!["[INFO] [STATUS_ONLINE] ",]);

                    let staking_pool_balances = staking_pool.ops_stake_pool_balances();
                    assert_eq!(staking_pool_balances.staked, YoctoNear::ZERO);
                    let receipts = deserialize_receipts();
                    assert_eq!(receipts.len(), 2);
                    {
                        let receipt = &receipts[0];
                        assert_eq!(receipt.receiver_id, env::current_account_id());
                        assert_eq!(receipt.actions.len(), 1);
                        let action = &receipt.actions[0];
                        if let Action::Stake(action) = action {
                            assert_eq!(action.stake, *staking_pool_balances.total_staked);
                        } else {
                            panic!("expected stake action");
                        }
                    }
                    {
                        let receipt = &receipts[1];
                        assert_eq!(receipt.receiver_id, env::current_account_id());
                        assert_eq!(receipt.actions.len(), 1);
                        let action = &receipt.actions[0];
                        if let Action::FunctionCall(action) = action {
                            assert_eq!(action.method_name, "ops_stake_start_finalize");
                        } else {
                            panic!("expected function call action");
                        }
                    }
                }
                {
                    let mut ctx = ctx.clone();
                    ctx.predecessor_account_id = ADMIN.to_string();
                    ctx.attached_deposit = YOCTO;
                    ctx.account_locked_balance = YOCTO;
                    testing_env!(ctx);
                    if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
                        panic!("expected Promise")
                    }
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                    assert_eq!(
                        logs,
                        vec![
                            "[INFO] [STAKE] near_amount=1000000000000000000000000, stake_token_amount=1000000000000000000000000",
                        ]
                    );
                    let staking_pool_balances = staking_pool.ops_stake_pool_balances();
                    assert_eq!(staking_pool_balances.total_staked, YOCTO.into());
                    assert_eq!(staking_pool_balances.staked, YOCTO.into());
                }
            }

            /// - deploy staking pool
            /// - start staking pool
            /// - stake 1 NEAR
            /// - stop staking pool
            /// - run `ops_stake_finalize()` callback
            /// - stake 1 NEAR
            ///   - total staked balance should be 2 NEAR
            ///   - treasury balance should contain staking fees for total staked balance
            #[test]
            fn stop_staking_pool_while_stake_action_in_flight() {
                let (ctx, mut staking_pool) = deploy_with_registered_account();
                bring_pool_online(ctx.clone(), &mut staking_pool);

                {
                    let mut ctx = ctx.clone();
                    ctx.predecessor_account_id = ADMIN.to_string();
                    ctx.attached_deposit = YOCTO;
                    testing_env!(ctx);
                    if let PromiseOrValue::Value(_balances) = staking_pool.ops_stake() {
                        panic!("expected Promise")
                    }
                }
                {
                    let mut ctx = ctx.clone();
                    ctx.predecessor_account_id = ADMIN.to_string();
                    ctx.attached_deposit = 0;
                    ctx.account_locked_balance = YOCTO;
                    testing_env!(ctx);
                    staking_pool
                        .ops_stake_operator_command(StakingPoolOperatorCommand::StopStaking);
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                    assert_eq!(logs, vec!["[WARN] [STATUS_OFFLINE] Stopped",]);

                    let staking_pool_balances = staking_pool.ops_stake_pool_balances();
                    assert_eq!(staking_pool_balances.staked, YoctoNear::ZERO);
                    let receipts = deserialize_receipts();
                    assert_eq!(receipts.len(), 2);
                    {
                        let receipt = &receipts[0];
                        assert_eq!(receipt.receiver_id, env::current_account_id());
                        assert_eq!(receipt.actions.len(), 1);
                        let action = &receipt.actions[0];
                        if let Action::Stake(action) = action {
                            assert_eq!(action.stake, 0);
                        } else {
                            panic!("expected stake action");
                        }
                    }
                    {
                        let receipt = &receipts[1];
                        assert_eq!(receipt.receiver_id, env::current_account_id());
                        assert_eq!(receipt.actions.len(), 1);
                        let action = &receipt.actions[0];
                        if let Action::FunctionCall(action) = action {
                            assert_eq!(action.method_name, "ops_stake_stop_finalize");
                        } else {
                            panic!("expected function call action");
                        }
                    }
                }
                {
                    let mut ctx = ctx.clone();
                    ctx.predecessor_account_id = ADMIN.to_string();
                    ctx.attached_deposit = 0;
                    ctx.account_locked_balance = YOCTO;
                    testing_env!(ctx);
                    let balances = staking_pool.ops_stake_finalize(
                        ADMIN.to_string(),
                        YOCTO.into(),
                        YOCTO.into(),
                    );
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                    assert_eq!(
                        logs,
                        vec![
                            "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
                            "[INFO] [FT_MINT] account: admin, amount: 1000000000000000000000000",
                            "[INFO] [FT_BURN] account: admin, amount: 8000000000000000000000",
                            "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
                            "[INFO] [FT_MINT] account: contract.near, amount: 8000000000000000000000",
                        ]
                    );
                    let state = staking_pool.ops_stake_state();
                    println!("{}", serde_json::to_string_pretty(&state).unwrap());

                    println!("{:#?}", balances);
                    let amount = YOCTO - *(state.staking_fee * YOCTO.into());
                    assert_eq!(balances.staked.as_ref().unwrap().near_value, amount.into());
                    assert_eq!(balances.staked.as_ref().unwrap().stake, amount.into())
                }
            }

            /// - deploy staking pool
            /// - start staking pool
            /// - stake 10 NEAR
            /// - finalize stake
            /// - unstake 1 NEAR
            /// - stop staking pool
            /// - run `ops_unstake_finalize()` callback
            /// - stake 1 NEAR
            ///   - total staked balance should be 2 NEAR
            ///   - treasury balance should contain staking fees for total staked balance
            #[test]
            fn stop_staking_pool_while_unstake_action_in_flight() {
                todo!()
            }
        }
    }
}

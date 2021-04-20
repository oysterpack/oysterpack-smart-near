use crate::{
    OfflineReason, StakeAccountBalances, StakeAccountData, StakeActionCallbacks, StakedBalance,
    StakingPool, StakingPoolBalances, StakingPoolOperator, StakingPoolOperatorCommand, Status,
    Treasury, ERR_STAKED_BALANCE_TOO_LOW_TO_UNSTAKE, ERR_STAKE_ACTION_FAILED, LOG_EVENT_EARNINGS,
    LOG_EVENT_LIQUIDITY, LOG_EVENT_NOT_ENOUGH_TO_STAKE, LOG_EVENT_STAKE, LOG_EVENT_STATUS_OFFLINE,
    LOG_EVENT_STATUS_ONLINE, LOG_EVENT_TREASURY_DEPOSIT, LOG_EVENT_TREASURY_DIVIDEND,
    LOG_EVENT_UNSTAKE, PERMISSION_TREASURER,
};
use oysterpack_smart_account_management::{
    components::account_management::AccountManagementComponent, AccountDataObject, AccountMetrics,
    AccountRepository, Permission, StorageManagement, ERR_ACCOUNT_NOT_REGISTERED,
    ERR_NOT_AUTHORIZED,
};
use oysterpack_smart_contract::{
    components::contract_ownership::ContractOwnershipComponent, BalanceId, ContractNearBalances,
    ContractOwnership,
};
use oysterpack_smart_fungible_token::{
    components::fungible_token::FungibleTokenComponent, FungibleToken, Memo, TokenAmount,
    TokenService, TransferCallMessage, TransferReceiver,
};
use oysterpack_smart_near::{
    asserts::{ERR_ILLEGAL_STATE, ERR_INSUFFICIENT_FUNDS, ERR_INVALID, ERR_NEAR_DEPOSIT_REQUIRED},
    component::{Component, ComponentState, Deploy},
    data::numbers::U256,
    domain::{
        ActionType, BasisPoints, ByteLen, Gas, PublicKey, SenderIsReceiver, TransactionResource,
        YoctoNear,
    },
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

pub type AccountManager = AccountManagementComponent<StakeAccountData>;
pub type StakeFungibleToken = FungibleTokenComponent<StakeAccountData>;

/// Staking Pool Component
///
/// ## Deployment
/// - permissions: [`crate::PERMISSION_TREASURER`];
pub struct StakingPoolComponent {
    account_manager: AccountManager,
    stake_token: StakeFungibleToken,
}

impl StakingPoolComponent {
    pub fn new(
        account_manager: AccountManagementComponent<StakeAccountData>,
        stake: StakeFungibleToken,
    ) -> Self {
        Self {
            account_manager,
            stake_token: stake,
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
    pub staking_fee: BasisPoints,

    pub status: Status,

    /// used to check if staking rewards were earned
    pub last_contract_managed_total_balance: YoctoNear,

    /// NEAR deposited into the treasury is staked and stake earnings are used to boost the
    /// staking pool yield
    /// - this is used to track the treasury STAKE NEAR value
    /// - if it changes before staking fees are deposited, then it means the treasury received
    ///   staking rewards. The staking rewards will be burned before depositing the staking fee
    pub treasury_balance: YoctoNear,
}

impl State {
    pub const TOTAL_STAKED_BALANCE: BalanceId = BalanceId(1956973021105502521442959170292258855);

    /// unstaked funds are locked for 4 epochs
    /// - we need to track unstaked funds that is owned by accounts separate from the account NEAR balances
    /// - accounts will need to withdraw unstaked balances against this balance in combination with
    /// [`Self::UNSTAKED_LIQUIDITY_POOL`]
    pub const TOTAL_UNSTAKED_BALANCE: BalanceId = BalanceId(1955705469859818043123742456310621056);
    /// provides liquidity for withdrawing unstaked funds that are still locked
    /// - liquidity is added automatically when funds are staked and there are unstaked funds pending
    ///   withdrawal
    /// - when liquidity is added, funds are debited from [`Self::TOTAL_UNSTAKED_BALANCE`] and credited
    ///   to this balance
    pub const UNSTAKED_LIQUIDITY_POOL: BalanceId = BalanceId(1955784487678443851622222785149485288);

    /// returns the total balance that is currently managed by the contract and that accounts have
    /// no access to
    /// - this is used to compute staking rewards that are earned - since this balance is completely
    ///  managed by the contract, then if the balance increases, then we know rewards have been earned
    /// - account storage balances and unstaked balances are funds that accounts can deposit
    ///   and withdraw from at any time, and thus are excluded from the equation
    fn contract_managed_total_balance() -> YoctoNear {
        let total_contract_balance: YoctoNear =
            (env::account_balance() + env::account_locked_balance() - env::attached_deposit())
                .into();
        total_contract_balance
            - AccountMetrics::load().total_near_balance
            - State::liquidity()
            - State::total_unstaked_balance()
    }

    /// excludes
    /// attached deposit because this should only be called in view mode on the contract
    /// - `env::attached_deposit` is illegal to call in view mode
    pub(crate) fn contract_managed_total_balance_in_view_mode() -> YoctoNear {
        let total_contract_balance: YoctoNear =
            (env::account_balance() + env::account_locked_balance()).into();
        total_contract_balance
            - AccountMetrics::load().total_near_balance
            - State::liquidity()
            - State::total_unstaked_balance()
    }

    /// returns any earnings that have been received since the last time we checked - but excludes
    /// attached deposit because this should only be called in view mode on the contract
    /// - `env::attached_deposit` is illegal to call in view mode
    fn check_for_earnings_in_view_mode(&self) -> YoctoNear {
        State::contract_managed_total_balance_in_view_mode()
            .saturating_sub(*self.last_contract_managed_total_balance)
            .into()
    }

    pub(crate) fn total_staked_balance() -> YoctoNear {
        ContractNearBalances::near_balance(Self::TOTAL_STAKED_BALANCE)
    }

    fn incr_total_staked_balance(amount: YoctoNear) {
        ContractNearBalances::incr_balance(Self::TOTAL_STAKED_BALANCE, amount);
    }

    fn decr_total_staked_balance(amount: YoctoNear) {
        ContractNearBalances::decr_balance(Self::TOTAL_STAKED_BALANCE, amount);
    }

    pub(crate) fn total_unstaked_balance() -> YoctoNear {
        ContractNearBalances::near_balance(Self::TOTAL_UNSTAKED_BALANCE)
    }

    fn incr_total_unstaked_balance(amount: YoctoNear) {
        ContractNearBalances::incr_balance(Self::TOTAL_UNSTAKED_BALANCE, amount);
    }

    /// first tries to apply the debit against [`Self::UNSTAKED_LIQUIDITY_POOL`] and then against
    /// [`Self::TOTAL_UNSTAKED_BALANCE`]
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

    /// If there are unstaked funds awaiting withdrawal, then transfer the specified amount to the
    /// liquidity pool
    fn add_liquidity(amount: YoctoNear) {
        if amount == YoctoNear::ZERO {
            return;
        }

        let total_unstaked_balance =
            ContractNearBalances::near_balance(Self::TOTAL_UNSTAKED_BALANCE);
        if total_unstaked_balance == YoctoNear::ZERO {
            return;
        }

        let liquidity = min(amount, total_unstaked_balance);
        ContractNearBalances::decr_balance(Self::TOTAL_UNSTAKED_BALANCE, liquidity);
        let total_liquidity =
            ContractNearBalances::incr_balance(Self::UNSTAKED_LIQUIDITY_POOL, liquidity);
        LOG_EVENT_LIQUIDITY.log(format!("added={}, total={}", liquidity, total_liquidity));
    }

    pub(crate) fn liquidity() -> YoctoNear {
        ContractNearBalances::near_balance(Self::UNSTAKED_LIQUIDITY_POOL)
    }
}

impl Deploy for StakingPoolComponent {
    type Config = StakingPoolComponentConfig;

    /// default settings:
    /// - staking fee = 80 BPS (0.8%)
    /// - the staking pool is deployed as stopped, i.e., in order to start staking, the pool will
    ///   need to be explicitly started after deployment
    fn deploy(config: Self::Config) {
        // the contract account serves as the treasury account
        // we need to register an account with storage management in order to deposit STAKE into the
        // treasury
        let treasury = env::current_account_id();
        AccountManager::register_account_if_not_exists(&treasury);

        let state = State {
            stake_public_key: config.stake_public_key,
            status: Status::Offline(OfflineReason::Stopped),
            staking_fee: config.staking_fee.unwrap_or(80.into()),
            treasury_balance: YoctoNear::ZERO,
            last_contract_managed_total_balance: State::contract_managed_total_balance(),
        };
        let state = Self::new_state(state);
        state.save();
    }
}

#[derive(
    BorshSerialize, BorshDeserialize, Serialize, Deserialize, Debug, Clone, Copy, PartialEq,
)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub struct StakingPoolComponentConfig {
    pub stake_public_key: PublicKey,
    pub staking_fee: Option<BasisPoints>,
}

impl StakingPool for StakingPoolComponent {
    fn ops_stake_balance(&self, account_id: ValidAccountId) -> Option<StakeAccountBalances> {
        self.account_manager
            .storage_balance_of(account_id.clone())
            .map(|storage_balance| {
                let staked = {
                    let token_balance = self.stake_token.ft_balance_of(account_id.clone());
                    if token_balance == TokenAmount::ZERO {
                        None
                    } else {
                        Some(StakedBalance {
                            stake: token_balance,
                            near_value: self.ops_stake_token_value(Some(token_balance)),
                        })
                    }
                };

                let unstaked = self
                    .account_manager
                    .load_account_data(account_id.as_ref())
                    .map(|data| {
                        if data.unstaked_balances.total() == YoctoNear::ZERO {
                            None
                        } else {
                            Some(data.unstaked_balances.into())
                        }
                    })
                    .flatten();

                StakeAccountBalances {
                    storage_balance,
                    staked,
                    unstaked,
                }
            })
    }

    fn ops_stake(&mut self) -> PromiseOrValue<StakeAccountBalances> {
        let account_id = env::predecessor_account_id();
        let mut account = self
            .account_manager
            .registered_account_near_data(&account_id);

        Self::state_with_updated_earnings();

        // stake the account's total available storage balance + attached deposit
        let (near_amount, stake_token_amount) = {
            let account_storage_available_balance = account
                .storage_balance(self.account_manager.storage_balance_bounds().min)
                .available;
            account.dec_near_balance(account_storage_available_balance);

            let near = account_storage_available_balance + env::attached_deposit();
            ERR_NEAR_DEPOSIT_REQUIRED.assert_with_message(
                || near > YoctoNear::ZERO,
                || "deposit NEAR into storage balance or attach NEAR deposit",
            );
            let (stake, remainder) = self.near_to_stake(near);
            account.incr_near_balance(remainder);
            account.save();

            (near - remainder, stake)
        };

        if near_amount == YoctoNear::ZERO {
            // INVARIANT CHECK: if `near_amount` is zero, then `stake_token_amount` should be zero
            assert_eq!(stake_token_amount, TokenAmount::ZERO);
            // NOTE: any attached deposit be deposited into the account's storage balance - this, there
            // is no need to panic
            LOG_EVENT_NOT_ENOUGH_TO_STAKE.log("");
            return self.registered_stake_account_balance(&account_id);
        }

        State::add_liquidity(near_amount);
        self.stake(&account_id, near_amount, stake_token_amount)
    }

    fn ops_unstake(&mut self, amount: Option<YoctoNear>) -> PromiseOrValue<StakeAccountBalances> {
        let account_id = env::predecessor_account_id();
        ERR_ACCOUNT_NOT_REGISTERED.assert(|| self.account_manager.account_exists(&account_id));

        Self::state_with_updated_earnings();

        let stake_balance = self
            .stake_token
            .ft_balance_of(to_valid_account_id(&account_id));
        let stake_near_value = self.stake_near_value_rounded_down(stake_balance);
        let (near_amount, stake_token_amount) = match amount {
            None => (stake_near_value, stake_balance),
            Some(near_amount) => {
                ERR_INSUFFICIENT_FUNDS.assert(|| stake_near_value >= near_amount);
                // we round up the number of STAKE tokens to ensure that we never overdraw from the
                // staked balance - this is more than compensated for by the treasury dividend
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

        State::decr_total_staked_balance(near_amount);
        State::incr_total_unstaked_balance(near_amount);
        self.stake_token.ft_burn(&account_id, stake_token_amount);
        self.credit_unstaked_amount(&account_id, near_amount);

        let state = Self::state();
        match state.status {
            Status::Online => {
                let promise =
                    Self::create_stake_workflow(*state, &account_id, "ops_stake_finalize");
                PromiseOrValue::Promise(promise)
            }
            Status::Offline(_) => {
                LOG_EVENT_STATUS_OFFLINE.log("");
                self.registered_stake_account_balance(&account_id)
            }
        }
    }

    fn ops_restake(&mut self, amount: Option<YoctoNear>) -> PromiseOrValue<StakeAccountBalances> {
        let account_id = env::predecessor_account_id();
        ERR_ACCOUNT_NOT_REGISTERED.assert(|| self.account_manager.account_exists(&account_id));

        Self::state_with_updated_earnings();

        match self.account_manager.load_account_data(&account_id) {
            // account has no unstaked funds to restake
            None => match amount {
                None => self.registered_stake_account_balance(&account_id),
                Some(_) => {
                    ERR_INSUFFICIENT_FUNDS.panic();
                    unreachable!()
                }
            },
            Some(mut account) => {
                let (near_amount, stake_token_amount) = {
                    let near = amount.unwrap_or_else(|| account.unstaked_balances.total());
                    let (stake, remainder) = self.near_to_stake(near);
                    let stake_near_value = near - remainder;
                    account
                        .unstaked_balances
                        .debit_for_restaking(stake_near_value);
                    account.save();
                    State::decr_total_unstaked_balance(stake_near_value);
                    (stake_near_value, stake)
                };
                // NOTE: restaking does not add liquidity because no new funds are being deposited
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
            unstaked_balances
                .unstaked_balances
                .debit_available_balance(amount);
            if unstaked_balances.unstaked_balances.total() == YoctoNear::ZERO {
                unstaked_balances.delete();
            } else {
                unstaked_balances.save();
            }
            State::decr_total_unstaked_balance(amount);
            Promise::new(env::predecessor_account_id()).transfer(*amount);
        }

        Self::state_with_updated_earnings();
        match amount {
            // withdraw all available
            None => {
                if let Some(mut account_staked_data) =
                    self.account_manager.load_account_data(&account_id)
                {
                    account_staked_data.unstaked_balances.apply_liquidity();
                    let amount = account_staked_data.unstaked_balances.available();
                    if amount > YoctoNear::ZERO {
                        debit_available_balance(account_staked_data, amount);
                    }
                }
            }
            // withdraw specified amount
            Some(amount) => {
                ERR_INVALID.assert(|| amount > YoctoNear::ZERO, || "amount must be > 0");
                match self.account_manager.load_account_data(&account_id) {
                    Some(mut unstaked_balances) => {
                        unstaked_balances.unstaked_balances.apply_liquidity();
                        debit_available_balance(unstaked_balances, amount);
                    }
                    None => ERR_INSUFFICIENT_FUNDS.panic(),
                }
            }
        }

        self.ops_stake_balance(to_valid_account_id(&account_id))
            .unwrap()
    }

    fn ops_stake_transfer(
        &mut self,
        receiver_id: ValidAccountId,
        amount: YoctoNear,
        memo: Option<Memo>,
    ) -> TokenAmount {
        Self::state_with_updated_earnings();
        let stake_value = self.near_stake_value_rounded_up(amount);
        self.stake_token.ft_transfer(receiver_id, stake_value, memo);
        stake_value
    }

    fn ops_stake_transfer_call(
        &mut self,
        receiver_id: ValidAccountId,
        amount: YoctoNear,
        memo: Option<Memo>,
        msg: TransferCallMessage,
    ) -> Promise {
        Self::state_with_updated_earnings();
        let stake_value = self.near_stake_value_rounded_up(amount);
        self.stake_token
            .ft_transfer_call(receiver_id, stake_value, memo, msg)
    }

    fn ops_stake_token_value(&self, amount: Option<TokenAmount>) -> YoctoNear {
        let state = Self::state();
        // NOTE: we cannot check for earnings because it check if there is any attached deposit,
        // which is not permitted to be invoked in a view method
        self.compute_stake_near_value_rounded_down(
            amount.unwrap_or(YOCTO.into()),
            State::total_staked_balance() + state.check_for_earnings_in_view_mode(),
        )
    }

    fn ops_stake_status(&self) -> Status {
        Self::state().status
    }

    fn ops_stake_pool_balances(&self) -> StakingPoolBalances {
        StakingPoolBalances::load()
    }

    fn ops_stake_fee(&self) -> BasisPoints {
        Self::state().staking_fee
    }

    fn ops_stake_public_key(&self) -> PublicKey {
        Self::state().stake_public_key
    }
}

impl StakingPoolOperator for StakingPoolComponent {
    fn ops_stake_operator_command(&mut self, command: StakingPoolOperatorCommand) {
        self.account_manager.assert_operator();

        match command {
            StakingPoolOperatorCommand::StopStaking => Self::stop_staking(OfflineReason::Stopped),
            StakingPoolOperatorCommand::StartStaking => Self::start_staking(),
            StakingPoolOperatorCommand::UpdatePublicKey(public_key) => {
                Self::update_public_key(public_key)
            }
            StakingPoolOperatorCommand::UpdateStakingFee(fee) => Self::update_staking_fee(fee),
        }
    }
}

impl StakingPoolComponent {
    fn stop_staking(reason: OfflineReason) {
        // update status to offline
        let mut state = Self::state();
        state.status = Status::Offline(reason);
        state.save();

        // unstake all
        if env::account_locked_balance() > 0 {
            Promise::new(env::current_account_id())
                .stake(0, state.stake_public_key.into())
                .then(json_function_callback(
                    "ops_stake_stop_finalize",
                    Option::<()>::None,
                    YoctoNear::ZERO,
                    StakingPoolComponent::callback_gas(),
                ));
        }

        // log
        if let OfflineReason::StakeActionFailed = reason {
            ERR_STAKE_ACTION_FAILED.log("");
        }
        LOG_EVENT_STATUS_OFFLINE.log(reason);
    }

    fn start_staking() {
        let mut state = Self::state_with_updated_earnings();
        if let Status::Offline(_) = state.status {
            // update status
            {
                state.status = Status::Online;
                state.save();
            }

            // stake
            let total_staked_balance = State::total_staked_balance();
            if total_staked_balance > YoctoNear::ZERO {
                Promise::new(env::current_account_id())
                    .stake(*total_staked_balance, state.stake_public_key.into())
                    .then(json_function_callback(
                        "ops_stake_start_finalize",
                        Option::<()>::None,
                        YoctoNear::ZERO,
                        Self::callback_gas(),
                    ));
            }

            LOG_EVENT_STATUS_ONLINE.log("");
        }
    }

    fn update_public_key(public_key: PublicKey) {
        let mut state = Self::state();
        ERR_ILLEGAL_STATE.assert(
            || !state.status.is_online(),
            || "staking pool must be offline to update the staking public key",
        );
        state.stake_public_key = public_key;
        state.save();
    }

    fn update_staking_fee(fee: BasisPoints) {
        const MAX_STAKING_FEE: BasisPoints = BasisPoints(1000); // 10%
        ERR_INVALID.assert(
            || fee <= MAX_STAKING_FEE,
            || "max staking fee is 1000 BPS (10%)",
        );
        let mut state = Self::state();
        state.staking_fee = fee;
        state.save();
    }
}

impl StakeActionCallbacks for StakingPoolComponent {
    fn ops_stake_finalize(&mut self, account_id: AccountId) -> StakeAccountBalances {
        let state = Self::state_with_updated_earnings();
        if state.status.is_online() && !is_promise_success() {
            Self::stop_staking(OfflineReason::StakeActionFailed);
        }

        self.ops_stake_balance(to_valid_account_id(&account_id))
            .unwrap()
    }

    fn ops_stake_start_finalize(&mut self) {
        if is_promise_success() {
            LOG_EVENT_STATUS_ONLINE.log("staked");
        } else {
            Self::stop_staking(OfflineReason::StakeActionFailed);
        }
    }

    fn ops_stake_stop_finalize(&mut self) {
        if is_promise_success() {
            LOG_EVENT_STATUS_OFFLINE.log("all NEAR has been unstaked");
        } else {
            ERR_STAKE_ACTION_FAILED.log("failed to unstake when trying to stop staking pool");
        }
    }
}

impl Treasury for StakingPoolComponent {
    fn ops_stake_treasury_deposit(&mut self) -> PromiseOrValue<StakeAccountBalances> {
        let deposit = YoctoNear::from(env::attached_deposit());
        ERR_NEAR_DEPOSIT_REQUIRED.assert(|| deposit > YoctoNear::ZERO);

        let mut state = Self::state_with_updated_earnings();
        let stake = self.near_stake_value_rounded_down(deposit);
        state.treasury_balance += deposit;
        state.save();

        State::add_liquidity(deposit);
        self.stake(&env::current_account_id(), deposit, stake)
    }

    fn ops_stake_treasury_distribution(&mut self) {
        let deposit = YoctoNear::from(env::attached_deposit());
        ERR_NEAR_DEPOSIT_REQUIRED.assert(|| deposit > YoctoNear::ZERO);

        Self::state_with_updated_earnings();
        State::add_liquidity(deposit);
        self.stake(&env::current_account_id(), deposit, TokenAmount::ZERO);
    }

    fn ops_stake_treasury_transfer_to_owner(&mut self, amount: Option<YoctoNear>) {
        let owner_account_id = ContractOwnershipComponent.ops_owner();
        ERR_NOT_AUTHORIZED.assert(|| {
            let account_id = env::predecessor_account_id();
            if owner_account_id == account_id {
                return true;
            }
            let account = self
                .account_manager
                .registered_account_near_data(&account_id);
            account.contains_permissions(self.treasurer_permission().into())
        });

        AccountManager::register_account_if_not_exists(&owner_account_id);

        let mut state = Self::state();
        self.pay_dividend(state.treasury_balance);

        let treasury_account = env::current_account_id();
        let (amount, stake) = {
            let treasury_balance = self
                .stake_token
                .ft_balance_of(to_valid_account_id(&treasury_account));
            if treasury_balance == TokenAmount::ZERO {
                return;
            }

            let treasury_near_balance = self.stake_near_value_rounded_down(treasury_balance);
            let amount = match amount {
                None => treasury_near_balance,
                Some(amount) => {
                    ERR_INSUFFICIENT_FUNDS.assert(|| treasury_near_balance >= amount);
                    amount
                }
            };
            let stake = self.near_stake_value_rounded_up(amount);
            (amount, min(treasury_balance, stake))
        };

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

impl TransferReceiver for StakingPoolComponent {
    fn ft_on_transfer(
        &mut self,
        _sender_id: ValidAccountId,
        _amount: TokenAmount,
        _msg: TransferCallMessage,
    ) -> PromiseOrValue<TokenAmount> {
        let mut state = Self::state();
        let treasury_stake_balance = self
            .stake_token
            .ft_balance_of(to_valid_account_id(&env::current_account_id()));
        state.treasury_balance = self.stake_near_value_rounded_down(treasury_stake_balance);
        state.save();
        LOG_EVENT_TREASURY_DEPOSIT.log("");
        PromiseOrValue::Value(TokenAmount::ZERO)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
struct StakeActionCallbackArgs {
    account_id: AccountId,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
struct ResumeFinalizeCallbackArgs {
    pub total_staked_balance: YoctoNear,
}

// staking related methods
impl StakingPoolComponent {
    /// Stakes the NEAR and mints the corresponding STAKE for the account
    ///
    /// ## Args
    /// - `near_amount` - new funds that are being staked
    /// - `stake_token_amount` - the amount of tokens that will be minted
    fn stake(
        &mut self,
        account_id: &str,
        near_amount: YoctoNear,
        stake_token_amount: TokenAmount,
    ) -> PromiseOrValue<StakeAccountBalances> {
        if near_amount == YoctoNear::ZERO {
            // INVARIANT CHECK: if `near_amount` is zero, then `stake_token_amount` should be zero
            assert_eq!(stake_token_amount, TokenAmount::ZERO);
            // NOTE: any attached deposit be deposited into the account's storage balance - this, there
            // is no need to panic
            LOG_EVENT_NOT_ENOUGH_TO_STAKE.log("");
            return self.registered_stake_account_balance(account_id);
        }

        LOG_EVENT_STAKE.log(format!(
            "near_amount={}, stake_token_amount={}",
            near_amount, stake_token_amount
        ));
        // update balances
        let state = {
            let mut state = Self::state();
            State::incr_total_staked_balance(near_amount);
            state.last_contract_managed_total_balance += near_amount;
            state.save();
            state
        };
        let state = self.pay_dividend_and_apply_staking_fees(
            state,
            account_id,
            near_amount,
            stake_token_amount,
        );
        match state.status {
            Status::Online => PromiseOrValue::Promise(Self::create_stake_workflow(
                *state,
                account_id,
                "ops_stake_finalize",
            )),
            Status::Offline(_) => {
                LOG_EVENT_STATUS_OFFLINE.log("");
                self.registered_stake_account_balance(account_id)
            }
        }
    }
}

impl StakingPoolComponent {
    /// compute how gas this function call requires to complete and give the remainder of the gas to
    /// the callback
    fn callback_gas() -> Gas {
        let transaction_gas_fees = Gas::compute(vec![
            (
                TransactionResource::ActionReceipt(SenderIsReceiver(true)),
                2, // stake + callback
            ),
            (
                TransactionResource::Action(ActionType::Stake(SenderIsReceiver(true))),
                1,
            ),
            (
                TransactionResource::Action(ActionType::FunctionCall(
                    SenderIsReceiver(true),
                    ByteLen(512),
                )),
                1,
            ),
            (
                TransactionResource::DataReceipt(SenderIsReceiver(true), ByteLen(0)),
                1,
            ),
        ]);

        const GAS_REQUIRED_TO_COMPLETE_THIS_CALL: u64 = 5 * TERA;
        let gas = (env::prepaid_gas() - env::used_gas())
            .saturating_sub(*transaction_gas_fees)
            .saturating_sub(GAS_REQUIRED_TO_COMPLETE_THIS_CALL)
            .into();

        let min_callback_gas = Self::min_callback_gas();
        ERR_INVALID.assert(
            || gas >= min_callback_gas,
            || {
                let min_required_gas = *min_callback_gas
                    + env::used_gas()
                    + *transaction_gas_fees
                    + GAS_REQUIRED_TO_COMPLETE_THIS_CALL;
                format!(
                    "not enough gas was attached - min required gas is {} TGas",
                    min_required_gas / TERA + 1
                )
            },
        );
        gas
    }

    // TODO: check actual gas usage on deployed contract
    fn min_callback_gas() -> Gas {
        const COMPUTE: Gas = Gas(5 * TERA);
        const RECEIPT: TransactionResource =
            TransactionResource::ActionReceipt(SenderIsReceiver(true));
        const STAKE_ACTION: TransactionResource =
            TransactionResource::Action(ActionType::Stake(SenderIsReceiver(true)));
        Gas::compute(vec![(RECEIPT, 1), (STAKE_ACTION, 1)]) + COMPUTE
    }

    fn treasurer_permission(&self) -> Permission {
        self.account_manager
            .permission_by_name(PERMISSION_TREASURER)
            .unwrap()
    }

    fn treasury_stake_balance(&self) -> (TokenAmount, YoctoNear) {
        let treasury_stake_balance = self
            .stake_token
            .ft_balance_of(to_valid_account_id(&env::current_account_id()));
        let treasury_near_value = self.stake_near_value_rounded_down(treasury_stake_balance);
        (treasury_stake_balance, treasury_near_value)
    }

    /// returns the current treasury balance after paying the dividend
    fn pay_dividend(&mut self, treasury_balance: YoctoNear) -> YoctoNear {
        let (treasury_stake_balance, current_treasury_near_value) = self.treasury_stake_balance();
        if treasury_balance == YoctoNear::ZERO {
            return current_treasury_near_value;
        }

        let treasury_staking_earnings = current_treasury_near_value - treasury_balance;
        let treasury_staking_earnings_stake_value =
            self.near_stake_value_rounded_down(treasury_staking_earnings);
        if treasury_staking_earnings_stake_value == TokenAmount::ZERO {
            return current_treasury_near_value;
        }

        self.stake_token.ft_burn(
            &env::current_account_id(),
            treasury_staking_earnings_stake_value,
        );
        LOG_EVENT_TREASURY_DIVIDEND.log(format!(
            "{} yoctoNEAR / {} yoctoSTAKE",
            treasury_staking_earnings, treasury_staking_earnings_stake_value
        ));
        self.stake_near_value_rounded_down(
            treasury_stake_balance - treasury_staking_earnings_stake_value,
        )
    }

    /// if treasury has earned staking rewards then burn STAKE tokens to distribute earnings
    fn pay_dividend_and_apply_staking_fees(
        &mut self,
        mut state: ComponentState<State>,
        account_id: &str,
        amount: YoctoNear,
        stake_token_amount: TokenAmount,
    ) -> ComponentState<State> {
        // stake_token_amount will be ZERO if this is a funds distribution
        // see [`Treasury::ops_stake_treasury_distribution`]
        if stake_token_amount > TokenAmount::ZERO {
            self.stake_token.ft_mint(&account_id, stake_token_amount);
        }

        // if treasury received staking rewards, then pay out the dividend
        state.treasury_balance = self.pay_dividend(state.treasury_balance);

        // treasury and owner accounts do not get charged staking fees
        let owner_id = ContractOwnershipComponent.ops_owner();
        if stake_token_amount > TokenAmount::ZERO
            && state.staking_fee > BasisPoints::ZERO
            && account_id != &env::current_account_id()
            && account_id != &owner_id
        {
            let staking_fee = self.near_stake_value_rounded_down(amount * state.staking_fee);
            if staking_fee > TokenAmount::ZERO {
                self.stake_token.ft_burn(&account_id, staking_fee);
                self.stake_token.ft_mint(&owner_id, staking_fee);
            }
        }

        state.save();
        state
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

    pub(crate) fn state() -> ComponentState<State> {
        Self::load_state().expect("component has not been deployed")
    }

    pub(crate) fn state_with_updated_earnings() -> ComponentState<State> {
        let mut state = Self::state();

        let contract_managed_total_balance = State::contract_managed_total_balance();
        let earnings: YoctoNear = contract_managed_total_balance
            .saturating_sub(*state.last_contract_managed_total_balance)
            .into();
        if earnings > YoctoNear::ZERO {
            LOG_EVENT_EARNINGS.log(earnings);
            State::incr_total_staked_balance(earnings);
        }
        state.last_contract_managed_total_balance = contract_managed_total_balance;
        state.save();
        state
    }

    fn credit_unstaked_amount(&self, account_id: &str, amount: YoctoNear) {
        let mut account = self.account_manager.registered_account_data(&account_id);
        account.unstaked_balances.credit_unstaked(amount);
        account.save();
    }

    fn create_stake_workflow(state: State, account_id: &str, callback: &str) -> Promise {
        let stake = Promise::new(env::current_account_id()).stake(
            *State::total_staked_balance(),
            state.stake_public_key.into(),
        );
        let finalize = json_function_callback(
            callback,
            Some(StakeActionCallbackArgs {
                account_id: account_id.to_string(),
            }),
            YoctoNear::ZERO,
            Self::callback_gas(),
        );
        stake.then(finalize)
    }

    fn stake_near_value_rounded_down(&self, stake: TokenAmount) -> YoctoNear {
        self.compute_stake_near_value_rounded_down(stake, State::total_staked_balance())
    }

    fn compute_stake_near_value_rounded_down(
        &self,
        stake: TokenAmount,
        total_staked_near_balance: YoctoNear,
    ) -> YoctoNear {
        if *stake == 0 {
            return YoctoNear::ZERO;
        }

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
    fn near_to_stake(&self, amount: YoctoNear) -> (TokenAmount, YoctoNear) {
        let stake = self.near_stake_value_rounded_down(amount);
        let stake_near_value = self.stake_near_value_rounded_down(stake);
        (stake, amount - stake_near_value)
    }

    fn near_stake_value_rounded_down(&self, amount: YoctoNear) -> TokenAmount {
        if amount == YoctoNear::ZERO {
            return TokenAmount::ZERO;
        }

        let total_staked_balance = *State::total_staked_balance();
        let ft_total_supply = *self.stake_token.ft_total_supply();
        if total_staked_balance == 0 || ft_total_supply == 0 {
            return (*amount).into();
        }

        (U256::from(ft_total_supply) * U256::from(*amount) / U256::from(total_staked_balance))
            .as_u128()
            .into()
    }

    fn near_stake_value_rounded_up(&self, amount: YoctoNear) -> TokenAmount {
        if amount == YoctoNear::ZERO {
            return TokenAmount::ZERO;
        }

        let total_staked_balance = *State::total_staked_balance();
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
mod tests_staking_pool {
    use super::*;
    use crate::*;
    use oysterpack_smart_account_management::{
        components::account_management::{
            AccountManagementComponent, AccountManagementComponentConfig,
        },
        ContractPermissions,
    };
    use oysterpack_smart_contract::{
        components::contract_operator::ContractOperatorComponent, ContractOperator,
    };
    use oysterpack_smart_fungible_token::components::fungible_token::FungibleTokenConfig;
    use oysterpack_smart_fungible_token::{
        components::fungible_token::FungibleTokenComponent, FungibleToken, Metadata, Name, Spec,
        Symbol, FT_METADATA_SPEC,
    };
    use oysterpack_smart_near::{
        component::*,
        near_sdk::{env, serde_json, test_utils},
        *,
    };
    use oysterpack_smart_near_test::*;
    use std::collections::HashMap;
    use std::convert::*;

    pub type AccountManager = AccountManagementComponent<StakeAccountData>;

    pub type StakeFungibleToken = FungibleTokenComponent<StakeAccountData>;

    const OWNER: &str = "owner";
    const ACCOUNT: &str = "bob";

    pub fn deploy_stake_contract(owner: Option<ValidAccountId>, stake_public_key: PublicKey) {
        let owner = owner.unwrap_or_else(|| env::predecessor_account_id().try_into().unwrap());
        ContractOwnershipComponent::deploy(owner.clone());

        AccountManager::deploy(AccountManagementComponentConfig {
            storage_usage_bounds: None,
            admin_account: owner.clone(),
            component_account_storage_mins: Some(vec![StakeFungibleToken::account_storage_min]),
        });

        // transfer any contract balance to the owner - minus the contract operational balance
        {
            contract_operator().ops_operator_lock_storage_balance(10000.into());
            let account_manager = account_manager();
            let mut owner_account = account_manager.registered_account_near_data(owner.as_ref());
            owner_account
                .incr_near_balance(ContractOwnershipComponent.ops_owner_balance().available);
            owner_account.save();
        }

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
            stake_public_key,
            staking_fee: None,
        });
    }

    fn account_manager() -> AccountManager {
        StakeFungibleToken::register_storage_management_event_handler();

        let contract_permissions = {
            let mut permissions = HashMap::with_capacity(1);
            permissions.insert(0, PERMISSION_TREASURER);
            ContractPermissions(permissions)
        };

        AccountManager::new(contract_permissions)
    }

    fn ft_stake() -> StakeFungibleToken {
        StakeFungibleToken::new(account_manager())
    }

    fn contract_operator() -> ContractOperatorComponent<StakeAccountData> {
        ContractOperatorComponent::new(account_manager())
    }

    fn staking_pool() -> StakingPoolComponent {
        StakingPoolComponent::new(account_manager(), ft_stake())
    }

    fn staking_public_key() -> PublicKey {
        serde_json::from_str("\"ed25519:GTi3gtSio5ZYYKTT8WVovqJEob6KqdmkTi8KqGSfwqdm\"").unwrap()
    }

    fn log_contract_managed_total_balance(msg: &str) {
        let total_contract_balance: YoctoNear =
            (env::account_balance() + env::account_locked_balance() - env::attached_deposit())
                .into();
        println!(
            r#"### contract_managed_total_balance - {}
env::account_balance()                          {}
env::account_locked_balance()                   {}
env::attached_deposit()                         {}
AccountMetrics::load().total_near_balance       {}      
--------------------------------------------------
contract_managed_total_balance                  {}
last_contract_managed_total_balance             {}
**************************************************"#,
            msg,
            env::account_balance(),
            env::account_locked_balance(),
            env::attached_deposit(),
            AccountMetrics::load().total_near_balance,
            total_contract_balance - AccountMetrics::load().total_near_balance,
            StakingPoolComponent::state().last_contract_managed_total_balance,
        );
    }

    #[cfg(test)]
    mod tests_offline {
        use super::*;

        #[cfg(test)]
        mod tests_stake {
            use super::*;

            #[test]
            fn with_attached_deposit() {
                // Arrange
                let mut ctx = new_context(ACCOUNT);
                ctx.predecessor_account_id = OWNER.to_string();
                testing_env!(ctx.clone());

                deploy_stake_contract(Some(to_valid_account_id(OWNER)), staking_public_key());
                let contract_managed_total_balance = State::contract_managed_total_balance();

                let mut account_manager = account_manager();
                let mut staking_pool = staking_pool();
                assert!(!staking_pool.ops_stake_status().is_online());

                let ft_stake = ft_stake();

                // register account
                ctx.account_balance = env::account_balance();
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx.clone());
                account_manager.storage_deposit(None, Some(true));

                // assert that there was no net change to contract_managed_total_balance
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                assert_eq!(
                    contract_managed_total_balance,
                    State::contract_managed_total_balance()
                );

                assert_eq!(
                    account_manager
                        .storage_balance_of(to_valid_account_id(ACCOUNT))
                        .unwrap()
                        .available,
                    YoctoNear::ZERO
                );

                // Act
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Value(balances) = staking_pool.ops_stake() {
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);

                    println!("{}", serde_json::to_string_pretty(&balances).unwrap());
                    let staking_fee =
                        staking_pool.ops_stake_fee() * staking_pool.ops_stake_token_value(None);
                    assert!(balances.unstaked.is_none());
                    match balances.staked.as_ref() {
                        Some(stake) => {
                            assert_eq!(stake.stake, (YOCTO - *staking_fee).into());
                            assert_eq!(stake.near_value, (YOCTO - *staking_fee).into());
                        }
                        None => panic!("expected staked balance"),
                    }

                    assert_eq!(
                        balances,
                        staking_pool
                            .ops_stake_balance(to_valid_account_id(ACCOUNT))
                            .unwrap()
                    );

                    // Assert
                    assert_eq!(logs, vec![
                        "[INFO] [STAKE] near_amount=1000000000000000000000000, stake_token_amount=1000000000000000000000000",
                        "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
                        "[INFO] [FT_MINT] account: bob, amount: 1000000000000000000000000",
                        "[INFO] [FT_BURN] account: bob, amount: 8000000000000000000000",
                        "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
                        "[INFO] [FT_MINT] account: owner, amount: 8000000000000000000000",
                        "[WARN] [STATUS_OFFLINE] ",
                    ]);
                    let staking_fee = staking_pool.ops_stake_fee() * YOCTO;
                    assert_eq!(staking_pool.ops_stake_token_value(None), YOCTO.into());
                    assert_eq!(
                        ft_stake.ft_balance_of(to_valid_account_id(ACCOUNT)),
                        (YOCTO - *staking_fee).into()
                    );
                    assert_eq!(
                        ft_stake.ft_balance_of(to_valid_account_id(OWNER)),
                        (*staking_fee).into()
                    );
                    let state = StakingPoolComponent::state();
                    println!("{:#?}", *state);
                    assert_eq!(State::total_staked_balance(), YOCTO.into());
                    assert_eq!(state.treasury_balance, YoctoNear::ZERO);

                    log_contract_managed_total_balance("after staking");

                    ctx.account_balance = env::account_balance();
                    ctx.attached_deposit = 0;
                    testing_env!(ctx.clone());
                    assert_eq!(
                        contract_managed_total_balance + YOCTO,
                        State::contract_managed_total_balance()
                    );
                    let stake_pool_balances = staking_pool.ops_stake_pool_balances();
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&stake_pool_balances).unwrap()
                    );
                    assert_eq!(stake_pool_balances.treasury_balance, YoctoNear::ZERO);
                    assert_eq!(stake_pool_balances.total_staked, YOCTO.into());
                    assert_eq!(stake_pool_balances.total_unstaked, YoctoNear::ZERO);
                    assert_eq!(stake_pool_balances.unstaked_liquidity, YoctoNear::ZERO);
                } else {
                    panic!("expected value")
                }
            }

            #[test]
            fn with_zero_attached_deposit_and_storage_available_balance() {
                // Arrange
                let mut ctx = new_context(OWNER);
                testing_env!(ctx.clone());

                deploy_stake_contract(Some(to_valid_account_id(OWNER)), staking_public_key());
                let contract_managed_total_balance = State::contract_managed_total_balance();

                let mut account_manager = account_manager();
                let mut staking_pool = staking_pool();
                assert!(!staking_pool.ops_stake_status().is_online());

                let ft_stake = ft_stake();

                // register account
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx.clone());
                account_manager.storage_deposit(None, Some(true));
                account_manager.storage_deposit(None, Some(false));

                // assert that there was no net change to contract_managed_total_balance
                ctx.account_balance = env::account_balance() + YOCTO;
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                assert_eq!(
                    contract_managed_total_balance,
                    State::contract_managed_total_balance()
                );

                assert_eq!(
                    account_manager
                        .storage_balance_of(to_valid_account_id(ACCOUNT))
                        .unwrap()
                        .available,
                    YOCTO.into()
                );

                log_contract_managed_total_balance("before staking");
                assert_eq!(staking_pool.ops_stake_token_value(None), YOCTO.into());

                // Act
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Value(balances) = staking_pool.ops_stake() {
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);

                    println!("{}", serde_json::to_string_pretty(&balances).unwrap());
                    let staking_fee = staking_pool.ops_stake_fee() * YOCTO;
                    assert!(balances.unstaked.is_none());
                    match balances.staked.as_ref() {
                        Some(stake) => {
                            assert_eq!(stake.stake, (YOCTO - *staking_fee).into());
                            assert_eq!(stake.near_value, (YOCTO - *staking_fee).into());
                        }
                        None => panic!("expected staked balance"),
                    }

                    assert_eq!(
                        balances,
                        staking_pool
                            .ops_stake_balance(to_valid_account_id(ACCOUNT))
                            .unwrap()
                    );

                    // Assert
                    assert_eq!(logs, vec![
                        "[INFO] [ACCOUNT_STORAGE_CHANGED] Withdrawal(YoctoNear(1000000000000000000000000))",
                        "[INFO] [STAKE] near_amount=1000000000000000000000000, stake_token_amount=1000000000000000000000000",
                        "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
                        "[INFO] [FT_MINT] account: bob, amount: 1000000000000000000000000",
                        "[INFO] [FT_BURN] account: bob, amount: 8000000000000000000000",
                        "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
                        "[INFO] [FT_MINT] account: owner, amount: 8000000000000000000000",
                        "[WARN] [STATUS_OFFLINE] ",
                    ]);
                    let staking_fee = staking_pool.ops_stake_fee() * YOCTO;
                    assert_eq!(staking_pool.ops_stake_token_value(None), YOCTO.into());
                    assert_eq!(
                        ft_stake.ft_balance_of(to_valid_account_id(ACCOUNT)),
                        (YOCTO - *staking_fee).into()
                    );
                    assert_eq!(
                        ft_stake.ft_balance_of(to_valid_account_id(OWNER)),
                        (*staking_fee).into()
                    );
                    let state = StakingPoolComponent::state();
                    println!("{:#?}", *state);
                    assert_eq!(State::total_staked_balance(), YOCTO.into());
                    assert_eq!(state.treasury_balance, YoctoNear::ZERO);

                    log_contract_managed_total_balance("after staking");

                    ctx.account_balance = env::account_balance();
                    ctx.attached_deposit = 0;
                    testing_env!(ctx.clone());
                    assert_eq!(
                        contract_managed_total_balance + YOCTO,
                        State::contract_managed_total_balance()
                    );
                    let stake_pool_balances = staking_pool.ops_stake_pool_balances();
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&stake_pool_balances).unwrap()
                    );
                    assert_eq!(stake_pool_balances.treasury_balance, YoctoNear::ZERO);
                    assert_eq!(stake_pool_balances.total_staked, YOCTO.into());
                    assert_eq!(stake_pool_balances.total_unstaked, YoctoNear::ZERO);
                    assert_eq!(stake_pool_balances.unstaked_liquidity, YoctoNear::ZERO);
                } else {
                    panic!("expected value")
                }
            }
        }

        #[cfg(test)]
        mod tests_unstake {
            use super::*;

            #[test]
            fn unstake_partial() {
                // Arrange
                let mut ctx = new_context(ACCOUNT);
                ctx.predecessor_account_id = OWNER.to_string();
                testing_env!(ctx.clone());

                deploy_stake_contract(Some(to_valid_account_id(OWNER)), staking_public_key());

                let mut account_manager = account_manager();
                let mut staking_pool = staking_pool();
                assert!(!staking_pool.ops_stake_status().is_online());

                // register account
                ctx.account_balance = env::account_balance();
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx.clone());
                account_manager.storage_deposit(None, Some(true));
                account_manager.storage_deposit(None, Some(false));

                // stake storage deposit
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Value(balances) = staking_pool.ops_stake() {
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);

                    let pool_balances = staking_pool.ops_stake_pool_balances();
                    println!(
                        "pool_balances before unstaking: {}",
                        serde_json::to_string_pretty(&pool_balances).unwrap()
                    );

                    let staked_balance = balances.staked.as_ref().unwrap().near_value;

                    ctx.account_balance = env::account_balance();
                    ctx.attached_deposit = 0;
                    testing_env!(ctx.clone());
                    if let PromiseOrValue::Value(balances_after_unstaking) =
                        staking_pool.ops_unstake(Some((*staked_balance / 4).into()))
                    {
                        let logs = test_utils::get_logs();
                        println!("{:#?}", logs);
                        assert_eq!(logs, vec![
                            "[INFO] [UNSTAKE] near_amount=248000000000000000000000, stake_token_amount=248000000000000000000000",
                            "[INFO] [FT_BURN] account: bob, amount: 248000000000000000000000",
                            "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(184)",
                            "[WARN] [STATUS_OFFLINE] ",
                        ]);

                        println!(
                            "account balances_after_unstaking: {}",
                            serde_json::to_string_pretty(&balances_after_unstaking).unwrap()
                        );
                        assert_eq!(
                            balances_after_unstaking.unstaked.as_ref().unwrap().total,
                            staked_balance / 4
                        );
                        assert_eq!(
                            balances_after_unstaking
                                .unstaked
                                .as_ref()
                                .unwrap()
                                .available,
                            YoctoNear::ZERO
                        );

                        let pool_balances = staking_pool.ops_stake_pool_balances();
                        println!(
                            "pool_balances: {}",
                            serde_json::to_string_pretty(&pool_balances).unwrap()
                        );
                        assert_eq!(
                            pool_balances.total_staked,
                            balances_after_unstaking.staked.as_ref().unwrap().near_value
                                + staking_pool
                                    .ops_stake_balance(to_valid_account_id(OWNER))
                                    .unwrap()
                                    .staked
                                    .unwrap()
                                    .near_value
                        );
                        assert_eq!(
                            pool_balances.total_unstaked,
                            balances_after_unstaking.unstaked.as_ref().unwrap().total
                        );
                    } else {
                        panic!("expected value")
                    }
                } else {
                    panic!("expected value")
                }
            }

            #[test]
            fn unstake_all() {
                // Arrange
                let mut ctx = new_context(ACCOUNT);
                ctx.predecessor_account_id = OWNER.to_string();
                testing_env!(ctx.clone());

                deploy_stake_contract(Some(to_valid_account_id(OWNER)), staking_public_key());

                let mut account_manager = account_manager();
                let mut staking_pool = staking_pool();
                assert!(!staking_pool.ops_stake_status().is_online());

                // register account
                ctx.account_balance = env::account_balance();
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx.clone());
                account_manager.storage_deposit(None, Some(true));
                account_manager.storage_deposit(None, Some(false));

                // stake storage deposit
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Value(balances) = staking_pool.ops_stake() {
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);

                    let pool_balances = staking_pool.ops_stake_pool_balances();
                    println!(
                        "pool_balances before unstaking: {}",
                        serde_json::to_string_pretty(&pool_balances).unwrap()
                    );

                    let staked_balance = balances.staked.as_ref().unwrap().near_value;

                    ctx.account_balance = env::account_balance();
                    ctx.attached_deposit = 0;
                    testing_env!(ctx.clone());
                    if let PromiseOrValue::Value(balances_after_unstaking) =
                        staking_pool.ops_unstake(None)
                    {
                        let logs = test_utils::get_logs();
                        println!("{:#?}", logs);
                        assert_eq!(logs, vec![
                            "[INFO] [UNSTAKE] near_amount=992000000000000000000000, stake_token_amount=992000000000000000000000",
                            "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(-104)",
                            "[INFO] [FT_BURN] account: bob, amount: 992000000000000000000000",
                            "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(184)",
                            "[WARN] [STATUS_OFFLINE] ",
                        ]);

                        println!(
                            "account balances_after_unstaking: {}",
                            serde_json::to_string_pretty(&balances_after_unstaking).unwrap()
                        );
                        assert_eq!(
                            balances_after_unstaking.unstaked.as_ref().unwrap().total,
                            staked_balance
                        );
                        assert_eq!(
                            balances_after_unstaking
                                .unstaked
                                .as_ref()
                                .unwrap()
                                .available,
                            YoctoNear::ZERO
                        );

                        let pool_balances = staking_pool.ops_stake_pool_balances();
                        println!(
                            "pool_balances: {}",
                            serde_json::to_string_pretty(&pool_balances).unwrap()
                        );
                        assert_eq!(
                            pool_balances.total_staked,
                            staking_pool
                                .ops_stake_balance(to_valid_account_id(OWNER))
                                .unwrap()
                                .staked
                                .unwrap()
                                .near_value
                        );
                        assert_eq!(
                            pool_balances.total_unstaked,
                            balances_after_unstaking.unstaked.as_ref().unwrap().total
                        );
                    } else {
                        panic!("expected value")
                    }
                } else {
                    panic!("expected value")
                }
            }
        }

        #[cfg(test)]
        mod tests_withdraw {
            use super::*;

            #[test]
            fn withdraw_partial() {
                // Arrange
                let mut ctx = new_context(ACCOUNT);
                ctx.predecessor_account_id = OWNER.to_string();
                testing_env!(ctx.clone());

                deploy_stake_contract(Some(to_valid_account_id(OWNER)), staking_public_key());

                let mut account_manager = account_manager();
                let mut staking_pool = staking_pool();
                assert!(!staking_pool.ops_stake_status().is_online());

                // register account
                ctx.account_balance = env::account_balance();
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx.clone());
                account_manager.storage_deposit(None, Some(true));
                account_manager.storage_deposit(None, Some(false));

                // stake storage deposit
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                } else {
                    panic!("expected value");
                }

                // unstake all
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Value(_) = staking_pool.ops_unstake(None) {
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                } else {
                    panic!("expected value");
                }

                let balances_before_withdrawal = staking_pool
                    .ops_stake_balance(to_valid_account_id(ACCOUNT))
                    .unwrap();

                // Act
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                ctx.epoch_height = env::epoch_height() + 4;
                testing_env!(ctx.clone());
                let balances = staking_pool.ops_stake_withdraw(Some((1000).into()));

                // Assert
                assert!(test_utils::get_logs().is_empty());

                assert_eq!(
                    balances.unstaked.as_ref().unwrap().total,
                    balances_before_withdrawal.unstaked.as_ref().unwrap().total - 1000
                );

                let receipts = deserialize_receipts();
                assert_eq!(receipts.len(), 1);
                match &receipts[0].actions[0] {
                    Action::Transfer(action) => {
                        assert_eq!(action.deposit, 1000);
                    }
                    _ => panic!("expected transfer action"),
                }
            }

            #[test]
            fn withdraw_available_using_liquidity() {
                // Arrange
                let mut ctx = new_context(ACCOUNT);
                ctx.predecessor_account_id = OWNER.to_string();
                testing_env!(ctx.clone());

                deploy_stake_contract(Some(to_valid_account_id(OWNER)), staking_public_key());

                let mut account_manager = account_manager();
                let mut staking_pool = staking_pool();
                assert!(!staking_pool.ops_stake_status().is_online());

                // register account
                ctx.account_balance = env::account_balance();
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx.clone());
                account_manager.storage_deposit(None, Some(true));
                account_manager.storage_deposit(None, Some(false));

                // stake storage deposit
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                } else {
                    panic!("expected value");
                }

                // unstake all
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Value(_) = staking_pool.ops_unstake(None) {
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                } else {
                    panic!("expected value");
                }

                let balances_before_withdrawal = staking_pool
                    .ops_stake_balance(to_valid_account_id(ACCOUNT))
                    .unwrap();

                State::add_liquidity((YOCTO / 2).into());
                // Act
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                let balances = staking_pool.ops_stake_withdraw(None);

                // Assert
                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(
                    logs,
                    vec!["[INFO] [LIQUIDITY] removed=500000000000000000000000, total=0",]
                );

                assert_eq!(
                    balances.unstaked.as_ref().unwrap().total,
                    balances_before_withdrawal.unstaked.as_ref().unwrap().total - (YOCTO / 2)
                );
                assert_eq!(
                    balances.unstaked.as_ref().unwrap().available,
                    YoctoNear::ZERO
                );

                let receipts = deserialize_receipts();
                assert_eq!(receipts.len(), 1);
                match &receipts[0].actions[0] {
                    Action::Transfer(action) => {
                        assert_eq!(action.deposit, (YOCTO / 2));
                    }
                    _ => panic!("expected transfer action"),
                }
            }

            #[test]
            fn withdraw_all() {
                // Arrange
                let mut ctx = new_context(ACCOUNT);
                ctx.predecessor_account_id = OWNER.to_string();
                testing_env!(ctx.clone());

                deploy_stake_contract(Some(to_valid_account_id(OWNER)), staking_public_key());

                let mut account_manager = account_manager();
                let mut staking_pool = staking_pool();
                assert!(!staking_pool.ops_stake_status().is_online());

                // register account
                ctx.account_balance = env::account_balance();
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx.clone());
                account_manager.storage_deposit(None, Some(true));
                account_manager.storage_deposit(None, Some(false));

                // stake storage deposit
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                } else {
                    panic!("expected value");
                }

                // unstake all
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Value(_) = staking_pool.ops_unstake(None) {
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                } else {
                    panic!("expected value");
                }

                let balances_before_withdrawal = staking_pool
                    .ops_stake_balance(to_valid_account_id(ACCOUNT))
                    .unwrap();

                // Act
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                ctx.epoch_height = env::epoch_height() + 4;
                testing_env!(ctx.clone());
                let balances = staking_pool.ops_stake_withdraw(None);

                // Assert
                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(
                    logs,
                    vec!["[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(-184)",]
                );

                assert!(balances.unstaked.is_none());

                let receipts = deserialize_receipts();
                assert_eq!(receipts.len(), 1);
                match &receipts[0].actions[0] {
                    Action::Transfer(action) => {
                        assert_eq!(
                            action.deposit,
                            *balances_before_withdrawal.unstaked.as_ref().unwrap().total
                        );
                    }
                    _ => panic!("expected transfer action"),
                }
            }
        }

        #[cfg(test)]
        mod tests_restake {
            use super::*;

            #[test]
            fn restake_partial() {
                // Arrange
                let mut ctx = new_context(ACCOUNT);
                ctx.predecessor_account_id = OWNER.to_string();
                testing_env!(ctx.clone());

                deploy_stake_contract(Some(to_valid_account_id(OWNER)), staking_public_key());

                let mut account_manager = account_manager();
                let mut staking_pool = staking_pool();
                assert!(!staking_pool.ops_stake_status().is_online());

                // register account
                ctx.account_balance = env::account_balance();
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx.clone());
                account_manager.storage_deposit(None, Some(true));
                account_manager.storage_deposit(None, Some(false));

                // stake storage deposit
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                } else {
                    panic!("expected value");
                }

                // unstake all
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Value(_) = staking_pool.ops_unstake(None) {
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                } else {
                    panic!("expected value");
                }

                let balances_before_restaking = staking_pool
                    .ops_stake_balance(to_valid_account_id(ACCOUNT))
                    .unwrap();

                // Act
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Value(balances) =
                    staking_pool.ops_restake(Some((1000).into()))
                {
                    // Assert
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                    assert_eq!(
                        logs,
                        vec![
                            "[INFO] [STAKE] near_amount=1000, stake_token_amount=1000",
                            "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
                            "[INFO] [FT_MINT] account: bob, amount: 1000",
                            "[INFO] [FT_BURN] account: bob, amount: 8",
                            "[INFO] [FT_MINT] account: owner, amount: 8",
                            "[WARN] [STATUS_OFFLINE] ",
                        ]
                    );

                    println!("{}", serde_json::to_string_pretty(&balances).unwrap());
                    assert_eq!(
                        balances.unstaked.as_ref().unwrap().total,
                        balances_before_restaking.unstaked.as_ref().unwrap().total - 1000
                    );
                    let staking_fee = staking_pool.ops_stake_fee() * 1000;
                    assert_eq!(
                        balances.staked.as_ref().unwrap().near_value,
                        (1000 - *staking_fee).into()
                    );
                } else {
                    panic!("expected Value")
                }
            }

            #[test]
            fn restake_all() {
                // Arrange
                let mut ctx = new_context(ACCOUNT);
                ctx.predecessor_account_id = OWNER.to_string();
                testing_env!(ctx.clone());

                deploy_stake_contract(Some(to_valid_account_id(OWNER)), staking_public_key());

                let mut account_manager = account_manager();
                let mut staking_pool = staking_pool();
                assert!(!staking_pool.ops_stake_status().is_online());

                // register account
                ctx.account_balance = env::account_balance();
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx.clone());
                account_manager.storage_deposit(None, Some(true));
                account_manager.storage_deposit(None, Some(false));

                // stake storage deposit
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                } else {
                    panic!("expected value");
                }

                // unstake all
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Value(_) = staking_pool.ops_unstake(None) {
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                } else {
                    panic!("expected value");
                }

                let balance_before_restaking = staking_pool
                    .ops_stake_balance(to_valid_account_id(ACCOUNT))
                    .unwrap();

                // Act
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Value(balances) = staking_pool.ops_restake(None) {
                    // Assert
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                    assert_eq!(
                        logs,
                        vec![
                            "[INFO] [STAKE] near_amount=992000000000000000000000, stake_token_amount=992000000000000000000000",
                            "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
                            "[INFO] [FT_MINT] account: bob, amount: 992000000000000000000000",
                            "[INFO] [FT_BURN] account: bob, amount: 7936000000000000000000",
                            "[INFO] [FT_MINT] account: owner, amount: 7936000000000000000000",
                            "[WARN] [STATUS_OFFLINE] ",
                        ]
                    );

                    println!("{}", serde_json::to_string_pretty(&balances).unwrap());
                    assert!(balances.unstaked.is_none());
                    let staking_fee = staking_pool.ops_stake_fee()
                        * balance_before_restaking.unstaked.as_ref().unwrap().total;
                    assert_eq!(
                        balances.staked.as_ref().unwrap().near_value,
                        balance_before_restaking.unstaked.as_ref().unwrap().total - staking_fee
                    );
                } else {
                    panic!("expected Value")
                }
            }
        }
    }

    #[cfg(test)]
    mod tests_online {
        use super::*;

        #[cfg(test)]
        mod tests_stake {
            use super::*;

            #[test]
            fn with_attached_deposit() {
                // Arrange
                let mut ctx = new_context(OWNER);
                testing_env!(ctx.clone());

                deploy_stake_contract(Some(to_valid_account_id(OWNER)), staking_public_key());
                let contract_managed_total_balance = State::contract_managed_total_balance();

                let mut account_manager = account_manager();
                let mut staking_pool = staking_pool();

                // start staking
                ctx.predecessor_account_id = OWNER.to_string();
                testing_env!(ctx.clone());
                staking_pool.ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
                assert!(staking_pool.ops_stake_status().is_online());

                let ft_stake = ft_stake();

                // register account
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx.clone());
                account_manager.storage_deposit(None, Some(true));

                // assert that there was no net change to contract_managed_total_balance
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                assert_eq!(
                    contract_managed_total_balance,
                    State::contract_managed_total_balance()
                );

                assert_eq!(
                    account_manager
                        .storage_balance_of(to_valid_account_id(ACCOUNT))
                        .unwrap()
                        .available,
                    YoctoNear::ZERO
                );

                // Act
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
                    panic!("expected promise");
                }
                let logs = test_utils::get_logs();
                println!("{:#?}", logs);

                let receipts = deserialize_receipts();
                assert_eq!(receipts.len(), 2);
                {
                    let receipt = &receipts[0];
                    assert_eq!(receipt.receiver_id, env::current_account_id());
                    assert_eq!(receipt.actions.len(), 1);
                    match &receipt.actions[0] {
                        Action::Stake(action) => {
                            assert_eq!(
                                action.stake,
                                *staking_pool.ops_stake_pool_balances().total_staked
                            );

                            assert_eq!(
                                action.public_key,
                                "1".to_string()
                                    + staking_pool
                                        .ops_stake_public_key()
                                        .to_string()
                                        .split(":")
                                        .last()
                                        .unwrap()
                            );
                        }
                        _ => panic!("expected StakeAction"),
                    }
                }
                {
                    let receipt = &receipts[1];
                    assert_eq!(receipt.receiver_id, env::current_account_id());
                    assert_eq!(receipt.actions.len(), 1);
                    match &receipt.actions[0] {
                        Action::FunctionCall(action) => {
                            assert_eq!(action.method_name, "ops_stake_finalize");
                            let args: StakeActionCallbackArgs =
                                serde_json::from_str(&action.args).unwrap();
                            assert_eq!(args.account_id, ACCOUNT);
                            assert_eq!(action.deposit, 0);
                        }
                        _ => panic!("expected StakeAction"),
                    }
                }

                // Assert
                assert_eq!(logs, vec![
                    "[INFO] [STAKE] near_amount=1000000000000000000000000, stake_token_amount=1000000000000000000000000",
                    "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
                    "[INFO] [FT_MINT] account: bob, amount: 1000000000000000000000000",
                    "[INFO] [FT_BURN] account: bob, amount: 8000000000000000000000",
                    "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
                    "[INFO] [FT_MINT] account: owner, amount: 8000000000000000000000",
                ]);
                let staking_fee = staking_pool.ops_stake_fee() * YOCTO;
                assert_eq!(staking_pool.ops_stake_token_value(None), YOCTO.into());
                assert_eq!(
                    ft_stake.ft_balance_of(to_valid_account_id(ACCOUNT)),
                    (YOCTO - *staking_fee).into()
                );
                assert_eq!(
                    ft_stake.ft_balance_of(to_valid_account_id(OWNER)),
                    (*staking_fee).into()
                );
                let state = StakingPoolComponent::state();
                println!("{:#?}", *state);
                assert_eq!(State::total_staked_balance(), YOCTO.into());
                assert_eq!(state.treasury_balance, YoctoNear::ZERO);

                log_contract_managed_total_balance("after staking");

                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                assert_eq!(
                    contract_managed_total_balance + YOCTO,
                    State::contract_managed_total_balance()
                );
                let stake_pool_balances = staking_pool.ops_stake_pool_balances();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&stake_pool_balances).unwrap()
                );
                assert_eq!(stake_pool_balances.treasury_balance, YoctoNear::ZERO);
                assert_eq!(stake_pool_balances.total_staked, YOCTO.into());
                assert_eq!(stake_pool_balances.total_unstaked, YoctoNear::ZERO);
                assert_eq!(stake_pool_balances.unstaked_liquidity, YoctoNear::ZERO);
            }

            #[test]
            fn with_zero_attached_deposit_and_storage_available_balance() {
                // Arrange
                let mut ctx = new_context(OWNER);
                testing_env!(ctx.clone());

                deploy_stake_contract(Some(to_valid_account_id(OWNER)), staking_public_key());
                let contract_managed_total_balance = State::contract_managed_total_balance();

                let mut account_manager = account_manager();
                let mut staking_pool = staking_pool();
                assert!(!staking_pool.ops_stake_status().is_online());

                // start staking
                ctx.predecessor_account_id = OWNER.to_string();
                testing_env!(ctx.clone());
                staking_pool.ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
                assert!(staking_pool.ops_stake_status().is_online());

                let ft_stake = ft_stake();

                // register account
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx.clone());
                account_manager.storage_deposit(None, Some(true));
                account_manager.storage_deposit(None, Some(false));

                // assert that there was no net change to contract_managed_total_balance
                ctx.account_balance = env::account_balance() + YOCTO;
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                assert_eq!(
                    contract_managed_total_balance,
                    State::contract_managed_total_balance()
                );

                assert_eq!(
                    account_manager
                        .storage_balance_of(to_valid_account_id(ACCOUNT))
                        .unwrap()
                        .available,
                    YOCTO.into()
                );

                log_contract_managed_total_balance("before staking");
                assert_eq!(staking_pool.ops_stake_token_value(None), YOCTO.into());

                // Act
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Promise(_) = staking_pool.ops_stake() {
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);

                    let staking_fee = staking_pool.ops_stake_fee() * YOCTO;

                    let balances = staking_pool
                        .ops_stake_balance(to_valid_account_id(ACCOUNT))
                        .unwrap();
                    assert!(balances.unstaked.is_none());
                    match balances.staked.as_ref() {
                        Some(stake) => {
                            assert_eq!(stake.stake, (YOCTO - *staking_fee).into());
                            assert_eq!(stake.near_value, (YOCTO - *staking_fee).into());
                        }
                        None => panic!("expected staked balance"),
                    }

                    // Assert
                    assert_eq!(logs, vec![
                        "[INFO] [ACCOUNT_STORAGE_CHANGED] Withdrawal(YoctoNear(1000000000000000000000000))",
                        "[INFO] [STAKE] near_amount=1000000000000000000000000, stake_token_amount=1000000000000000000000000",
                        "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
                        "[INFO] [FT_MINT] account: bob, amount: 1000000000000000000000000",
                        "[INFO] [FT_BURN] account: bob, amount: 8000000000000000000000",
                        "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
                        "[INFO] [FT_MINT] account: owner, amount: 8000000000000000000000",
                    ]);
                    let staking_fee = staking_pool.ops_stake_fee() * YOCTO;
                    assert_eq!(staking_pool.ops_stake_token_value(None), YOCTO.into());
                    assert_eq!(
                        ft_stake.ft_balance_of(to_valid_account_id(ACCOUNT)),
                        (YOCTO - *staking_fee).into()
                    );
                    assert_eq!(
                        ft_stake.ft_balance_of(to_valid_account_id(OWNER)),
                        (*staking_fee).into()
                    );
                    let state = StakingPoolComponent::state();
                    println!("{:#?}", *state);
                    assert_eq!(State::total_staked_balance(), YOCTO.into());
                    assert_eq!(state.treasury_balance, YoctoNear::ZERO);

                    log_contract_managed_total_balance("after staking");

                    ctx.account_balance = env::account_balance();
                    ctx.attached_deposit = 0;
                    testing_env!(ctx.clone());
                    assert_eq!(
                        contract_managed_total_balance + YOCTO,
                        State::contract_managed_total_balance()
                    );
                    let stake_pool_balances = staking_pool.ops_stake_pool_balances();
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&stake_pool_balances).unwrap()
                    );
                    assert_eq!(stake_pool_balances.treasury_balance, YoctoNear::ZERO);
                    assert_eq!(stake_pool_balances.total_staked, YOCTO.into());
                    assert_eq!(stake_pool_balances.total_unstaked, YoctoNear::ZERO);
                    assert_eq!(stake_pool_balances.unstaked_liquidity, YoctoNear::ZERO);
                } else {
                    panic!("expected Promise")
                }
            }
        }

        #[cfg(test)]
        mod tests_unstake {
            use super::*;

            #[test]
            fn unstake_partial() {
                // Arrange
                let mut ctx = new_context(ACCOUNT);
                ctx.predecessor_account_id = OWNER.to_string();
                testing_env!(ctx.clone());

                deploy_stake_contract(Some(to_valid_account_id(OWNER)), staking_public_key());

                let mut account_manager = account_manager();
                let mut staking_pool = staking_pool();
                assert!(!staking_pool.ops_stake_status().is_online());

                // start staking
                ctx.predecessor_account_id = OWNER.to_string();
                testing_env!(ctx.clone());
                staking_pool.ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
                assert!(staking_pool.ops_stake_status().is_online());

                // register account
                ctx.account_balance = env::account_balance();
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx.clone());
                account_manager.storage_deposit(None, Some(true));
                account_manager.storage_deposit(None, Some(false));

                // stake storage deposit
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Promise(_) = staking_pool.ops_stake() {
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);

                    let pool_balances = staking_pool.ops_stake_pool_balances();
                    println!(
                        "pool_balances before unstaking: {}",
                        serde_json::to_string_pretty(&pool_balances).unwrap()
                    );
                } else {
                    panic!("expected promise")
                }

                let balances = staking_pool
                    .ops_stake_balance(to_valid_account_id(ACCOUNT))
                    .unwrap();
                let staked_balance = balances.staked.as_ref().unwrap().near_value;

                // Act
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Promise(_) =
                    staking_pool.ops_unstake(Some((*staked_balance / 4).into()))
                {
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                    assert_eq!(logs, vec![
                        "[INFO] [UNSTAKE] near_amount=248000000000000000000000, stake_token_amount=248000000000000000000000",
                        "[INFO] [FT_BURN] account: bob, amount: 248000000000000000000000",
                        "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(184)",
                    ]);

                    let balances_after_unstaking = staking_pool
                        .ops_stake_balance(to_valid_account_id(ACCOUNT))
                        .unwrap();
                    println!(
                        "account balances_after_unstaking: {}",
                        serde_json::to_string_pretty(&balances_after_unstaking).unwrap()
                    );
                    assert_eq!(
                        balances_after_unstaking.unstaked.as_ref().unwrap().total,
                        staked_balance / 4
                    );
                    assert_eq!(
                        balances_after_unstaking
                            .unstaked
                            .as_ref()
                            .unwrap()
                            .available,
                        YoctoNear::ZERO
                    );

                    let pool_balances = staking_pool.ops_stake_pool_balances();
                    println!(
                        "pool_balances: {}",
                        serde_json::to_string_pretty(&pool_balances).unwrap()
                    );
                    assert_eq!(
                        pool_balances.total_staked,
                        balances_after_unstaking.staked.as_ref().unwrap().near_value
                            + staking_pool
                                .ops_stake_balance(to_valid_account_id(OWNER))
                                .unwrap()
                                .staked
                                .unwrap()
                                .near_value
                    );
                    assert_eq!(
                        pool_balances.total_unstaked,
                        balances_after_unstaking.unstaked.as_ref().unwrap().total
                    );
                } else {
                    panic!("expected value")
                }

                let receipts = deserialize_receipts();
                assert_eq!(receipts.len(), 2);
                {
                    let receipt = &receipts[0];
                    assert_eq!(receipt.receiver_id, env::current_account_id());
                    assert_eq!(receipt.actions.len(), 1);
                    match &receipt.actions[0] {
                        Action::Stake(action) => {
                            assert_eq!(
                                action.stake,
                                *staking_pool.ops_stake_pool_balances().total_staked
                            );

                            assert_eq!(
                                action.public_key,
                                "1".to_string()
                                    + staking_pool
                                        .ops_stake_public_key()
                                        .to_string()
                                        .split(":")
                                        .last()
                                        .unwrap()
                            );
                        }
                        _ => panic!("expected StakeAction"),
                    }
                }
                {
                    let receipt = &receipts[1];
                    assert_eq!(receipt.receiver_id, env::current_account_id());
                    assert_eq!(receipt.actions.len(), 1);
                    match &receipt.actions[0] {
                        Action::FunctionCall(action) => {
                            assert_eq!(action.method_name, "ops_stake_finalize");
                            let args: StakeActionCallbackArgs =
                                serde_json::from_str(&action.args).unwrap();
                            assert_eq!(args.account_id, ACCOUNT);
                            assert_eq!(action.deposit, 0);
                        }
                        _ => panic!("expected StakeAction"),
                    }
                }
            }

            #[test]
            fn unstake_all() {
                // Arrange
                let mut ctx = new_context(ACCOUNT);
                ctx.predecessor_account_id = OWNER.to_string();
                testing_env!(ctx.clone());

                deploy_stake_contract(Some(to_valid_account_id(OWNER)), staking_public_key());

                let mut account_manager = account_manager();
                let mut staking_pool = staking_pool();

                // start staking
                ctx.predecessor_account_id = OWNER.to_string();
                testing_env!(ctx.clone());
                staking_pool.ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
                assert!(staking_pool.ops_stake_status().is_online());

                // register account
                ctx.account_balance = env::account_balance();
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx.clone());
                account_manager.storage_deposit(None, Some(true));
                account_manager.storage_deposit(None, Some(false));

                // stake storage deposit
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
                    panic!("expected value")
                }

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);

                let pool_balances = staking_pool.ops_stake_pool_balances();
                println!(
                    "pool_balances before unstaking: {}",
                    serde_json::to_string_pretty(&pool_balances).unwrap()
                );

                let balances = staking_pool
                    .ops_stake_balance(to_valid_account_id(ACCOUNT))
                    .unwrap();
                let staked_balance = balances.staked.as_ref().unwrap().near_value;

                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Value(_) = staking_pool.ops_unstake(None) {
                    panic!("expected Promise")
                }

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(logs, vec![
                    "[INFO] [UNSTAKE] near_amount=992000000000000000000000, stake_token_amount=992000000000000000000000",
                    "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(-104)",
                    "[INFO] [FT_BURN] account: bob, amount: 992000000000000000000000",
                    "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(184)",
                ]);

                let balances_after_unstaking = staking_pool
                    .ops_stake_balance(to_valid_account_id(ACCOUNT))
                    .unwrap();
                println!(
                    "account balances_after_unstaking: {}",
                    serde_json::to_string_pretty(&balances_after_unstaking).unwrap()
                );
                assert_eq!(
                    balances_after_unstaking.unstaked.as_ref().unwrap().total,
                    staked_balance
                );
                assert_eq!(
                    balances_after_unstaking
                        .unstaked
                        .as_ref()
                        .unwrap()
                        .available,
                    YoctoNear::ZERO
                );

                let pool_balances = staking_pool.ops_stake_pool_balances();
                println!(
                    "pool_balances: {}",
                    serde_json::to_string_pretty(&pool_balances).unwrap()
                );
                assert_eq!(
                    pool_balances.total_staked,
                    staking_pool
                        .ops_stake_balance(to_valid_account_id(OWNER))
                        .unwrap()
                        .staked
                        .unwrap()
                        .near_value
                );
                assert_eq!(
                    pool_balances.total_unstaked,
                    balances_after_unstaking.unstaked.as_ref().unwrap().total
                );

                let receipts = deserialize_receipts();
                assert_eq!(receipts.len(), 2);
                {
                    let receipt = &receipts[0];
                    assert_eq!(receipt.receiver_id, env::current_account_id());
                    assert_eq!(receipt.actions.len(), 1);
                    match &receipt.actions[0] {
                        Action::Stake(action) => {
                            assert_eq!(
                                action.stake,
                                *staking_pool.ops_stake_pool_balances().total_staked
                            );

                            assert_eq!(
                                action.public_key,
                                "1".to_string()
                                    + staking_pool
                                        .ops_stake_public_key()
                                        .to_string()
                                        .split(":")
                                        .last()
                                        .unwrap()
                            );
                        }
                        _ => panic!("expected StakeAction"),
                    }
                }
                {
                    let receipt = &receipts[1];
                    assert_eq!(receipt.receiver_id, env::current_account_id());
                    assert_eq!(receipt.actions.len(), 1);
                    match &receipt.actions[0] {
                        Action::FunctionCall(action) => {
                            assert_eq!(action.method_name, "ops_stake_finalize");
                            let args: StakeActionCallbackArgs =
                                serde_json::from_str(&action.args).unwrap();
                            assert_eq!(args.account_id, ACCOUNT);
                            assert_eq!(action.deposit, 0);
                        }
                        _ => panic!("expected StakeAction"),
                    }
                }
            }
        }

        #[cfg(test)]
        mod tests_withdraw {
            use super::*;

            #[test]
            fn withdraw_partial() {
                // Arrange
                let mut ctx = new_context(ACCOUNT);
                ctx.predecessor_account_id = OWNER.to_string();
                testing_env!(ctx.clone());

                deploy_stake_contract(Some(to_valid_account_id(OWNER)), staking_public_key());

                let mut account_manager = account_manager();
                let mut staking_pool = staking_pool();
                assert!(!staking_pool.ops_stake_status().is_online());

                // start staking
                ctx.predecessor_account_id = OWNER.to_string();
                testing_env!(ctx.clone());
                staking_pool.ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
                assert!(staking_pool.ops_stake_status().is_online());

                // register account
                ctx.account_balance = env::account_balance();
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx.clone());
                account_manager.storage_deposit(None, Some(true));
                account_manager.storage_deposit(None, Some(false));

                // stake storage deposit
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Promise(_) = staking_pool.ops_stake() {
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                } else {
                    panic!("expected Promise");
                }

                // unstake all
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Promise(_) = staking_pool.ops_unstake(None) {
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                } else {
                    panic!("expected Promise");
                }

                let balances_before_withdrawal = staking_pool
                    .ops_stake_balance(to_valid_account_id(ACCOUNT))
                    .unwrap();

                // Act
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                ctx.epoch_height = env::epoch_height() + 4;
                testing_env!(ctx.clone());
                let balances = staking_pool.ops_stake_withdraw(Some((1000).into()));

                // Assert
                assert!(test_utils::get_logs().is_empty());

                assert_eq!(
                    balances.unstaked.as_ref().unwrap().total,
                    balances_before_withdrawal.unstaked.as_ref().unwrap().total - 1000
                );

                let receipts = deserialize_receipts();
                assert_eq!(receipts.len(), 1);
                match &receipts[0].actions[0] {
                    Action::Transfer(action) => {
                        assert_eq!(action.deposit, 1000);
                    }
                    _ => panic!("expected transfer action"),
                }
            }

            #[test]
            fn withdraw_all() {
                // Arrange
                let mut ctx = new_context(ACCOUNT);
                ctx.predecessor_account_id = OWNER.to_string();
                testing_env!(ctx.clone());

                deploy_stake_contract(Some(to_valid_account_id(OWNER)), staking_public_key());

                let mut account_manager = account_manager();
                let mut staking_pool = staking_pool();
                assert!(!staking_pool.ops_stake_status().is_online());

                // start staking
                ctx.predecessor_account_id = OWNER.to_string();
                testing_env!(ctx.clone());
                staking_pool.ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
                assert!(staking_pool.ops_stake_status().is_online());

                // register account
                ctx.account_balance = env::account_balance();
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx.clone());
                account_manager.storage_deposit(None, Some(true));
                account_manager.storage_deposit(None, Some(false));

                // stake storage deposit
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Promise(_) = staking_pool.ops_stake() {
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                } else {
                    panic!("expected Promise");
                }

                // unstake all
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Promise(_) = staking_pool.ops_unstake(None) {
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                } else {
                    panic!("expected Promise");
                }

                let balances_before_withdrawal = staking_pool
                    .ops_stake_balance(to_valid_account_id(ACCOUNT))
                    .unwrap();

                // Act
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                ctx.epoch_height = env::epoch_height() + 4;
                testing_env!(ctx.clone());
                let balances = staking_pool.ops_stake_withdraw(None);

                // Assert
                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(
                    logs,
                    vec!["[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(-184)",]
                );

                assert!(balances.unstaked.is_none());

                let receipts = deserialize_receipts();
                assert_eq!(receipts.len(), 1);
                match &receipts[0].actions[0] {
                    Action::Transfer(action) => {
                        assert_eq!(
                            action.deposit,
                            *balances_before_withdrawal.unstaked.as_ref().unwrap().total
                        );
                    }
                    _ => panic!("expected transfer action"),
                }
            }
        }

        #[cfg(test)]
        mod tests_restake {
            use super::*;

            #[test]
            fn restake_partial() {
                // Arrange
                let mut ctx = new_context(ACCOUNT);
                ctx.predecessor_account_id = OWNER.to_string();
                testing_env!(ctx.clone());

                deploy_stake_contract(Some(to_valid_account_id(OWNER)), staking_public_key());

                let mut account_manager = account_manager();
                let mut staking_pool = staking_pool();
                assert!(!staking_pool.ops_stake_status().is_online());

                // start staking
                ctx.predecessor_account_id = OWNER.to_string();
                testing_env!(ctx.clone());
                staking_pool.ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
                assert!(staking_pool.ops_stake_status().is_online());

                // register account
                ctx.account_balance = env::account_balance();
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx.clone());
                account_manager.storage_deposit(None, Some(true));
                account_manager.storage_deposit(None, Some(false));

                // stake storage deposit
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Promise(_) = staking_pool.ops_stake() {
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                } else {
                    panic!("expected Promise");
                }

                // unstake all
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Promise(_) = staking_pool.ops_unstake(None) {
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                } else {
                    panic!("expected Promise");
                }

                let balances_before_restaking = staking_pool
                    .ops_stake_balance(to_valid_account_id(ACCOUNT))
                    .unwrap();

                // Act
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Value(_) = staking_pool.ops_restake(Some((1000).into())) {
                    panic!("expected Promise")
                }

                // Assert
                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(
                    logs,
                    vec![
                        "[INFO] [STAKE] near_amount=1000, stake_token_amount=1000",
                        "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
                        "[INFO] [FT_MINT] account: bob, amount: 1000",
                        "[INFO] [FT_BURN] account: bob, amount: 8",
                        "[INFO] [FT_MINT] account: owner, amount: 8",
                    ]
                );

                let balances = staking_pool
                    .ops_stake_balance(to_valid_account_id(ACCOUNT))
                    .unwrap();
                println!("{}", serde_json::to_string_pretty(&balances).unwrap());
                assert_eq!(
                    balances.unstaked.as_ref().unwrap().total,
                    balances_before_restaking.unstaked.as_ref().unwrap().total - 1000
                );
                let staking_fee = staking_pool.ops_stake_fee() * 1000;
                assert_eq!(
                    balances.staked.as_ref().unwrap().near_value,
                    (1000 - *staking_fee).into()
                );

                let receipts = deserialize_receipts();
                assert_eq!(receipts.len(), 2);
                {
                    let receipt = &receipts[0];
                    assert_eq!(receipt.receiver_id, env::current_account_id());
                    assert_eq!(receipt.actions.len(), 1);
                    match &receipt.actions[0] {
                        Action::Stake(action) => {
                            assert_eq!(
                                action.stake,
                                *staking_pool.ops_stake_pool_balances().total_staked
                            );

                            assert_eq!(
                                action.public_key,
                                "1".to_string()
                                    + staking_pool
                                        .ops_stake_public_key()
                                        .to_string()
                                        .split(":")
                                        .last()
                                        .unwrap()
                            );
                        }
                        _ => panic!("expected StakeAction"),
                    }
                }
                {
                    let receipt = &receipts[1];
                    assert_eq!(receipt.receiver_id, env::current_account_id());
                    assert_eq!(receipt.actions.len(), 1);
                    match &receipt.actions[0] {
                        Action::FunctionCall(action) => {
                            assert_eq!(action.method_name, "ops_stake_finalize");
                            let args: StakeActionCallbackArgs =
                                serde_json::from_str(&action.args).unwrap();
                            assert_eq!(args.account_id, ACCOUNT);
                            assert_eq!(action.deposit, 0);
                        }
                        _ => panic!("expected StakeAction"),
                    }
                }
            }

            #[test]
            fn restake_all() {
                // Arrange
                let mut ctx = new_context(ACCOUNT);
                ctx.predecessor_account_id = OWNER.to_string();
                testing_env!(ctx.clone());

                deploy_stake_contract(Some(to_valid_account_id(OWNER)), staking_public_key());

                let mut account_manager = account_manager();
                let mut staking_pool = staking_pool();
                assert!(!staking_pool.ops_stake_status().is_online());

                // start staking
                ctx.predecessor_account_id = OWNER.to_string();
                testing_env!(ctx.clone());
                staking_pool.ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
                assert!(staking_pool.ops_stake_status().is_online());

                // register account
                ctx.account_balance = env::account_balance();
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx.clone());
                account_manager.storage_deposit(None, Some(true));
                account_manager.storage_deposit(None, Some(false));

                // stake storage deposit
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Promise(_) = staking_pool.ops_stake() {
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                } else {
                    panic!("expected Promise");
                }

                // unstake all
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Promise(_) = staking_pool.ops_unstake(None) {
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                } else {
                    panic!("expected Promise");
                }

                let balance_before_restaking = staking_pool
                    .ops_stake_balance(to_valid_account_id(ACCOUNT))
                    .unwrap();

                // Act
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Value(_) = staking_pool.ops_restake(None) {
                    panic!("expected Promise")
                }

                // Assert
                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(
                    logs,
                    vec![
                        "[INFO] [STAKE] near_amount=992000000000000000000000, stake_token_amount=992000000000000000000000",
                        "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
                        "[INFO] [FT_MINT] account: bob, amount: 992000000000000000000000",
                        "[INFO] [FT_BURN] account: bob, amount: 7936000000000000000000",
                        "[INFO] [FT_MINT] account: owner, amount: 7936000000000000000000",
                    ]
                );

                let balances = staking_pool
                    .ops_stake_balance(to_valid_account_id(ACCOUNT))
                    .unwrap();
                println!("{}", serde_json::to_string_pretty(&balances).unwrap());
                assert!(balances.unstaked.is_none());
                let staking_fee = staking_pool.ops_stake_fee()
                    * balance_before_restaking.unstaked.as_ref().unwrap().total;
                assert_eq!(
                    balances.staked.as_ref().unwrap().near_value,
                    balance_before_restaking.unstaked.as_ref().unwrap().total - staking_fee
                );

                let receipts = deserialize_receipts();
                assert_eq!(receipts.len(), 2);
                {
                    let receipt = &receipts[0];
                    assert_eq!(receipt.receiver_id, env::current_account_id());
                    assert_eq!(receipt.actions.len(), 1);
                    match &receipt.actions[0] {
                        Action::Stake(action) => {
                            assert_eq!(
                                action.stake,
                                *staking_pool.ops_stake_pool_balances().total_staked
                            );

                            assert_eq!(
                                action.public_key,
                                "1".to_string()
                                    + staking_pool
                                        .ops_stake_public_key()
                                        .to_string()
                                        .split(":")
                                        .last()
                                        .unwrap()
                            );
                        }
                        _ => panic!("expected StakeAction"),
                    }
                }
                {
                    let receipt = &receipts[1];
                    assert_eq!(receipt.receiver_id, env::current_account_id());
                    assert_eq!(receipt.actions.len(), 1);
                    match &receipt.actions[0] {
                        Action::FunctionCall(action) => {
                            assert_eq!(action.method_name, "ops_stake_finalize");
                            let args: StakeActionCallbackArgs =
                                serde_json::from_str(&action.args).unwrap();
                            assert_eq!(args.account_id, ACCOUNT);
                            assert_eq!(action.deposit, 0);
                        }
                        _ => panic!("expected StakeAction"),
                    }
                }
            }
        }
    }

    #[cfg(test)]
    mod tests_callbacks {
        use super::*;

        #[cfg(test)]
        mod tests_stake_callback {
            use super::*;

            mod tests_online {
                use super::*;

                #[test]
                fn promise_success_with_zero_earnings() {
                    // Arrange
                    let mut ctx = new_context(ACCOUNT);
                    ctx.predecessor_account_id = OWNER.to_string();
                    testing_env!(ctx.clone());

                    deploy_stake_contract(Some(to_valid_account_id(OWNER)), staking_public_key());

                    let mut account_manager = account_manager();
                    let mut staking_pool = staking_pool();

                    // start staking
                    ctx.predecessor_account_id = OWNER.to_string();
                    testing_env!(ctx.clone());
                    staking_pool
                        .ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
                    assert!(staking_pool.ops_stake_status().is_online());

                    // register account
                    ctx.predecessor_account_id = ACCOUNT.to_string();
                    ctx.attached_deposit = YOCTO;
                    testing_env!(ctx.clone());
                    account_manager.storage_deposit(None, Some(true));

                    // stake
                    ctx.account_balance = env::account_balance();
                    ctx.attached_deposit = YOCTO;
                    testing_env!(ctx.clone());
                    staking_pool.ops_stake();

                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);

                    let receipts = deserialize_receipts();
                    match &receipts[1].actions[0] {
                        Action::FunctionCall(action) => {
                            let args: StakeActionCallbackArgs =
                                serde_json::from_str(&action.args).unwrap();

                            ctx.predecessor_account_id = (&receipts[1]).receiver_id.to_string();
                            ctx.account_balance = env::account_balance()
                                - *staking_pool.ops_stake_pool_balances().total_staked;
                            ctx.account_locked_balance =
                                *staking_pool.ops_stake_pool_balances().total_staked;
                            testing_env_with_promise_result_success(ctx.clone());
                            let state_before_callback =
                                StakingPoolComponent::state_with_updated_earnings();
                            let balances = staking_pool.ops_stake_finalize(args.account_id.clone());
                            println!("{}", serde_json::to_string_pretty(&balances).unwrap());
                            assert_eq!(
                                balances,
                                staking_pool
                                    .ops_stake_balance(to_valid_account_id(&args.account_id))
                                    .unwrap()
                            );
                            let state_after_callback =
                                StakingPoolComponent::state_with_updated_earnings();
                            assert_eq!(
                                state_before_callback.last_contract_managed_total_balance,
                                state_after_callback.last_contract_managed_total_balance
                            );
                        }
                        _ => panic!("expected function call"),
                    }
                }

                #[test]
                fn promise_success_with_some_earnings() {
                    // Arrange
                    let mut ctx = new_context(ACCOUNT);
                    ctx.predecessor_account_id = OWNER.to_string();
                    testing_env!(ctx.clone());

                    deploy_stake_contract(Some(to_valid_account_id(OWNER)), staking_public_key());

                    let mut account_manager = account_manager();
                    let mut staking_pool = staking_pool();

                    // start staking
                    ctx.predecessor_account_id = OWNER.to_string();
                    testing_env!(ctx.clone());
                    staking_pool
                        .ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
                    assert!(staking_pool.ops_stake_status().is_online());

                    // register account
                    ctx.predecessor_account_id = ACCOUNT.to_string();
                    ctx.attached_deposit = YOCTO;
                    testing_env!(ctx.clone());
                    account_manager.storage_deposit(None, Some(true));

                    // stake
                    ctx.account_balance = env::account_balance();
                    ctx.attached_deposit = YOCTO;
                    testing_env!(ctx.clone());
                    staking_pool.ops_stake();

                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);

                    let receipts = deserialize_receipts();
                    match &receipts[1].actions[0] {
                        Action::FunctionCall(action) => {
                            let args: StakeActionCallbackArgs =
                                serde_json::from_str(&action.args).unwrap();

                            let state_before_callback = {
                                ctx.predecessor_account_id = (&receipts[1]).receiver_id.to_string();
                                ctx.account_balance = env::account_balance()
                                    - *staking_pool.ops_stake_pool_balances().total_staked;
                                ctx.account_locked_balance =
                                    *staking_pool.ops_stake_pool_balances().total_staked;
                                ctx.attached_deposit = 0;
                                testing_env!(ctx.clone());
                                StakingPoolComponent::state_with_updated_earnings()
                            };

                            let earnings = 1000;
                            ctx.predecessor_account_id = (&receipts[1]).receiver_id.to_string();
                            ctx.account_balance = env::account_balance();
                            ctx.account_locked_balance = env::account_locked_balance() + earnings;
                            ctx.attached_deposit = 0;
                            testing_env_with_promise_result_success(ctx.clone());

                            let balances = staking_pool.ops_stake_finalize(args.account_id.clone());
                            println!("{}", serde_json::to_string_pretty(&balances).unwrap());
                            assert_eq!(
                                balances,
                                staking_pool
                                    .ops_stake_balance(to_valid_account_id(&args.account_id))
                                    .unwrap()
                            );
                            assert!(
                                *balances.staked.as_ref().unwrap().near_value
                                    > *balances.staked.as_ref().unwrap().stake
                            );
                            assert_eq!(
                                *balances.staked.as_ref().unwrap().near_value,
                                992000000000000000000992
                            );

                            let state_after_callback =
                                StakingPoolComponent::state_with_updated_earnings();
                            assert_eq!(
                                state_before_callback.last_contract_managed_total_balance
                                    + earnings,
                                state_after_callback.last_contract_managed_total_balance
                            );
                        }
                        _ => panic!("expected function call"),
                    }
                }

                #[test]
                fn promise_failure() {
                    // Arrange
                    let mut ctx = new_context(ACCOUNT);
                    ctx.predecessor_account_id = OWNER.to_string();
                    testing_env!(ctx.clone());

                    deploy_stake_contract(Some(to_valid_account_id(OWNER)), staking_public_key());

                    let mut account_manager = account_manager();
                    let mut staking_pool = staking_pool();

                    // start staking
                    ctx.predecessor_account_id = OWNER.to_string();
                    testing_env!(ctx.clone());
                    staking_pool
                        .ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
                    assert!(staking_pool.ops_stake_status().is_online());

                    // register account
                    ctx.predecessor_account_id = ACCOUNT.to_string();
                    ctx.attached_deposit = YOCTO;
                    testing_env!(ctx.clone());
                    account_manager.storage_deposit(None, Some(true));

                    // stake
                    ctx.account_balance = env::account_balance();
                    ctx.attached_deposit = YOCTO;
                    testing_env!(ctx.clone());
                    staking_pool.ops_stake();

                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);

                    let receipts = deserialize_receipts();
                    match &receipts[1].actions[0] {
                        Action::FunctionCall(action) => {
                            let args: StakeActionCallbackArgs =
                                serde_json::from_str(&action.args).unwrap();

                            ctx.predecessor_account_id = (&receipts[1]).receiver_id.to_string();
                            ctx.account_balance = env::account_balance()
                                - *staking_pool.ops_stake_pool_balances().total_staked;
                            ctx.account_locked_balance =
                                *staking_pool.ops_stake_pool_balances().total_staked;
                            testing_env_with_promise_result_failure(ctx.clone());
                            let state_before_callback =
                                StakingPoolComponent::state_with_updated_earnings();
                            let balances = staking_pool.ops_stake_finalize(args.account_id.clone());
                            println!("{}", serde_json::to_string_pretty(&balances).unwrap());
                            assert_eq!(
                                balances,
                                staking_pool
                                    .ops_stake_balance(to_valid_account_id(&args.account_id))
                                    .unwrap()
                            );
                            let state_after_callback =
                                StakingPoolComponent::state_with_updated_earnings();
                            assert_eq!(
                                state_before_callback.last_contract_managed_total_balance,
                                state_after_callback.last_contract_managed_total_balance
                            );

                            let receipts = deserialize_receipts();
                            assert_eq!(receipts.len(), 2);
                            {
                                let receipt = &receipts[0];
                                assert_eq!(receipt.receiver_id, env::current_account_id());
                                assert_eq!(receipt.actions.len(), 1);
                                match &receipt.actions[0] {
                                    Action::Stake(action) => {
                                        assert_eq!(action.stake, 0);

                                        assert_eq!(
                                            action.public_key,
                                            "1".to_string()
                                                + staking_pool
                                                    .ops_stake_public_key()
                                                    .to_string()
                                                    .split(":")
                                                    .last()
                                                    .unwrap()
                                        );
                                    }
                                    _ => panic!("expected StakeAction"),
                                }
                            }
                            {
                                let receipt = &receipts[1];
                                assert_eq!(receipt.receiver_id, env::current_account_id());
                                assert_eq!(receipt.actions.len(), 1);
                                match &receipt.actions[0] {
                                    Action::FunctionCall(action) => {
                                        assert_eq!(action.method_name, "ops_stake_stop_finalize");
                                        assert!(action.args.is_empty());
                                        assert_eq!(action.deposit, 0);
                                    }
                                    _ => panic!("expected StakeAction"),
                                }
                            }
                        }
                        _ => panic!("expected function call"),
                    }
                }
            }
        }
    }

    #[cfg(test)]
    mod tests_operator_commands {
        use super::*;

        #[cfg(test)]
        mod tests_start_staking {
            use super::*;

            #[test]
            fn initial_startup_with_zero_staked() {
                // Arrange
                let mut ctx = new_context(OWNER);
                testing_env!(ctx.clone());

                deploy_stake_contract(Some(to_valid_account_id(OWNER)), staking_public_key());

                let mut staking_pool = staking_pool();
                assert!(!staking_pool.ops_stake_status().is_online());

                // start staking
                ctx.predecessor_account_id = OWNER.to_string();
                testing_env!(ctx.clone());
                staking_pool.ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
                assert!(staking_pool.ops_stake_status().is_online());

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(logs, vec!["[INFO] [STATUS_ONLINE] ",]);

                let pool_balances = staking_pool.ops_stake_pool_balances();
                println!("{}", serde_json::to_string_pretty(&pool_balances).unwrap());
                assert_eq!(pool_balances.total_staked, YoctoNear::ZERO);
                assert_eq!(pool_balances.total_unstaked, YoctoNear::ZERO);
                assert_eq!(pool_balances.unstaked_liquidity, YoctoNear::ZERO);
                assert_eq!(pool_balances.treasury_balance, YoctoNear::ZERO);

                assert!(deserialize_receipts().is_empty());
            }

            #[test]
            fn startup_with_non_zero_staked_balance() {
                // Arrange
                let mut ctx = new_context(OWNER);
                testing_env!(ctx.clone());

                deploy_stake_contract(Some(to_valid_account_id(OWNER)), staking_public_key());
                let contract_managed_total_balance = State::contract_managed_total_balance();

                let mut account_manager = account_manager();
                let mut staking_pool = staking_pool();

                // register account
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx.clone());
                account_manager.storage_deposit(None, Some(true));

                // assert that there was no net change to contract_managed_total_balance
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                assert_eq!(
                    contract_managed_total_balance,
                    State::contract_managed_total_balance()
                );

                assert_eq!(
                    account_manager
                        .storage_balance_of(to_valid_account_id(ACCOUNT))
                        .unwrap()
                        .available,
                    YoctoNear::ZERO
                );

                // stake
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Promise(_) = staking_pool.ops_stake() {
                    panic!("expected Value");
                }
                let logs = test_utils::get_logs();
                println!("{:#?}", logs);

                // Act - start staking
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = 0;
                ctx.predecessor_account_id = OWNER.to_string();
                testing_env!(ctx.clone());
                staking_pool.ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);

                // Assert
                assert!(staking_pool.ops_stake_status().is_online());

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(logs, vec!["[INFO] [STATUS_ONLINE] ",]);

                let receipts = deserialize_receipts();
                assert_eq!(receipts.len(), 2);
                {
                    let receipt = &receipts[0];
                    assert_eq!(receipt.receiver_id, env::current_account_id());
                    assert_eq!(receipt.actions.len(), 1);
                    match &receipt.actions[0] {
                        Action::Stake(action) => {
                            assert_eq!(
                                action.stake,
                                *staking_pool.ops_stake_pool_balances().total_staked
                            );

                            assert_eq!(
                                action.public_key,
                                "1".to_string()
                                    + staking_pool
                                        .ops_stake_public_key()
                                        .to_string()
                                        .split(":")
                                        .last()
                                        .unwrap()
                            );
                        }
                        _ => panic!("expected StakeAction"),
                    }
                }
                {
                    let receipt = &receipts[1];
                    assert_eq!(receipt.receiver_id, env::current_account_id());
                    assert_eq!(receipt.actions.len(), 1);
                    match &receipt.actions[0] {
                        Action::FunctionCall(action) => {
                            assert_eq!(action.method_name, "ops_stake_start_finalize");
                            assert!(action.args.is_empty());
                            assert_eq!(action.deposit, 0);
                        }
                        _ => panic!("expected StakeAction"),
                    }
                }
            }

            #[test]
            fn start_while_already_started() {
                // Arrange
                let mut ctx = new_context(OWNER);
                testing_env!(ctx.clone());

                deploy_stake_contract(Some(to_valid_account_id(OWNER)), staking_public_key());

                let mut staking_pool = staking_pool();
                assert!(!staking_pool.ops_stake_status().is_online());

                // stake
                ctx.predecessor_account_id = OWNER.to_string();
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx.clone());
                if let PromiseOrValue::Promise(_) = staking_pool.ops_stake() {
                    panic!("expected Value");
                }
                let logs = test_utils::get_logs();
                println!("{:#?}", logs);

                // Act - start staking 2x
                for i in 0..2 {
                    ctx.account_balance = env::account_balance();
                    ctx.attached_deposit = 0;
                    ctx.predecessor_account_id = OWNER.to_string();
                    testing_env!(ctx.clone());
                    staking_pool
                        .ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);

                    let receipts = deserialize_receipts();
                    if i == 1 {
                        assert!(logs.is_empty());
                        assert!(receipts.is_empty());
                    }
                }

                assert!(staking_pool.ops_stake_status().is_online());
            }
        }

        #[cfg(test)]
        mod tests_stop_staking {
            use super::*;

            #[test]
            fn start_then_stop_with_zero_staked_balance() {
                // Arrange
                let mut ctx = new_context(OWNER);
                testing_env!(ctx.clone());

                deploy_stake_contract(Some(to_valid_account_id(OWNER)), staking_public_key());

                let mut staking_pool = staking_pool();
                assert!(!staking_pool.ops_stake_status().is_online());

                // start staking
                ctx.predecessor_account_id = OWNER.to_string();
                testing_env!(ctx.clone());
                staking_pool.ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
                assert!(staking_pool.ops_stake_status().is_online());

                // Act - stop staking
                ctx.predecessor_account_id = OWNER.to_string();
                testing_env!(ctx.clone());
                staking_pool.ops_stake_operator_command(StakingPoolOperatorCommand::StopStaking);
                assert!(!staking_pool.ops_stake_status().is_online());

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(logs, vec!["[WARN] [STATUS_OFFLINE] Stopped",]);

                assert!(deserialize_receipts().is_empty());
            }

            #[test]
            fn start_then_stop_with_nonzero_staked_balance_and_nonzero_locked_balance() {
                // Arrange
                let mut ctx = new_context(OWNER);
                testing_env!(ctx.clone());

                deploy_stake_contract(Some(to_valid_account_id(OWNER)), staking_public_key());

                let mut staking_pool = staking_pool();
                assert!(!staking_pool.ops_stake_status().is_online());

                // start staking
                ctx.predecessor_account_id = OWNER.to_string();
                testing_env!(ctx.clone());
                staking_pool.ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
                assert!(staking_pool.ops_stake_status().is_online());

                // stake
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx.clone());
                staking_pool.ops_stake();
                let logs = test_utils::get_logs();
                println!("{:#?}", logs);

                ctx.predecessor_account_id = OWNER.to_string();
                ctx.account_balance = env::account_balance();
                ctx.account_locked_balance = *staking_pool.ops_stake_pool_balances().total_staked;
                testing_env!(ctx.clone());
                staking_pool.ops_stake_operator_command(StakingPoolOperatorCommand::StopStaking);
                assert!(!staking_pool.ops_stake_status().is_online());

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(logs, vec!["[WARN] [STATUS_OFFLINE] Stopped",]);

                let receipts = deserialize_receipts();
                assert_eq!(receipts.len(), 2);
                {
                    let receipt = &receipts[0];
                    assert_eq!(receipt.receiver_id, env::current_account_id());
                    assert_eq!(receipt.actions.len(), 1);
                    match &receipt.actions[0] {
                        Action::Stake(action) => {
                            assert_eq!(action.stake, 0);

                            assert_eq!(
                                action.public_key,
                                "1".to_string()
                                    + staking_pool
                                        .ops_stake_public_key()
                                        .to_string()
                                        .split(":")
                                        .last()
                                        .unwrap()
                            );
                        }
                        _ => panic!("expected StakeAction"),
                    }
                }
                {
                    let receipt = &receipts[1];
                    assert_eq!(receipt.receiver_id, env::current_account_id());
                    assert_eq!(receipt.actions.len(), 1);
                    match &receipt.actions[0] {
                        Action::FunctionCall(action) => {
                            assert_eq!(action.method_name, "ops_stake_stop_finalize");
                            assert!(action.args.is_empty());
                            assert_eq!(action.deposit, 0);
                        }
                        _ => panic!("expected StakeAction"),
                    }
                }
            }

            #[test]
            fn start_then_stop_with_nonzero_staked_balance_and_zero_locked_balance() {
                // Arrange
                let mut ctx = new_context(OWNER);
                testing_env!(ctx.clone());

                deploy_stake_contract(Some(to_valid_account_id(OWNER)), staking_public_key());

                let mut staking_pool = staking_pool();
                assert!(!staking_pool.ops_stake_status().is_online());

                // start staking
                ctx.predecessor_account_id = OWNER.to_string();
                testing_env!(ctx.clone());
                staking_pool.ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
                assert!(staking_pool.ops_stake_status().is_online());

                // stake
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx.clone());
                staking_pool.ops_stake();
                let logs = test_utils::get_logs();
                println!("{:#?}", logs);

                // Act
                ctx.predecessor_account_id = OWNER.to_string();
                ctx.account_balance = env::account_balance();
                ctx.account_locked_balance = 0;
                testing_env!(ctx.clone());
                staking_pool.ops_stake_operator_command(StakingPoolOperatorCommand::StopStaking);

                // Assert
                assert!(!staking_pool.ops_stake_status().is_online());

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(logs, vec!["[WARN] [STATUS_OFFLINE] Stopped",]);

                assert!(deserialize_receipts().is_empty());
            }

            #[test]
            fn already_stopped_with_nonzero_locked_balance() {
                // Arrange
                let mut ctx = new_context(OWNER);
                testing_env!(ctx.clone());

                deploy_stake_contract(Some(to_valid_account_id(OWNER)), staking_public_key());

                let mut staking_pool = staking_pool();
                assert!(!staking_pool.ops_stake_status().is_online());

                // start staking
                ctx.predecessor_account_id = OWNER.to_string();
                testing_env!(ctx.clone());
                staking_pool.ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
                assert!(staking_pool.ops_stake_status().is_online());

                // stake
                ctx.account_balance = env::account_balance();
                ctx.attached_deposit = YOCTO;
                testing_env!(ctx.clone());
                staking_pool.ops_stake();
                let logs = test_utils::get_logs();
                println!("{:#?}", logs);

                // stop staking
                ctx.predecessor_account_id = OWNER.to_string();
                ctx.account_balance = env::account_balance();
                ctx.account_locked_balance = *staking_pool.ops_stake_pool_balances().total_staked;
                testing_env!(ctx.clone());
                staking_pool.ops_stake_operator_command(StakingPoolOperatorCommand::StopStaking);
                assert!(!staking_pool.ops_stake_status().is_online());
                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(deserialize_receipts().len(), 2);

                // Act
                ctx.predecessor_account_id = OWNER.to_string();
                ctx.account_balance = env::account_balance();
                ctx.account_locked_balance = *staking_pool.ops_stake_pool_balances().total_staked;
                testing_env!(ctx.clone());
                staking_pool.ops_stake_operator_command(StakingPoolOperatorCommand::StopStaking);

                // Assert
                assert!(!staking_pool.ops_stake_status().is_online());
                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(logs, vec!["[WARN] [STATUS_OFFLINE] Stopped",]);

                let receipts = deserialize_receipts();
                assert_eq!(receipts.len(), 2);
                {
                    let receipt = &receipts[0];
                    assert_eq!(receipt.receiver_id, env::current_account_id());
                    assert_eq!(receipt.actions.len(), 1);
                    match &receipt.actions[0] {
                        Action::Stake(action) => {
                            assert_eq!(action.stake, 0);

                            assert_eq!(
                                action.public_key,
                                "1".to_string()
                                    + staking_pool
                                        .ops_stake_public_key()
                                        .to_string()
                                        .split(":")
                                        .last()
                                        .unwrap()
                            );
                        }
                        _ => panic!("expected StakeAction"),
                    }
                }
                {
                    let receipt = &receipts[1];
                    assert_eq!(receipt.receiver_id, env::current_account_id());
                    assert_eq!(receipt.actions.len(), 1);
                    match &receipt.actions[0] {
                        Action::FunctionCall(action) => {
                            assert_eq!(action.method_name, "ops_stake_stop_finalize");
                            assert!(action.args.is_empty());
                            assert_eq!(action.deposit, 0);
                        }
                        _ => panic!("expected StakeAction"),
                    }
                }
            }
        }
    }

    #[cfg(test)]
    mod tests_treasury {
        use super::*;

        #[cfg(test)]
        mod tests_online {
            use super::*;

            #[cfg(test)]
            mod tests_deposit {
                use super::*;

                #[test]
                fn nonzero_attached_deposit() {
                    // Arrange
                    let mut ctx = new_context(OWNER);
                    testing_env!(ctx.clone());

                    deploy_stake_contract(Some(to_valid_account_id(OWNER)), staking_public_key());

                    let mut staking_pool = staking_pool();

                    // start staking
                    ctx.predecessor_account_id = OWNER.to_string();
                    testing_env!(ctx.clone());
                    staking_pool
                        .ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
                    assert!(staking_pool.ops_stake_status().is_online());

                    let ft_stake = ft_stake();
                    assert_eq!(
                        ft_stake.ft_balance_of(to_valid_account_id(&env::current_account_id())),
                        TokenAmount::ZERO
                    );
                    let state = StakingPoolComponent::state();
                    assert_eq!(state.treasury_balance, YoctoNear::ZERO);

                    // Act
                    {
                        ctx.predecessor_account_id = ACCOUNT.to_string();
                        ctx.attached_deposit = YOCTO;
                        ctx.account_balance = env::account_balance();
                        testing_env!(ctx.clone());
                        if let PromiseOrValue::Value(_) = staking_pool.ops_stake_treasury_deposit()
                        {
                            panic!("expected Promise")
                        }
                    }

                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                    assert_eq!(logs, vec![
                        "[INFO] [STAKE] near_amount=1000000000000000000000000, stake_token_amount=1000000000000000000000000",
                        "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
                        "[INFO] [FT_MINT] account: contract.near, amount: 1000000000000000000000000",
                    ]);

                    assert_eq!(
                        ft_stake.ft_balance_of(to_valid_account_id(&env::current_account_id())),
                        YOCTO.into()
                    );

                    let state = StakingPoolComponent::state();
                    assert_eq!(state.treasury_balance, YOCTO.into());

                    let receipts = deserialize_receipts();
                    assert_eq!(receipts.len(), 2);
                    {
                        let receipt = &receipts[0];
                        assert_eq!(receipt.receiver_id, env::current_account_id());
                        assert_eq!(receipt.actions.len(), 1);
                        match &receipt.actions[0] {
                            Action::Stake(action) => {
                                assert_eq!(
                                    action.stake,
                                    *staking_pool.ops_stake_pool_balances().total_staked
                                );

                                assert_eq!(
                                    action.public_key,
                                    "1".to_string()
                                        + staking_pool
                                            .ops_stake_public_key()
                                            .to_string()
                                            .split(":")
                                            .last()
                                            .unwrap()
                                );
                            }
                            _ => panic!("expected StakeAction"),
                        }
                    }
                    {
                        let receipt = &receipts[1];
                        assert_eq!(receipt.receiver_id, env::current_account_id());
                        assert_eq!(receipt.actions.len(), 1);
                        match &receipt.actions[0] {
                            Action::FunctionCall(action) => {
                                assert_eq!(action.method_name, "ops_stake_finalize");
                                let args: StakeActionCallbackArgs =
                                    serde_json::from_str(&action.args).unwrap();
                                assert_eq!(args.account_id, env::current_account_id());
                                assert_eq!(action.deposit, 0);
                            }
                            _ => panic!("expected StakeAction"),
                        }
                    }

                    // Act - deposit again
                    {
                        ctx.predecessor_account_id = ACCOUNT.to_string();
                        ctx.attached_deposit = YOCTO;
                        ctx.account_balance = env::account_balance();
                        testing_env!(ctx.clone());
                        if let PromiseOrValue::Value(_) = staking_pool.ops_stake_treasury_deposit()
                        {
                            panic!("expected Promise")
                        }
                    }

                    // Assert
                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);
                    assert_eq!(logs, vec![
                        "[INFO] [STAKE] near_amount=1000000000000000000000000, stake_token_amount=1000000000000000000000000",
                        "[INFO] [FT_MINT] account: contract.near, amount: 1000000000000000000000000",
                    ]);

                    assert_eq!(
                        ft_stake.ft_balance_of(to_valid_account_id(&env::current_account_id())),
                        (2 * YOCTO).into()
                    );

                    let state = StakingPoolComponent::state();
                    assert_eq!(state.treasury_balance, (2 * YOCTO).into());

                    assert_eq!(
                        *staking_pool.ops_stake_pool_balances().total_staked,
                        2 * YOCTO
                    );

                    let receipts = deserialize_receipts();
                    assert_eq!(receipts.len(), 2);
                    {
                        let receipt = &receipts[0];
                        assert_eq!(receipt.receiver_id, env::current_account_id());
                        assert_eq!(receipt.actions.len(), 1);
                        match &receipt.actions[0] {
                            Action::Stake(action) => {
                                assert_eq!(
                                    action.stake,
                                    *staking_pool.ops_stake_pool_balances().total_staked
                                );

                                assert_eq!(
                                    action.public_key,
                                    "1".to_string()
                                        + staking_pool
                                            .ops_stake_public_key()
                                            .to_string()
                                            .split(":")
                                            .last()
                                            .unwrap()
                                );
                            }
                            _ => panic!("expected StakeAction"),
                        }
                    }
                    {
                        let receipt = &receipts[1];
                        assert_eq!(receipt.receiver_id, env::current_account_id());
                        assert_eq!(receipt.actions.len(), 1);
                        match &receipt.actions[0] {
                            Action::FunctionCall(action) => {
                                assert_eq!(action.method_name, "ops_stake_finalize");
                                let args: StakeActionCallbackArgs =
                                    serde_json::from_str(&action.args).unwrap();
                                assert_eq!(args.account_id, env::current_account_id());
                                assert_eq!(action.deposit, 0);
                            }
                            _ => panic!("expected StakeAction"),
                        }
                    }
                }
            }

            #[cfg(test)]
            mod tests_distribution {
                use super::*;

                #[test]
                fn nonzero_attached_deposit() {
                    // Arrange
                    let mut ctx = new_context(OWNER);
                    testing_env!(ctx.clone());

                    deploy_stake_contract(Some(to_valid_account_id(OWNER)), staking_public_key());

                    let mut staking_pool = staking_pool();

                    // start staking
                    ctx.predecessor_account_id = OWNER.to_string();
                    testing_env!(ctx.clone());
                    staking_pool
                        .ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
                    assert!(staking_pool.ops_stake_status().is_online());

                    let ft_stake = ft_stake();
                    assert_eq!(
                        ft_stake.ft_balance_of(to_valid_account_id(&env::current_account_id())),
                        TokenAmount::ZERO
                    );

                    // Act
                    ctx.predecessor_account_id = ACCOUNT.to_string();
                    ctx.attached_deposit = YOCTO;
                    ctx.account_balance = env::account_balance();
                    testing_env!(ctx.clone());
                    staking_pool.ops_stake_treasury_distribution();

                    let logs = test_utils::get_logs();
                    println!("{:#?}", logs);

                    assert_eq!(
                        ft_stake.ft_balance_of(to_valid_account_id(&env::current_account_id())),
                        TokenAmount::ZERO
                    );

                    let receipts = deserialize_receipts();
                    assert_eq!(receipts.len(), 2);
                    {
                        let receipt = &receipts[0];
                        assert_eq!(receipt.receiver_id, env::current_account_id());
                        assert_eq!(receipt.actions.len(), 1);
                        match &receipt.actions[0] {
                            Action::Stake(action) => {
                                assert_eq!(
                                    action.stake,
                                    *staking_pool.ops_stake_pool_balances().total_staked
                                );

                                assert_eq!(
                                    action.public_key,
                                    "1".to_string()
                                        + staking_pool
                                            .ops_stake_public_key()
                                            .to_string()
                                            .split(":")
                                            .last()
                                            .unwrap()
                                );
                            }
                            _ => panic!("expected StakeAction"),
                        }
                    }
                    {
                        let receipt = &receipts[1];
                        assert_eq!(receipt.receiver_id, env::current_account_id());
                        assert_eq!(receipt.actions.len(), 1);
                        match &receipt.actions[0] {
                            Action::FunctionCall(action) => {
                                assert_eq!(action.method_name, "ops_stake_finalize");
                                let args: StakeActionCallbackArgs =
                                    serde_json::from_str(&action.args).unwrap();
                                assert_eq!(args.account_id, env::current_account_id());
                                assert_eq!(action.deposit, 0);
                            }
                            _ => panic!("expected StakeAction"),
                        }
                    }
                }
            }
        }
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::*;
//     use oysterpack_smart_account_management::{
//         components::account_management::AccountManagementComponentConfig, *,
//     };
//     use oysterpack_smart_fungible_token::components::fungible_token::FungibleTokenConfig;
//     use oysterpack_smart_fungible_token::*;
//     use oysterpack_smart_near::near_sdk::{env, serde_json, test_utils, VMContext};
//     use oysterpack_smart_near::{component::*, *};
//     use oysterpack_smart_near_test::*;
//     use std::collections::HashMap;
//     use std::convert::*;
//
//     const TREASURER_PERMISSION_BIT: u8 = 0;
//
//     fn account_manager() -> AccountManager {
//         StakeFungibleToken::register_storage_management_event_handler();
//         let mut permissions = HashMap::new();
//         permissions.insert(TREASURER_PERMISSION_BIT, PERMISSION_TREASURER);
//         let contract_permissions = ContractPermissions(permissions);
//         AccountManager::new(contract_permissions)
//     }
//
//     fn ft_stake() -> StakeFungibleToken {
//         StakeFungibleToken::new(account_manager())
//     }
//
//     fn staking_public_key() -> PublicKey {
//         let key = [0_u8; 33];
//         let key: PublicKey = key[..].try_into().unwrap();
//         key
//     }
//
//     fn staking_public_key_as_string() -> String {
//         let pk_bytes: Vec<u8> = staking_public_key().into();
//         bs58::encode(pk_bytes).into_string()
//     }
//
//     fn deploy_with_registered_account() -> (VMContext, StakingPoolComponent) {
//         deploy(OWNER, ADMIN, ACCOUNT, true)
//     }
//
//     fn deploy_with_unregistered_account() -> (VMContext, StakingPoolComponent) {
//         deploy(OWNER, ADMIN, ACCOUNT, false)
//     }
//
//     fn deploy(
//         owner: &str,
//         admin: &str,
//         account: &str,
//         register_account: bool,
//     ) -> (VMContext, StakingPoolComponent) {
//         let mut ctx = new_context(account);
//         testing_env!(ctx.clone());
//
//         ContractOwnershipComponent::deploy(to_valid_account_id(owner));
//
//         AccountManager::deploy(AccountManagementComponentConfig {
//             storage_usage_bounds: None,
//             admin_account: to_valid_account_id(admin),
//             component_account_storage_mins: Some(vec![StakeFungibleToken::account_storage_min]),
//         });
//
//         StakeFungibleToken::deploy(FungibleTokenConfig {
//             metadata: Metadata {
//                 spec: Spec(FT_METADATA_SPEC.to_string()),
//                 name: Name("STAKE".to_string()),
//                 symbol: Symbol("STAKE".to_string()),
//                 decimals: 24,
//                 icon: None,
//                 reference: None,
//                 reference_hash: None,
//             },
//             token_supply: 0,
//         });
//
//         if register_account {
//             ctx.attached_deposit = YOCTO;
//             testing_env!(ctx.clone());
//             account_manager().storage_deposit(None, Some(true));
//
//             println!(
//                 "after account registered() : State::contract_managed_total_balance() = {}",
//                 State::contract_managed_total_balance()
//             );
//         }
//
//         StakingPoolComponent::deploy(StakingPoolComponentConfig {
//             stake_public_key: staking_public_key(),
//             staking_fee: None,
//         });
//         println!(
//             "after StakingPoolComponent::deploy() : {}",
//             State::contract_managed_total_balance()
//         );
//         assert!(AccountNearDataObject::exists(env::current_account_id().as_str()),
//                 "staking pool deployment should have registered an account for itself to serve as the treasury");
//
//         (
//             ctx,
//             StakingPoolComponent::new(account_manager(), ft_stake()),
//         )
//     }
//
//     fn bring_pool_online(mut ctx: VMContext, staking_pool: &mut StakingPoolComponent) {
//         ctx.predecessor_account_id = ADMIN.to_string();
//         testing_env!(ctx.clone());
//         staking_pool.ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
//         assert!(staking_pool.ops_stake_status().is_online());
//         println!("{:#?}", test_utils::get_logs());
//     }
//
//     const OWNER: &str = "owner";
//     const ADMIN: &str = "admin";
//     const ACCOUNT: &str = "bob";
//
//     #[test]
//     fn basic_workflow() {
//         let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);
//
//         // the staking pool is initially offline after deployment
//         let state = *StakingPoolComponent::state();
//         let state_json = serde_json::to_string_pretty(&state).unwrap();
//         println!("{}", state_json);
//         assert!(!state.status.is_online());
//
//         assert_eq!(staking_pool.ops_stake_token_value(None), YOCTO.into());
//
//         // Assert - account has zero STAKE balance to start with
//         let account_manager = account_manager();
//         assert_eq!(
//             staking_pool.ops_stake_balance(to_valid_account_id(ACCOUNT)),
//             Some(StakeAccountBalances {
//                 storage_balance: StorageBalance {
//                     total: account_manager.storage_balance_bounds().min,
//                     available: YoctoNear::ZERO
//                 },
//                 staked: None,
//                 unstaked: None
//             })
//         );
//
//         // Act - accounts can stake while the pool is offline
//         {
//             let mut ctx = ctx.clone();
//             ctx.attached_deposit = YOCTO;
//             testing_env!(ctx.clone());
//             assert_eq!(env::account_locked_balance(), 0);
//             if let PromiseOrValue::Value(balance) = staking_pool.ops_stake() {
//                 let staking_fee = state.staking_fee * YOCTO.into();
//                 assert_eq!(
//                     balance.staked,
//                     Some(StakedBalance {
//                         stake: (YOCTO - *staking_fee).into(),
//                         near_value: (YOCTO - *staking_fee).into()
//                     })
//                 );
//                 assert_eq!(env::account_locked_balance(), 0);
//
//                 assert_eq!(
//                     staking_pool.ops_stake_pool_balances().total_staked,
//                     YOCTO.into()
//                 );
//             } else {
//                 panic!("expected value")
//             }
//             assert_eq!(staking_pool.ops_stake_token_value(None), YOCTO.into());
//             println!("{:#?}", test_utils::get_logs());
//         }
//
//         // Act - bring the pool online
//         {
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ADMIN.to_string();
//             testing_env!(ctx);
//             staking_pool.ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
//             assert!(staking_pool.ops_stake_status().is_online());
//             let state = *StakingPoolComponent::state();
//             println!(
//                 "after pool is online {}",
//                 serde_json::to_string_pretty(&state).unwrap()
//             );
//             assert!(state.status.is_online());
//
//             let logs = test_utils::get_logs();
//             println!("{:#?}", logs);
//             assert_eq!(logs, vec!["[INFO] [STATUS_ONLINE] ",]);
//
//             let receipts = deserialize_receipts();
//             assert_eq!(receipts.len(), 2);
//             {
//                 let receipt = &receipts[0];
//                 assert_eq!(receipt.receiver_id, env::current_account_id());
//                 assert_eq!(receipt.actions.len(), 1);
//                 let action = &receipt.actions[0];
//
//                 match action {
//                     Action::Stake(action) => {
//                         assert_eq!(action.stake, YOCTO);
//
//                         assert_eq!(staking_public_key_as_string(), action.public_key);
//                     }
//                     _ => panic!("expected StakeAction"),
//                 }
//             }
//             {
//                 let receipt = &receipts[1];
//                 assert_eq!(receipt.receiver_id, env::current_account_id());
//                 assert_eq!(receipt.actions.len(), 1);
//                 let action = &receipt.actions[0];
//
//                 match action {
//                     Action::FunctionCall(action) => {
//                         assert_eq!(action.method_name, "ops_stake_start_finalize");
//                         assert!(action.args.is_empty());
//                         assert_eq!(action.deposit, 0);
//                     }
//                     _ => panic!("expected FunctionCall"),
//                 }
//             }
//         }
//     }
//
//     #[cfg(test)]
//     mod tests_stake_online {
//         use super::*;
//
//         #[test]
//         fn stake_attached_deposit() {}
//
//         #[test]
//         fn stake_with_zero_storage_available_balance() {
//             let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);
//
//             let initial_staking_pool_balances = staking_pool.ops_stake_pool_balances();
//             println!(
//                 "before pool is online = {:#?}",
//                 initial_staking_pool_balances
//             );
//
//             // Arrange
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//             let initial_staking_pool_balances = staking_pool.ops_stake_pool_balances();
//             println!(
//                 "after pool is online = {:#?}",
//                 initial_staking_pool_balances
//             );
//
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 ctx.attached_deposit = 1000;
//                 testing_env!(ctx);
//                 // Act
//                 if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
//                     panic!("expected Promise")
//                 }
//                 // Assert
//                 assert_eq!(State::liquidity(), YoctoNear::ZERO);
//                 let state = *StakingPoolComponent::state();
//                 println!(
//                     "staked 1000 {}",
//                     serde_json::to_string_pretty(&state).unwrap()
//                 );
//                 println!(
//                     "after staking {:#?}",
//                     staking_pool.ops_stake_pool_balances()
//                 );
//                 assert_eq!(
//                     staking_pool.ops_stake_pool_balances().total_staked,
//                     initial_staking_pool_balances.total_staked + 1000
//                 );
//
//                 let logs = test_utils::get_logs();
//                 println!("{:#?}", logs);
//                 assert_eq!(
//                     logs,
//                     vec!["[INFO] [STAKE] near_amount=1000, stake_token_amount=1000",]
//                 );
//
//                 let receipts = deserialize_receipts();
//                 assert_eq!(receipts.len(), 2);
//                 {
//                     let receipt = &receipts[0];
//                     assert_eq!(receipt.receiver_id, env::current_account_id());
//                     assert_eq!(receipt.actions.len(), 1);
//                     let action = &receipt.actions[0];
//                     match action {
//                         Action::Stake(action) => {
//                             assert_eq!(action.stake, 1000);
//                         }
//                         _ => panic!("expected StakeAction"),
//                     }
//                 }
//                 {
//                     let receipt = &receipts[1];
//                     assert_eq!(receipt.receiver_id, env::current_account_id());
//                     assert_eq!(receipt.actions.len(), 1);
//                     let action = &receipt.actions[0];
//                     match action {
//                         Action::FunctionCall(f) => {
//                             assert_eq!(f.method_name, "ops_stake_finalize");
//                             let args: StakeActionCallbackArgs =
//                                 serde_json::from_str(&f.args).unwrap();
//                             assert_eq!(args.account_id, ACCOUNT);
//                         }
//                         _ => panic!("expected FunctionCall"),
//                     }
//                 }
//             }
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 ctx.attached_deposit = 1000;
//                 testing_env!(ctx);
//                 // Act
//                 if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
//                     panic!("expected Promise")
//                 }
//
//                 // Assert
//                 assert_eq!(staking_pool.ops_stake_token_value(None), YOCTO.into());
//                 let state = *StakingPoolComponent::state();
//                 println!(
//                     "staked 1000 {}",
//                     serde_json::to_string_pretty(&state).unwrap()
//                 );
//                 assert_eq!(
//                     staking_pool.ops_stake_pool_balances().total_staked,
//                     2000.into()
//                 );
//                 let logs = test_utils::get_logs();
//                 println!("{:#?}", logs);
//
//                 let logs = test_utils::get_logs();
//                 println!("{:#?}", logs);
//                 assert_eq!(
//                     logs,
//                     vec!["[INFO] [STAKE] near_amount=1000, stake_token_amount=1000",]
//                 );
//
//                 let receipts = deserialize_receipts();
//                 assert_eq!(receipts.len(), 2);
//                 {
//                     let receipt = &receipts[0];
//                     assert_eq!(receipt.receiver_id, env::current_account_id());
//                     assert_eq!(receipt.actions.len(), 1);
//                     let action = &receipt.actions[0];
//                     match action {
//                         Action::Stake(action) => {
//                             assert_eq!(action.stake, 2000);
//                         }
//                         _ => panic!("expected StakeAction"),
//                     }
//                 }
//                 {
//                     let receipt = &receipts[1];
//                     assert_eq!(receipt.receiver_id, env::current_account_id());
//                     assert_eq!(receipt.actions.len(), 1);
//                     let action = &receipt.actions[0];
//                     match action {
//                         Action::FunctionCall(f) => {
//                             assert_eq!(f.method_name, "ops_stake_finalize");
//                             let args: StakeActionCallbackArgs =
//                                 serde_json::from_str(&f.args).unwrap();
//                             assert_eq!(args.account_id, ACCOUNT);
//                         }
//                         _ => panic!("expected FunctionCall"),
//                     }
//                 }
//             }
//         }
//
//         #[test]
//         fn stake_with_storage_available_balance() {
//             let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);
//
//             // Arrange
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//
//             // deposit some funds into account's storage balance
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 ctx.attached_deposit = YOCTO;
//                 testing_env!(ctx.clone());
//                 let mut account_manager = account_manager();
//                 account_manager.storage_deposit(None, None);
//             }
//
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 ctx.attached_deposit = 1000;
//                 testing_env!(ctx);
//                 // Act
//                 if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
//                     panic!("expected Promise")
//                 }
//                 // Assert
//                 assert_eq!(State::liquidity(), YoctoNear::ZERO);
//                 let state = *StakingPoolComponent::state();
//                 println!(
//                     "staked 1000 {}",
//                     serde_json::to_string_pretty(&state).unwrap()
//                 );
//                 assert_eq!(
//                     staking_pool.ops_stake_pool_balances().total_staked,
//                     (YOCTO + 1000).into()
//                 );
//
//                 let logs = test_utils::get_logs();
//                 println!("{:#?}", logs);
//                 assert_eq!(
//                     logs,
//                     vec![
//                         "[INFO] [ACCOUNT_STORAGE_CHANGED] Withdrawal(YoctoNear(1000000000000000000000000))",
//                         "[INFO] [STAKE] near_amount=1000000000000000000001000, stake_token_amount=1000000000000000000001000",
//                     ]
//                 );
//
//                 let receipts = deserialize_receipts();
//                 assert_eq!(receipts.len(), 2);
//                 {
//                     let receipt = &receipts[0];
//                     assert_eq!(receipt.receiver_id, env::current_account_id());
//                     assert_eq!(receipt.actions.len(), 1);
//                     let action = &receipt.actions[0];
//                     match action {
//                         Action::Stake(action) => {
//                             assert_eq!(action.stake, 1000000000000000000001000);
//                         }
//                         _ => panic!("expected StakeAction"),
//                     }
//                 }
//                 {
//                     let receipt = &receipts[1];
//                     assert_eq!(receipt.receiver_id, env::current_account_id());
//                     assert_eq!(receipt.actions.len(), 1);
//                     let action = &receipt.actions[0];
//                     match action {
//                         Action::FunctionCall(f) => {
//                             assert_eq!(f.method_name, "ops_stake_finalize");
//                             let args: StakeActionCallbackArgs =
//                                 serde_json::from_str(&f.args).unwrap();
//                             assert_eq!(args.account_id, ACCOUNT);
//                         }
//                         _ => panic!("expected FunctionCall"),
//                     }
//                 }
//             }
//         }
//
//         #[test]
//         fn staked_amount_has_near_remainder() {
//             let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);
//
//             // Arrange
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//
//             let total_supply: TokenAmount = 1000.into();
//             let total_staked_balance: YoctoNear = 1005.into();
//             {
//                 testing_env!(ctx.clone());
//                 let mut ft_stake = ft_stake();
//                 ft_stake.ft_mint(ACCOUNT, total_supply);
//                 assert_eq!(ft_stake.ft_total_supply(), total_supply);
//
//                 let mut state = StakingPoolComponent::state();
//                 state.total_staked_balance = total_staked_balance;
//                 state.save();
//             }
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             ctx.attached_deposit = 100;
//             testing_env!(ctx);
//             let account = account_manager().registered_account_near_data(ACCOUNT);
//             // Act
//             if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
//                 panic!("expected Value")
//             }
//             let state = *StakingPoolComponent::state();
//             let logs = test_utils::get_logs();
//             println!(
//                 "{}\n{:#?}",
//                 serde_json::to_string_pretty(&state).unwrap(),
//                 logs
//             );
//             assert_eq!(
//                 logs,
//                 vec![
//                     "[INFO] [ACCOUNT_STORAGE_CHANGED] Deposit(YoctoNear(1))",
//                     "[INFO] [STAKE] near_amount=99, stake_token_amount=99",
//                 ]
//             );
//             let account_after_staking = account_manager().registered_account_near_data(ACCOUNT);
//             assert_eq!(
//                 account_after_staking.near_balance(),
//                 account.near_balance() + 1
//             );
//
//             let receipts = deserialize_receipts();
//             assert_eq!(receipts.len(), 2);
//             {
//                 let receipt = &receipts[0];
//                 assert_eq!(receipt.receiver_id, env::current_account_id());
//                 assert_eq!(receipt.actions.len(), 1);
//                 let action = &receipt.actions[0];
//                 match action {
//                     Action::Stake(action) => {
//                         assert_eq!(action.stake, *total_staked_balance + 99);
//                     }
//                     _ => panic!("expected StakeAction"),
//                 }
//             }
//             {
//                 let receipt = &receipts[1];
//                 assert_eq!(receipt.receiver_id, env::current_account_id());
//                 assert_eq!(receipt.actions.len(), 1);
//                 let action = &receipt.actions[0];
//                 match action {
//                     Action::FunctionCall(f) => {
//                         assert_eq!(f.method_name, "ops_stake_finalize");
//                         let args: StakeActionCallbackArgs = serde_json::from_str(&f.args).unwrap();
//                         assert_eq!(args.account_id, ACCOUNT);
//                     }
//                     _ => panic!("expected FunctionCall"),
//                 }
//             }
//         }
//
//         #[test]
//         fn with_zero_stake_amount() {
//             let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);
//
//             // Arrange
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx);
//             match staking_pool.ops_stake() {
//                 PromiseOrValue::Value(balance) => {
//                     assert!(balance.staked.is_none());
//                     assert_eq!(balance.storage_balance.available, YoctoNear::ZERO);
//                 }
//                 _ => panic!("expected Value"),
//             }
//         }
//
//         #[test]
//         fn not_enough_to_stake() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//
//             // Arrange
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//
//             let total_supply: TokenAmount = 1000.into();
//             let total_staked_balance: YoctoNear = 1005.into();
//             {
//                 testing_env!(ctx.clone());
//                 let mut ft_stake = ft_stake();
//                 ft_stake.ft_mint(ACCOUNT, total_supply);
//                 assert_eq!(ft_stake.ft_total_supply(), total_supply);
//
//                 let mut state = StakingPoolComponent::state();
//                 state.total_staked_balance = total_staked_balance;
//                 state.save();
//             }
//
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             ctx.attached_deposit = 1;
//             testing_env!(ctx);
//             let account = account_manager().registered_account_near_data(ACCOUNT);
//             // Act
//             match staking_pool.ops_stake() {
//                 PromiseOrValue::Value(balance) => {
//                     assert_eq!(balance.staked.unwrap().stake, total_supply);
//                     assert_eq!(balance.storage_balance.available, 1.into());
//                 }
//                 _ => panic!("expected Value"),
//             }
//             let state = *StakingPoolComponent::state();
//             let logs = test_utils::get_logs();
//             println!(
//                 "{}\n{:#?}",
//                 serde_json::to_string_pretty(&state).unwrap(),
//                 logs
//             );
//             assert_eq!(
//                 logs,
//                 vec![
//                     "[INFO] [ACCOUNT_STORAGE_CHANGED] Deposit(YoctoNear(1))",
//                     "[INFO] [NOT_ENOUGH_TO_STAKE] ",
//                 ]
//             );
//             let account_after_staking = account_manager().registered_account_near_data(ACCOUNT);
//             assert_eq!(
//                 account_after_staking.near_balance(),
//                 account.near_balance() + 1
//             );
//
//             let receipts = deserialize_receipts();
//             assert!(receipts.is_empty());
//         }
//
//         #[test]
//         fn with_liquidity_needed() {
//             let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);
//
//             // Arrange
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//             // simulate some unstaked balance
//             testing_env!(ctx.clone());
//             State::incr_total_unstaked_balance((10 * YOCTO).into());
//
//             // Act
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = YOCTO;
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 testing_env!(ctx.clone());
//                 if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
//                     panic!("expected Promise")
//                 }
//             }
//             // Assert
//             assert_eq!(State::liquidity(), YOCTO.into());
//             assert_eq!(State::total_unstaked_balance(), (9 * YOCTO).into());
//             let logs = test_utils::get_logs();
//             println!("{:#?}", logs);
//             assert_eq!(logs, vec![
//                 "[INFO] [LIQUIDITY] added=1000000000000000000000000, total=1000000000000000000000000",
//                 "[INFO] [STAKE] near_amount=1000000000000000000000000, stake_token_amount=1000000000000000000000000",
//             ]);
//
//             // Act
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = YOCTO;
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 testing_env!(ctx);
//                 if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
//                     panic!("expected Promise")
//                 }
//             }
//             // Assert
//             let logs = test_utils::get_logs();
//             println!("{:#?}", logs);
//             assert_eq!(logs, vec![
//                 "[INFO] [LIQUIDITY] added=1000000000000000000000000, total=2000000000000000000000000",
//                 "[INFO] [STAKE] near_amount=1000000000000000000000000, stake_token_amount=1000000000000000000000000",
//             ]);
//
//             assert_eq!(State::liquidity(), (2 * YOCTO).into());
//             assert_eq!(State::total_unstaked_balance(), (8 * YOCTO).into());
//         }
//
//         #[test]
//         #[should_panic(
//             expected = "[ERR] [INVALID] not enough gas was attached - min required gas is 27 TGas"
//         )]
//         fn not_enough_gas_attached_to_cover_callback() {
//             let (mut ctx, mut staking_pool) = deploy_with_registered_account();
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//
//             let min_callback_gas = StakingPoolComponent::min_callback_gas();
//             println!(
//                 "min_callback_gas = {} | {} TGas",
//                 min_callback_gas,
//                 *min_callback_gas / TERA
//             );
//
//             ctx.prepaid_gas = 300 * TERA;
//             ctx.attached_deposit = YOCTO;
//             testing_env!(ctx.clone());
//             staking_pool.ops_stake();
//             println!("used gas: {}", env::used_gas());
//             println!("{:#?}", test_utils::get_logs());
//             let receipts = deserialize_receipts();
//             let callback = &receipts[1].actions[0];
//             let callback_gas = {
//                 if let Action::FunctionCall(action) = callback {
//                     action.gas
//                 } else {
//                     panic!("expected FunctionCall")
//                 }
//             };
//
//             let prepaid_gas = (300 * TERA) - callback_gas;
//             println!("prepaid_gas = {}", prepaid_gas);
//             ctx.prepaid_gas = prepaid_gas;
//             ctx.attached_deposit = YOCTO;
//             testing_env!(ctx);
//             staking_pool.ops_stake();
//         }
//
//         #[test]
//         fn stake_with_min_gas() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//
//             // very first stake requires an extra TGas to cover for storage allocation
//             let mut ctx = ctx.clone();
//             ctx.prepaid_gas = 28 * TERA;
//             ctx.attached_deposit = YOCTO;
//             testing_env!(ctx.clone());
//             staking_pool.ops_stake();
//             println!("used gas: {}", env::used_gas());
//             println!("{:#?}", test_utils::get_logs());
//             deserialize_receipts();
//
//             // subsequent stake requires 1 TGas less
//             let mut ctx = ctx.clone();
//             ctx.prepaid_gas = 27 * TERA;
//             ctx.attached_deposit = YOCTO;
//             testing_env!(ctx.clone());
//             staking_pool.ops_stake();
//             println!("used gas: {}", env::used_gas());
//             println!("{:#?}", test_utils::get_logs());
//             deserialize_receipts();
//         }
//     }
//
//     #[cfg(test)]
//     mod tests_stake_offline {
//         use super::*;
//
//         #[test]
//         fn stake_with_zero_storage_available_balance() {
//             let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);
//
//             // Arrange
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 ctx.attached_deposit = 1000;
//                 testing_env!(ctx);
//                 // Act
//                 let state = *StakingPoolComponent::state();
//                 println!(
//                     "before staking {}\ncurrent treasury stake balance: {}",
//                     serde_json::to_string_pretty(&state).unwrap(),
//                     ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()))
//                 );
//                 if let PromiseOrValue::Value(balances) = staking_pool.ops_stake() {
//                     println!("{:#?}", balances);
//                     let staked_balance = balances.staked.unwrap();
//                     let staking_fee = state.staking_fee * 1000.into();
//                     assert_eq!(staked_balance.stake, (1000 - *staking_fee).into());
//                     assert_eq!(staked_balance.near_value, (1000 - *staking_fee).into());
//                 } else {
//                     panic!("expected Value");
//                 }
//                 // Assert
//                 assert_eq!(State::liquidity(), YoctoNear::ZERO);
//                 let state = *StakingPoolComponent::state();
//                 println!(
//                     "staked 1000 {}",
//                     serde_json::to_string_pretty(&state).unwrap()
//                 );
//                 assert_eq!(
//                     staking_pool.ops_stake_pool_balances().total_staked,
//                     1000.into()
//                 );
//                 assert_eq!(state.treasury_balance, state.staking_fee * 1000.into());
//
//                 let logs = test_utils::get_logs();
//                 println!("{:#?}", logs);
//                 assert_eq!(
//                     logs,
//                     vec![
//                         "[INFO] [STAKE] near_amount=1000, stake_token_amount=1000",
//                         "[WARN] [STATUS_OFFLINE] ",
//                         "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
//                         "[INFO] [FT_MINT] account: bob, amount: 1000",
//                         "[INFO] [FT_BURN] account: bob, amount: 8",
//                         "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
//                         "[INFO] [FT_MINT] account: contract.near, amount: 8",
//                     ]
//                 );
//
//                 let receipts = deserialize_receipts();
//                 assert!(receipts.is_empty());
//             }
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 ctx.attached_deposit = 1000;
//                 testing_env!(ctx);
//                 // Act
//                 if let PromiseOrValue::Promise(_) = staking_pool.ops_stake() {
//                     panic!("expected Value")
//                 }
//
//                 // Assert
//                 let state = *StakingPoolComponent::state();
//                 println!(
//                     "staked another 1000 {}",
//                     serde_json::to_string_pretty(&state).unwrap()
//                 );
//                 assert_eq!(
//                     staking_pool.ops_stake_pool_balances().total_staked,
//                     2000.into()
//                 );
//                 let logs = test_utils::get_logs();
//                 println!("{:#?}", logs);
//
//                 let logs = test_utils::get_logs();
//                 println!("{:#?}", logs);
//                 assert_eq!(
//                     logs,
//                     vec![
//                         "[INFO] [STAKE] near_amount=1000, stake_token_amount=1000",
//                         "[WARN] [STATUS_OFFLINE] ",
//                         "[INFO] [FT_MINT] account: bob, amount: 1000",
//                         "[INFO] [FT_BURN] account: bob, amount: 8",
//                         "[INFO] [FT_MINT] account: contract.near, amount: 8",
//                     ]
//                 );
//
//                 let receipts = deserialize_receipts();
//                 assert!(receipts.is_empty());
//             }
//         }
//
//         #[test]
//         fn stake_with_storage_available_balance() {
//             let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);
//
//             // Arrange
//
//             // deposit some funds into account's storage balance
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 ctx.attached_deposit = YOCTO;
//                 testing_env!(ctx.clone());
//                 let mut account_manager = account_manager();
//                 account_manager.storage_deposit(None, None);
//             }
//
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 ctx.attached_deposit = 1000;
//                 testing_env!(ctx);
//                 // Act
//                 let state = *StakingPoolComponent::state();
//                 if let PromiseOrValue::Value(balances) = staking_pool.ops_stake() {
//                     println!("{:#?}", balances);
//                     let staked_balances = balances.staked.unwrap();
//                     let staking_fee = state.staking_fee * (YOCTO + 1000).into();
//                     assert_eq!(staked_balances.stake, (YOCTO + 1000 - *staking_fee).into());
//                     assert_eq!(
//                         staked_balances.near_value,
//                         (YOCTO + 1000 - *staking_fee).into()
//                     );
//                 } else {
//                     panic!("expected Promise")
//                 }
//                 // Assert
//                 assert_eq!(State::liquidity(), YoctoNear::ZERO);
//                 let state = *StakingPoolComponent::state();
//                 println!(
//                     "staked 1000 {}",
//                     serde_json::to_string_pretty(&state).unwrap()
//                 );
//                 assert_eq!(
//                     staking_pool.ops_stake_pool_balances().total_staked,
//                     1000.into()
//                 );
//
//                 let logs = test_utils::get_logs();
//                 println!("{:#?}", logs);
//                 assert_eq!(
//                     logs,
//                     vec![
//                         "[INFO] [ACCOUNT_STORAGE_CHANGED] Withdrawal(YoctoNear(1000000000000000000000000))",
//                         "[INFO] [STAKE] near_amount=1000000000000000000001000, stake_token_amount=1000000000000000000001000",
//                         "[WARN] [STATUS_OFFLINE] ",
//                         "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
//                         "[INFO] [FT_MINT] account: bob, amount: 1000000000000000000001000",
//                         "[INFO] [FT_BURN] account: bob, amount: 8000000000000000000008",
//                         "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
//                         "[INFO] [FT_MINT] account: contract.near, amount: 8000000000000000000008",
//                     ]
//                 );
//
//                 assert!(deserialize_receipts().is_empty());
//             }
//         }
//
//         #[test]
//         fn staked_amount_has_near_remainder() {
//             let (mut ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);
//
//             // Arrange
//             let total_supply: TokenAmount = 1000.into();
//             let total_staked_balance: YoctoNear = 1005.into();
//             {
//                 testing_env!(ctx.clone());
//                 let mut ft_stake = ft_stake();
//                 ft_stake.ft_mint(ACCOUNT, total_supply);
//                 assert_eq!(ft_stake.ft_total_supply(), total_supply);
//
//                 let mut state = StakingPoolComponent::state();
//                 state.total_staked_balance = total_staked_balance;
//                 state.save();
//             }
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             ctx.attached_deposit = 100;
//             testing_env!(ctx.clone());
//             let account = account_manager().registered_account_near_data(ACCOUNT);
//             // Act
//             let state = *StakingPoolComponent::state();
//             println!(
//                 "before staking: {}",
//                 serde_json::to_string_pretty(&state).unwrap()
//             );
//             if let PromiseOrValue::Value(balances) = staking_pool.ops_stake() {
//                 println!("{:#?}", balances);
//                 let staked_balances = balances.staked.unwrap();
//                 assert_eq!(staked_balances.stake, (1000 + (1000 * 100 / 1005)).into());
//                 assert_eq!(
//                     staked_balances.near_value,
//                     ((1000 + (1000 * 100 / 1005)) * 1005 / 1000).into()
//                 );
//             } else {
//                 panic!("expected Value")
//             }
//             let state = *StakingPoolComponent::state();
//             let logs = test_utils::get_logs();
//             println!(
//                 "{}\n{:#?}",
//                 serde_json::to_string_pretty(&state).unwrap(),
//                 logs
//             );
//             assert_eq!(
//                 logs,
//                 vec![
//                     "[INFO] [ACCOUNT_STORAGE_CHANGED] Deposit(YoctoNear(1))",
//                     "[INFO] [STAKE] near_amount=99, stake_token_amount=99",
//                     "[WARN] [STATUS_OFFLINE] ",
//                     "[INFO] [FT_MINT] account: bob, amount: 99",
//                 ]
//             );
//             let account_after_staking = account_manager().registered_account_near_data(ACCOUNT);
//             assert_eq!(
//                 account_after_staking.near_balance(),
//                 account.near_balance() + 1
//             );
//
//             assert!(deserialize_receipts().is_empty());
//         }
//
//         #[test]
//         fn with_zero_stake_amount() {
//             let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);
//
//             // Arrange
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx);
//             match staking_pool.ops_stake() {
//                 PromiseOrValue::Value(balance) => {
//                     assert!(balance.staked.is_none());
//                     assert_eq!(balance.storage_balance.available, YoctoNear::ZERO);
//                 }
//                 _ => panic!("expected Value"),
//             }
//         }
//
//         #[test]
//         fn not_enough_to_stake() {
//             let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);
//
//             // Arrange
//             let total_supply: TokenAmount = 1000.into();
//             let total_staked_balance: YoctoNear = 1005.into();
//             {
//                 testing_env!(ctx.clone());
//                 let mut ft_stake = ft_stake();
//                 ft_stake.ft_mint(ACCOUNT, total_supply);
//                 assert_eq!(ft_stake.ft_total_supply(), total_supply);
//
//                 let mut state = StakingPoolComponent::state();
//                 state.total_staked_balance = total_staked_balance;
//                 state.save();
//             }
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             ctx.attached_deposit = 1;
//             testing_env!(ctx);
//             let account = account_manager().registered_account_near_data(ACCOUNT);
//             // Act
//             match staking_pool.ops_stake() {
//                 PromiseOrValue::Value(balance) => {
//                     assert_eq!(balance.staked.unwrap().stake, total_supply);
//                     assert_eq!(balance.storage_balance.available, 1.into());
//                 }
//                 _ => panic!("expected Value"),
//             }
//             let state = *StakingPoolComponent::state();
//             let logs = test_utils::get_logs();
//             println!(
//                 "{}\n{:#?}",
//                 serde_json::to_string_pretty(&state).unwrap(),
//                 logs
//             );
//             assert_eq!(
//                 logs,
//                 vec![
//                     "[INFO] [ACCOUNT_STORAGE_CHANGED] Deposit(YoctoNear(1))",
//                     "[INFO] [NOT_ENOUGH_TO_STAKE] ",
//                 ]
//             );
//             let account_after_staking = account_manager().registered_account_near_data(ACCOUNT);
//             assert_eq!(
//                 account_after_staking.near_balance(),
//                 account.near_balance() + 1
//             );
//
//             let receipts = deserialize_receipts();
//             assert!(receipts.is_empty());
//         }
//
//         #[test]
//         fn with_liquidity_needed() {
//             let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);
//
//             // Arrange
//             // simulate some unstaked balance
//             testing_env!(ctx.clone());
//             State::incr_total_unstaked_balance((10 * YOCTO).into());
//
//             // Act
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = YOCTO;
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 testing_env!(ctx.clone());
//                 if let PromiseOrValue::Value(balances) = staking_pool.ops_stake() {
//                     println!("{:#?}", balances);
//                     let state = *StakingPoolComponent::state();
//                     let staking_fee = state.staking_fee * YOCTO.into();
//                     assert_eq!(
//                         balances.staked.unwrap(),
//                         StakedBalance {
//                             stake: (YOCTO - *staking_fee).into(),
//                             near_value: (YOCTO - *staking_fee).into()
//                         }
//                     );
//                 } else {
//                     panic!("expected Promise")
//                 }
//             }
//             // Assert
//             assert_eq!(State::liquidity(), YOCTO.into());
//             assert_eq!(State::total_unstaked_balance(), (9 * YOCTO).into());
//             let logs = test_utils::get_logs();
//             println!("{:#?}", logs);
//             assert_eq!(logs, vec![
//                 "[INFO] [LIQUIDITY] added=1000000000000000000000000, total=1000000000000000000000000",
//                 "[INFO] [STAKE] near_amount=1000000000000000000000000, stake_token_amount=1000000000000000000000000",
//                 "[WARN] [STATUS_OFFLINE] ",
//                 "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
//                 "[INFO] [FT_MINT] account: bob, amount: 1000000000000000000000000",
//                 "[INFO] [FT_BURN] account: bob, amount: 8000000000000000000000",
//                 "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
//                 "[INFO] [FT_MINT] account: contract.near, amount: 8000000000000000000000",
//             ]);
//
//             // Act
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = YOCTO;
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 testing_env!(ctx);
//                 if let PromiseOrValue::Value(balances) = staking_pool.ops_stake() {
//                     let state = *StakingPoolComponent::state();
//                     let staking_fee = state.staking_fee * YOCTO.into();
//                     assert_eq!(
//                         balances.staked.unwrap(),
//                         StakedBalance {
//                             stake: (2 * YOCTO - (2 * *staking_fee)).into(),
//                             near_value: (2 * YOCTO - (2 * *staking_fee)).into()
//                         }
//                     );
//                 } else {
//                     panic!("expected Promise")
//                 }
//             }
//             // Assert
//             let logs = test_utils::get_logs();
//             println!("{:#?}", logs);
//             assert_eq!(logs, vec![
//                 "[INFO] [LIQUIDITY] added=1000000000000000000000000, total=2000000000000000000000000",
//                 "[INFO] [STAKE] near_amount=1000000000000000000000000, stake_token_amount=1000000000000000000000000",
//                 "[WARN] [STATUS_OFFLINE] ",
//                 "[INFO] [FT_MINT] account: bob, amount: 1000000000000000000000000",
//                 "[INFO] [FT_BURN] account: bob, amount: 8000000000000000000000",
//                 "[INFO] [FT_MINT] account: contract.near, amount: 8000000000000000000000",
//             ]);
//
//             assert_eq!(State::liquidity(), (2 * YOCTO).into());
//             assert_eq!(State::total_unstaked_balance(), (8 * YOCTO).into());
//         }
//     }
//
//     #[cfg(test)]
//     mod tests_stake_finalize {
//         use super::*;
//         use oysterpack_smart_contract::components::contract_metrics::ContractMetricsComponent;
//         use oysterpack_smart_contract::ContractMetrics;
//
//         #[test]
//         fn stake_action_success() {
//             let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);
//
//             // Arrange
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//             // stake
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = YOCTO;
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 testing_env!(ctx.clone());
//                 if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
//                     panic!("expected Promise")
//                 }
//             }
//             let logs = test_utils::get_logs();
//             println!("{:#?}", logs);
//
//             // Act
//             {
//                 let state = StakingPoolComponent::state();
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = 0;
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 ctx.account_locked_balance = *state.total_staked_balance;
//                 testing_env_with_promise_result_success(ctx.clone());
//                 let balances = staking_pool.ops_stake_finalize(ACCOUNT.to_string());
//                 // Assert
//                 let logs = test_utils::get_logs();
//                 println!("ops_stake_finalize: {:#?}", logs);
//                 println!("{:#?}", balances);
//                 {
//                     let staked_balance = balances.staked.unwrap();
//                     let expected_amount = YOCTO - *(state.staking_fee * YOCTO.into());
//                     assert_eq!(staked_balance.stake, expected_amount.into());
//                     assert_eq!(staked_balance.near_value, expected_amount.into());
//                 }
//                 assert!(balances.unstaked.is_none());
//                 let state = *StakingPoolComponent::state();
//                 println!("{}", serde_json::to_string_pretty(&state).unwrap());
//                 assert_eq!(state.total_staked_balance, YOCTO.into());
//             }
//
//             let contract_near_balances = ContractMetricsComponent.ops_metrics_near_balances();
//             println!("{:#?}", contract_near_balances);
//         }
//
//         #[test]
//         fn stake_action_success_with_dividend_payout() {
//             // Arrange
//             let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//
//             // stake
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = YOCTO;
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 testing_env!(ctx.clone());
//                 if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
//                     panic!("expected Promise")
//                 }
//                 println!("{:#?}", test_utils::get_logs());
//             }
//             // finalize stake
//             {
//                 let state = StakingPoolComponent::state();
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = 0;
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 ctx.account_locked_balance = *state.total_staked_balance;
//                 testing_env_with_promise_result_success(ctx.clone());
//                 staking_pool.ops_stake_finalize(ACCOUNT.to_string());
//                 println!("{:#?}", test_utils::get_logs());
//             }
//
//             let state = *StakingPoolComponent::state();
//             println!(
//                 "after stake finalized {}",
//                 serde_json::to_string_pretty(&state).unwrap()
//             );
//             assert_eq!(
//                 state.treasury_balance,
//                 state.staking_fee * state.total_staked_balance
//             );
//             println!("finalized stake {:#?}", StakingPoolBalances::load());
//
//             // stake - with staking rewards issued - 50 yoctoNEAR staking rewards
//             const STAKING_REWARDS: u128 = 500;
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = YOCTO;
//                 ctx.account_locked_balance = *state.total_staked_balance + STAKING_REWARDS;
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 testing_env!(ctx.clone());
//                 if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
//                     panic!("expected Promise")
//                 }
//                 println!("with staking rewards {:#?}", test_utils::get_logs());
//                 deserialize_receipts();
//             }
//
//             println!("stake with rewards {:#?}", StakingPoolBalances::load());
//
//             let state = *StakingPoolComponent::state();
//             println!(
//                 "before finalize dividend payout {}",
//                 serde_json::to_string_pretty(&state).unwrap()
//             );
//             // Act - finalize stake
//             {
//                 let staking_pool_balances = StakingPoolBalances::load();
//                 println!(
//                     "staked with earned staking rewards {:#?}",
//                     staking_pool_balances
//                 );
//                 let state = StakingPoolComponent::state();
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = 0;
//                 ctx.account_locked_balance = *state.total_staked_balance;
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 ctx.account_locked_balance = *state.total_staked_balance;
//                 testing_env_with_promise_result_success(ctx.clone());
//                 staking_pool.ops_stake_finalize(ACCOUNT.to_string());
//                 let staking_pool_balances = StakingPoolBalances::load();
//                 println!("ops_stake_finalize {:#?}", staking_pool_balances);
//                 let logs = test_utils::get_logs();
//                 println!("ops_stake_finalize: {:#?}", logs);
//                 assert_eq!(
//                     logs,
//                     vec![
//                         "[INFO] [FT_MINT] account: bob, amount: 999999999999999999999500",
//                         "[INFO] [FT_BURN] account: contract.near, amount: 2",
//                         "[INFO] [TREASURY_DIVIDEND] 3 yoctoNEAR / 2 yoctoSTAKE",
//                         "[INFO] [FT_BURN] account: bob, amount: 7999999999999999999994",
//                         "[INFO] [FT_MINT] account: contract.near, amount: 7999999999999999999994",
//                     ]
//                 );
//                 let state = *StakingPoolComponent::state();
//                 println!("{}", serde_json::to_string_pretty(&state).unwrap());
//             }
//             let state_after_stake_finalized = *StakingPoolComponent::state();
//             println!(
//                 "after finalize dividend payout {}",
//                 serde_json::to_string_pretty(&state_after_stake_finalized).unwrap()
//             );
//             assert_eq!(
//                 state_after_stake_finalized.treasury_balance,
//                 state.staking_fee * (2 * YOCTO).into()
//             )
//         }
//
//         #[test]
//         fn stake_action_failed() {
//             let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);
//
//             // Arrange
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//             // stake
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = YOCTO;
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 testing_env!(ctx.clone());
//                 if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
//                     panic!("expected Promise")
//                 }
//             }
//             let logs = test_utils::get_logs();
//             println!("{:#?}", logs);
//
//             // Act
//             {
//                 let state = StakingPoolComponent::state();
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = YOCTO;
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 ctx.account_locked_balance = *state.total_staked_balance;
//                 testing_env_with_promise_result_failure(ctx);
//                 println!("**** finalizing ...");
//                 let balances = staking_pool.ops_stake_finalize(ACCOUNT.to_string());
//                 // Assert
//                 let logs = test_utils::get_logs();
//                 println!("ops_stake_finalize: {:#?}", logs);
//                 assert_eq!(
//                     logs,
//                     vec![
//                         "[ERR] [STAKE_ACTION_FAILED] ",
//                         "[WARN] [STATUS_OFFLINE] StakeActionFailed",
//                         "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
//                         "[INFO] [FT_MINT] account: bob, amount: 1000000000000000000000000",
//                         "[INFO] [FT_BURN] account: bob, amount: 8000000000000000000000",
//                         "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
//                         "[INFO] [FT_MINT] account: contract.near, amount: 8000000000000000000000",
//                     ]
//                 );
//                 assert_eq!(
//                     staking_pool.ops_stake_status(),
//                     Status::Offline(OfflineReason::StakeActionFailed)
//                 );
//                 println!("{:#?}", balances);
//                 {
//                     let staked_balance = balances.staked.unwrap();
//                     let expected_amount = YOCTO - *(state.staking_fee * YOCTO.into());
//                     assert_eq!(staked_balance.stake, expected_amount.into());
//                     assert_eq!(staked_balance.near_value, expected_amount.into());
//                 }
//                 assert!(balances.unstaked.is_none());
//
//                 assert_eq!(
//                     staking_pool.ops_stake_pool_balances().total_staked,
//                     YOCTO.into()
//                 );
//
//                 let state = *StakingPoolComponent::state();
//                 println!("{}", serde_json::to_string_pretty(&state).unwrap());
//                 assert_eq!(state.total_staked_balance, YOCTO.into());
//
//                 let receipts = deserialize_receipts();
//                 assert_eq!(receipts.len(), 2);
//                 {
//                     let receipt = &receipts[0];
//                     assert_eq!(receipt.receiver_id, env::current_account_id());
//                     assert_eq!(receipt.actions.len(), 1);
//                     let action = &receipt.actions[0];
//                     match action {
//                         Action::Stake(action) => {
//                             assert_eq!(action.stake, 0);
//                         }
//                         _ => panic!("expected stake action"),
//                     };
//                 }
//                 {
//                     let receipt = &receipts[1];
//                     assert_eq!(receipt.receiver_id, env::current_account_id());
//                     assert_eq!(receipt.actions.len(), 1);
//                     let action = &receipt.actions[0];
//                     match action {
//                         Action::FunctionCall(function_call) => {
//                             assert_eq!(function_call.method_name, "ops_stake_stop_finalize");
//                         }
//                         _ => panic!("expected FunctionCallAction"),
//                     };
//                 }
//             }
//         }
//     }
//
//     #[cfg(test)]
//     mod tests_unstake_online {
//         use super::*;
//
//         #[test]
//         fn specified_amount() {
//             let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);
//
//             // Arrange
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//             // stake
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 ctx.attached_deposit = YOCTO;
//                 testing_env!(ctx);
//                 // Act
//                 if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
//                     panic!("expected Promise")
//                 }
//                 println!("staked {:#?}", StakingPoolBalances::load());
//             }
//             // finalize stake
//             {
//                 let state = StakingPoolComponent::state();
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = 0;
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 ctx.account_locked_balance = *state.total_staked_balance;
//                 testing_env_with_promise_result_success(ctx.clone());
//                 let balances = staking_pool.ops_stake_finalize(ACCOUNT.to_string());
//                 println!("{:#?}", balances);
//                 println!("ops_stake_finalize {:#?}", StakingPoolBalances::load());
//             }
//             let state = *StakingPoolComponent::state();
//             println!(
//                 "before unstaking {}",
//                 serde_json::to_string_pretty(&state).unwrap()
//             );
//             // Act
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.account_locked_balance = *StakingPoolBalances::load().total_staked;
//                 testing_env!(ctx);
//                 if let PromiseOrValue::Value(_) = staking_pool.ops_unstake(Some(1000.into())) {
//                     panic!("expected Promise");
//                 }
//                 println!("ops_unstake {:#?}", StakingPoolBalances::load());
//                 let logs = test_utils::get_logs();
//                 println!("{:#?}", logs);
//                 assert_eq!(
//                     logs,
//                     vec![
//                         "[INFO] [UNSTAKE] near_amount=1000, stake_token_amount=1000",
//                         "[INFO] [FT_LOCK] account: bob, amount: 1000",
//                     ]
//                 );
//                 assert_eq!(ft_stake().ft_locked_balance(ACCOUNT).unwrap(), 1000.into());
//                 let stake_balance = YOCTO - *(state.staking_fee * YOCTO.into());
//                 assert_eq!(
//                     ft_stake().ft_balance_of(to_valid_account_id(ACCOUNT)),
//                     (stake_balance - 1000).into()
//                 );
//                 let state = *StakingPoolComponent::state();
//                 println!("{}", serde_json::to_string_pretty(&state).unwrap());
//                 assert_eq!(state.total_staked_balance, (YOCTO - 1000).into());
//
//                 let receipts = deserialize_receipts();
//                 assert_eq!(receipts.len(), 2);
//                 {
//                     let receipt = &receipts[0];
//                     assert_eq!(receipt.receiver_id, env::current_account_id());
//                     assert_eq!(receipt.actions.len(), 1);
//                     let action = &receipt.actions[0];
//                     match action {
//                         Action::Stake(action) => {
//                             assert_eq!(action.stake, YOCTO - 1000);
//                         }
//                         _ => panic!("expected StakeAction"),
//                     }
//                 }
//                 {
//                     let receipt = &receipts[1];
//                     assert_eq!(receipt.receiver_id, env::current_account_id());
//                     assert_eq!(receipt.actions.len(), 1);
//                     let action = &receipt.actions[0];
//                     match action {
//                         Action::FunctionCall(f) => {
//                             assert_eq!(f.method_name, "ops_unstake_finalize");
//                             let args: StakeActionCallbackArgs =
//                                 serde_json::from_str(&f.args).unwrap();
//                             assert_eq!(args.account_id, ACCOUNT);
//                         }
//                         _ => panic!("expected FunctionCall"),
//                     }
//                 }
//             }
//         }
//
//         #[test]
//         fn total_account_staked_balance() {
//             let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);
//
//             // Arrange
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//             // stake
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 ctx.attached_deposit = YOCTO;
//                 testing_env!(ctx);
//                 // Act
//                 if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
//                     panic!("expected Promise")
//                 }
//             }
//             // finalize stake
//             {
//                 let state = StakingPoolComponent::state();
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = 0;
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 ctx.account_locked_balance = *state.total_staked_balance;
//                 testing_env_with_promise_result_success(ctx.clone());
//                 let balances = staking_pool.ops_stake_finalize(ACCOUNT.to_string());
//                 println!("{:#?}", balances);
//             }
//             let state = *StakingPoolComponent::state();
//             println!(
//                 "before unstaking {}",
//                 serde_json::to_string_pretty(&state).unwrap()
//             );
//             // Act
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.account_locked_balance = *StakingPoolBalances::load().total_staked;
//                 testing_env!(ctx);
//                 if let PromiseOrValue::Value(_) = staking_pool.ops_unstake(None) {
//                     panic!("expected Promise");
//                 }
//                 let logs = test_utils::get_logs();
//                 println!("{:#?}", logs);
//                 assert_eq!(
//                     logs,
//                     vec![
//                         "[INFO] [UNSTAKE] near_amount=992000000000000000000000, stake_token_amount=992000000000000000000000",
//                         "[INFO] [FT_LOCK] account: bob, amount: 992000000000000000000000",
//                     ]
//                 );
//                 let expected_locked_balance = YOCTO - *(state.staking_fee * YOCTO.into());
//                 assert_eq!(
//                     ft_stake().ft_locked_balance(ACCOUNT).unwrap(),
//                     expected_locked_balance.into()
//                 );
//                 assert_eq!(
//                     ft_stake().ft_balance_of(to_valid_account_id(ACCOUNT)),
//                     0.into()
//                 );
//                 let state = *StakingPoolComponent::state();
//                 println!("{}", serde_json::to_string_pretty(&state).unwrap());
//                 assert_eq!(state.total_staked_balance, state.treasury_balance);
//                 assert_eq!(
//                     State::total_unstaked_balance(),
//                     expected_locked_balance.into()
//                 );
//
//                 let receipts = deserialize_receipts();
//                 assert_eq!(receipts.len(), 2);
//                 {
//                     let receipt = &receipts[0];
//                     assert_eq!(receipt.receiver_id, env::current_account_id());
//                     assert_eq!(receipt.actions.len(), 1);
//                     let action = &receipt.actions[0];
//                     match action {
//                         Action::Stake(action) => {
//                             assert_eq!(action.stake, *(state.staking_fee * YOCTO.into()));
//                         }
//                         _ => panic!("expected StakeAction"),
//                     }
//                 }
//                 {
//                     let receipt = &receipts[1];
//                     assert_eq!(receipt.receiver_id, env::current_account_id());
//                     assert_eq!(receipt.actions.len(), 1);
//                     let action = &receipt.actions[0];
//                     match action {
//                         Action::FunctionCall(f) => {
//                             assert_eq!(f.method_name, "ops_unstake_finalize");
//                             let args: StakeActionCallbackArgs =
//                                 serde_json::from_str(&f.args).unwrap();
//                             assert_eq!(args.account_id, ACCOUNT);
//                         }
//                         _ => panic!("expected FunctionCall"),
//                     }
//                 }
//             }
//         }
//
//         #[test]
//         #[should_panic(expected = "[ERR] [INSUFFICIENT_FUNDS]")]
//         fn insufficient_staked_funds() {
//             let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//
//             // Act
//             testing_env!(ctx.clone());
//             staking_pool.ops_unstake(Some(YOCTO.into()));
//         }
//
//         #[test]
//         #[should_panic(expected = "[ERR] [ACCOUNT_NOT_REGISTERED]")]
//         fn account_not_registered() {
//             let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, false);
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//             testing_env!(ctx.clone());
//             staking_pool.ops_unstake(None);
//         }
//     }
//
//     #[cfg(test)]
//     mod tests_unstake_finalize {
//         use super::*;
//
//         #[test]
//         fn stake_action_success() {
//             let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);
//
//             // Arrange
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//             // stake 3 NEAR
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = 3 * YOCTO;
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 testing_env!(ctx.clone());
//                 if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
//                     panic!("expected Promise")
//                 }
//             }
//             let logs = test_utils::get_logs();
//             println!("{:#?}", logs);
//
//             // finalize stake
//             {
//                 let state = StakingPoolComponent::state();
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = 0;
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 ctx.account_locked_balance = 3 * YOCTO;
//                 testing_env_with_promise_result_success(ctx.clone());
//                 let balances = staking_pool.ops_stake_finalize(ACCOUNT.to_string());
//                 // Assert
//                 let logs = test_utils::get_logs();
//                 println!("ops_stake_finalize: {:#?}", logs);
//                 println!("{:#?}", balances);
//                 {
//                     let staked_balance = balances.staked.unwrap();
//                     let expected_amount = (3 * YOCTO) - *(state.staking_fee * (3 * YOCTO).into());
//                     assert_eq!(staked_balance.stake, expected_amount.into());
//                     assert_eq!(staked_balance.near_value, expected_amount.into());
//                 }
//                 assert!(balances.unstaked.is_none());
//                 let state = *StakingPoolComponent::state();
//                 println!("{}", serde_json::to_string_pretty(&state).unwrap());
//                 assert_eq!(state.total_staked_balance, (3 * YOCTO).into());
//             }
//
//             // unstake 2 NEAR
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 ctx.account_locked_balance = *staking_pool.ops_stake_pool_balances().total_staked;
//                 testing_env!(ctx.clone());
//                 if let PromiseOrValue::Value(_) = staking_pool.ops_unstake(Some((2 * YOCTO).into()))
//                 {
//                     panic!("expected Promise")
//                 }
//                 let logs = test_utils::get_logs();
//                 println!("{:#?}", logs);
//             }
//
//             // finalize the unstaked funds
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 ctx.account_locked_balance = *staking_pool.ops_stake_pool_balances().total_staked;
//                 testing_env_with_promise_result_success(ctx);
//                 let stake_account_balances = staking_pool.ops_unstake_finalize(ACCOUNT.to_string());
//                 let logs = test_utils::get_logs();
//                 println!("{:#?}", logs);
//                 assert_eq!(
//                     logs,
//                     vec![
//                         "[INFO] [FT_BURN] account: bob, amount: 2000000000000000000000000",
//                         "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(184)",
//                     ]
//                 );
//                 assert_eq!(
//                     stake_account_balances.staked.as_ref().unwrap().stake,
//                     (YOCTO - *StakingPoolComponent::state().treasury_balance).into() // subtract staking fees
//                 );
//                 assert_eq!(
//                     stake_account_balances.unstaked.as_ref().unwrap().total,
//                     (2 * YOCTO).into()
//                 );
//                 assert_eq!(
//                     stake_account_balances.unstaked.as_ref().unwrap().available,
//                     YoctoNear::ZERO
//                 );
//             }
//         }
//
//         #[test]
//         fn stake_action_failed() {
//             let (ctx, mut staking_pool) = deploy(OWNER, ADMIN, ACCOUNT, true);
//
//             // Arrange
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//             // stake 3 NEAR
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = 3 * YOCTO;
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 testing_env!(ctx.clone());
//                 if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
//                     panic!("expected Promise")
//                 }
//             }
//             let logs = test_utils::get_logs();
//             println!("{:#?}", logs);
//
//             // finalize stake
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = 0;
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 ctx.account_locked_balance = 3 * YOCTO;
//                 testing_env_with_promise_result_success(ctx.clone());
//                 staking_pool.ops_stake_finalize(ACCOUNT.to_string());
//                 // Assert
//                 let logs = test_utils::get_logs();
//                 println!("ops_stake_finalize: {:#?}", logs);
//             }
//
//             // unstake 2 NEAR
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 ctx.account_locked_balance = *staking_pool.ops_stake_pool_balances().total_staked;
//                 testing_env!(ctx.clone());
//                 if let PromiseOrValue::Value(_) = staking_pool.ops_unstake(Some((2 * YOCTO).into()))
//                 {
//                     panic!("expected Promise")
//                 }
//                 let logs = test_utils::get_logs();
//                 println!("{:#?}", logs);
//             }
//
//             // finalize the unstaked funds
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 ctx.account_locked_balance = *staking_pool.ops_stake_pool_balances().total_staked;
//                 testing_env_with_promise_result_failure(ctx);
//                 let stake_account_balances = staking_pool.ops_unstake_finalize(ACCOUNT.to_string());
//                 let logs = test_utils::get_logs();
//                 println!("{:#?}", logs);
//                 assert_eq!(
//                     logs,
//                     vec![
//                         "[ERR] [STAKE_ACTION_FAILED] ",
//                         "[WARN] [STATUS_OFFLINE] StakeActionFailed",
//                         "[INFO] [FT_BURN] account: bob, amount: 2000000000000000000000000",
//                         "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(184)",
//                     ]
//                 );
//                 assert_eq!(
//                     stake_account_balances.staked.as_ref().unwrap().stake,
//                     (YOCTO - *StakingPoolComponent::state().treasury_balance).into() // subtract staking fees
//                 );
//                 assert_eq!(
//                     stake_account_balances.unstaked.as_ref().unwrap().total,
//                     (2 * YOCTO).into()
//                 );
//                 assert_eq!(
//                     stake_account_balances.unstaked.as_ref().unwrap().available,
//                     YoctoNear::ZERO
//                 );
//                 assert_eq!(
//                     staking_pool.ops_stake_status(),
//                     Status::Offline(OfflineReason::StakeActionFailed)
//                 );
//             }
//         }
//     }
//
//     #[cfg(test)]
//     mod tests_restake_online {
//         use super::*;
//
//         #[test]
//         fn all_with_unstaked_funds() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//             // stake
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = YOCTO;
//                 ctx.account_locked_balance = 0;
//                 testing_env!(ctx);
//                 println!("before staking {:#?}", StakingPoolBalances::load());
//                 staking_pool.ops_stake();
//                 println!("staked {:#?}", StakingPoolBalances::load());
//             }
//
//             // finalize stake
//             {
//                 let state = *StakingPoolComponent::state();
//                 println!("{}", serde_json::to_string_pretty(&state).unwrap());
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = 0;
//                 ctx.account_locked_balance = *state.total_staked_balance;
//                 testing_env_with_promise_result_success(ctx.clone());
//                 staking_pool.ops_stake_finalize(ctx.predecessor_account_id);
//                 println!("finalize stake {:#?}", StakingPoolBalances::load());
//                 let logs = test_utils::get_logs();
//                 println!("finalized stake: {:#?}", logs);
//             }
//
//             // unstake all
//             {
//                 let state = *StakingPoolComponent::state();
//                 println!("{}", serde_json::to_string_pretty(&state).unwrap());
//                 let mut ctx = ctx.clone();
//                 ctx.account_locked_balance = *state.total_staked_balance;
//                 testing_env!(ctx);
//                 staking_pool.ops_unstake(None);
//                 println!("unstaked {:#?}", StakingPoolBalances::load());
//                 let logs = test_utils::get_logs();
//                 println!("unstake: {:#?}", logs);
//             }
//
//             // finalize unstaking
//             {
//                 let state = *StakingPoolComponent::state();
//                 println!("{}", serde_json::to_string_pretty(&state).unwrap());
//                 let mut ctx = ctx.clone();
//                 ctx.account_locked_balance = *state.treasury_balance;
//                 testing_env_with_promise_result_success(ctx.clone());
//                 staking_pool.ops_unstake_finalize(ctx.predecessor_account_id);
//                 println!("finalized unstaking {:#?}", StakingPoolBalances::load());
//                 let logs = test_utils::get_logs();
//                 println!("finalized unstake {:#?}", logs);
//                 let state = *StakingPoolComponent::state();
//                 println!("{}", serde_json::to_string_pretty(&state).unwrap());
//             }
//
//             assert_eq!(
//                 ft_stake().ft_balance_of(to_valid_account_id(ACCOUNT)),
//                 TokenAmount::ZERO
//             );
//
//             // Act - restake
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = YOCTO;
//                 testing_env!(ctx);
//                 let account = account_manager().registered_account_data(ACCOUNT);
//                 assert!(account.unstaked_balances.locked().is_some());
//                 staking_pool.ops_restake(None);
//                 println!("restaked {:#?}", StakingPoolBalances::load());
//                 let logs = test_utils::get_logs();
//                 println!("restaked all: {:#?}", logs);
//                 assert_eq!(logs, vec![
//                     "[INFO] [STAKE] near_amount=992000000000000000000000, stake_token_amount=992000000000000000000000",
//                 ]);
//                 let account = account_manager().registered_account_data(ACCOUNT);
//                 assert!(account.unstaked_balances.locked().is_none());
//             }
//         }
//
//         #[test]
//         fn all_with_zero_unstaked_funds() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//
//             // stake
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = YOCTO;
//                 testing_env!(ctx);
//                 staking_pool.ops_stake();
//             }
//             // finalize stake
//             let balances = {
//                 let state = *StakingPoolComponent::state();
//                 println!("{}", serde_json::to_string_pretty(&state).unwrap());
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = 0;
//                 ctx.account_locked_balance = *state.total_staked_balance;
//                 testing_env_with_promise_result_success(ctx.clone());
//                 let balances = staking_pool.ops_stake_finalize(ctx.predecessor_account_id);
//                 let logs = test_utils::get_logs();
//                 println!("finalized stake: {:#?}", logs);
//                 balances
//             };
//
//             // Act
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = YOCTO;
//                 testing_env!(ctx);
//                 if let PromiseOrValue::Value(balances_after_restaking) =
//                     staking_pool.ops_restake(None)
//                 {
//                     assert_eq!(balances_after_restaking, balances);
//                 } else {
//                     panic!("expected Value");
//                 }
//                 assert!(test_utils::get_logs().is_empty());
//             }
//         }
//
//         #[test]
//         fn all_with_zero_staked_and_unstaked_funds() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//             if let PromiseOrValue::Value(balance) = staking_pool.ops_restake(None) {
//                 assert!(balance.staked.is_none());
//                 assert!(balance.unstaked.is_none());
//             } else {
//                 panic!("expected Value");
//             }
//         }
//
//         #[test]
//         fn partial_with_unstaked_funds() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//             // stake
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = YOCTO;
//                 testing_env!(ctx);
//                 staking_pool.ops_stake();
//             }
//             // finalize stake
//             {
//                 let state = *StakingPoolComponent::state();
//                 println!("{}", serde_json::to_string_pretty(&state).unwrap());
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = 0;
//                 ctx.account_locked_balance = *state.total_staked_balance;
//                 testing_env_with_promise_result_success(ctx.clone());
//                 staking_pool.ops_stake_finalize(ctx.predecessor_account_id);
//                 let logs = test_utils::get_logs();
//                 println!("finalized stake: {:#?}", logs);
//             }
//             // unstake all
//             {
//                 let state = *StakingPoolComponent::state();
//                 println!("{}", serde_json::to_string_pretty(&state).unwrap());
//                 let mut ctx = ctx.clone();
//                 ctx.account_locked_balance = *state.total_staked_balance;
//                 testing_env!(ctx);
//                 staking_pool.ops_unstake(None);
//                 let logs = test_utils::get_logs();
//                 println!("unstake: {:#?}", logs);
//             }
//             // finalize unstaking
//             {
//                 let state = *StakingPoolComponent::state();
//                 println!("{}", serde_json::to_string_pretty(&state).unwrap());
//                 let mut ctx = ctx.clone();
//                 ctx.account_locked_balance = *state.treasury_balance;
//                 testing_env_with_promise_result_success(ctx.clone());
//                 staking_pool.ops_unstake_finalize(ctx.predecessor_account_id);
//                 let logs = test_utils::get_logs();
//                 println!("finalized unstake {:#?}", logs);
//                 let state = *StakingPoolComponent::state();
//                 println!("{}", serde_json::to_string_pretty(&state).unwrap());
//             }
//             assert_eq!(
//                 ft_stake().ft_balance_of(to_valid_account_id(ACCOUNT)),
//                 TokenAmount::ZERO
//             );
//
//             // Act - restake
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = YOCTO;
//                 testing_env!(ctx);
//                 let account_before_restaking = account_manager().registered_account_data(ACCOUNT);
//                 assert!(account_before_restaking
//                     .unstaked_balances
//                     .locked()
//                     .is_some());
//                 staking_pool.ops_restake(Some(10000.into()));
//                 let logs = test_utils::get_logs();
//                 println!("restaked: {:#?}", logs);
//                 assert_eq!(
//                     logs,
//                     vec!["[INFO] [STAKE] near_amount=10000, stake_token_amount=10000",]
//                 );
//                 let account = account_manager().registered_account_data(ACCOUNT);
//                 assert_eq!(
//                     account.unstaked_balances.locked_balance(),
//                     account_before_restaking.unstaked_balances.locked_balance() - 10000
//                 );
//             }
//         }
//
//         #[test]
//         #[should_panic(expected = "[ERR] [INSUFFICIENT_FUNDS]")]
//         fn partial_insufficient_unstaked_funds() {
//             let (mut ctx, mut staking_pool) = deploy_with_registered_account();
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx);
//             staking_pool.ops_restake(Some(YOCTO.into()));
//         }
//
//         #[test]
//         #[should_panic(expected = "[ERR] [ACCOUNT_NOT_REGISTERED]")]
//         fn all_with_unregistered_account() {
//             let (mut ctx, mut staking_pool) = deploy_with_unregistered_account();
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx);
//             staking_pool.ops_restake(None);
//         }
//
//         #[test]
//         #[should_panic(expected = "[ERR] [ACCOUNT_NOT_REGISTERED]")]
//         fn partial_with_unregistered_account() {
//             let (mut ctx, mut staking_pool) = deploy_with_unregistered_account();
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx);
//             staking_pool.ops_restake(Some(YOCTO.into()));
//         }
//     }
//
//     #[cfg(test)]
//     mod tests_restake_offline {
//         use super::*;
//
//         #[test]
//         fn all_with_unstaked_funds() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//             // stake
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = YOCTO;
//                 testing_env!(ctx);
//                 staking_pool.ops_stake();
//             }
//             // unstake all
//             {
//                 let state = *StakingPoolComponent::state();
//                 println!("{}", serde_json::to_string_pretty(&state).unwrap());
//                 let mut ctx = ctx.clone();
//                 ctx.account_locked_balance = *state.total_staked_balance;
//                 testing_env!(ctx);
//                 staking_pool.ops_unstake(None);
//                 let logs = test_utils::get_logs();
//                 println!("unstake: {:#?}", logs);
//             }
//
//             // Act - restake
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = YOCTO;
//                 testing_env!(ctx);
//                 let account = account_manager().registered_account_data(ACCOUNT);
//                 assert!(account.unstaked_balances.locked().is_some());
//                 staking_pool.ops_restake(None);
//                 let account = account_manager().registered_account_data(ACCOUNT);
//                 assert!(account.unstaked_balances.locked().is_none());
//
//                 let logs = test_utils::get_logs();
//                 println!("restaked all: {:#?}", logs);
//                 assert_eq!(logs, vec![
//                     "[INFO] [STAKE] near_amount=992000000000000000000000, stake_token_amount=992000000000000000000000",
//                     "[WARN] [STATUS_OFFLINE] ",
//                     "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
//                     "[INFO] [FT_MINT] account: bob, amount: 992000000000000000000000",
//                     "[INFO] [FT_BURN] account: bob, amount: 7936000000000000000000",
//                     "[INFO] [FT_MINT] account: contract.near, amount: 7936000000000000000000",
//                 ]);
//             }
//         }
//
//         #[test]
//         fn all_with_zero_unstaked_funds() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//
//             // stake
//             let balances = {
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = YOCTO;
//                 testing_env!(ctx);
//                 if let PromiseOrValue::Value(balances) = staking_pool.ops_stake() {
//                     balances
//                 } else {
//                     unreachable!();
//                 }
//             };
//
//             // Act
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = YOCTO;
//                 testing_env!(ctx);
//                 if let PromiseOrValue::Value(balances_after_restaking) =
//                     staking_pool.ops_restake(None)
//                 {
//                     assert_eq!(balances_after_restaking, balances);
//                 } else {
//                     panic!("expected Value");
//                 }
//                 assert!(test_utils::get_logs().is_empty());
//             }
//         }
//
//         #[test]
//         fn all_with_zero_staked_and_unstaked_funds() {
//             let (_ctx, mut staking_pool) = deploy_with_registered_account();
//             if let PromiseOrValue::Value(balance) = staking_pool.ops_restake(None) {
//                 assert!(balance.staked.is_none());
//                 assert!(balance.unstaked.is_none());
//             } else {
//                 panic!("expected Value");
//             }
//         }
//
//         #[test]
//         fn partial_with_unstaked_funds() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//             // stake
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = YOCTO;
//                 testing_env!(ctx);
//                 staking_pool.ops_stake();
//             }
//             // unstake all
//             {
//                 let state = *StakingPoolComponent::state();
//                 println!("{}", serde_json::to_string_pretty(&state).unwrap());
//                 let mut ctx = ctx.clone();
//                 ctx.account_locked_balance = *state.total_staked_balance;
//                 testing_env!(ctx);
//                 staking_pool.ops_unstake(None);
//                 let logs = test_utils::get_logs();
//                 println!("unstake: {:#?}", logs);
//             }
//             assert_eq!(
//                 ft_stake().ft_balance_of(to_valid_account_id(ACCOUNT)),
//                 TokenAmount::ZERO
//             );
//
//             // Act - restake
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = YOCTO;
//                 testing_env!(ctx);
//                 let account_before_restaking = account_manager().registered_account_data(ACCOUNT);
//                 assert!(account_before_restaking
//                     .unstaked_balances
//                     .locked()
//                     .is_some());
//                 staking_pool.ops_restake(Some(10000.into()));
//                 let logs = test_utils::get_logs();
//                 println!("restaked: {:#?}", logs);
//                 assert_eq!(
//                     logs,
//                     vec![
//                         "[INFO] [STAKE] near_amount=10000, stake_token_amount=10000",
//                         "[WARN] [STATUS_OFFLINE] ",
//                         "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
//                         "[INFO] [FT_MINT] account: bob, amount: 10000",
//                         "[INFO] [FT_BURN] account: bob, amount: 80",
//                         "[INFO] [FT_MINT] account: contract.near, amount: 80",
//                     ]
//                 );
//                 let account = account_manager().registered_account_data(ACCOUNT);
//                 assert_eq!(
//                     account.unstaked_balances.locked_balance(),
//                     account_before_restaking.unstaked_balances.locked_balance() - 10000
//                 );
//             }
//         }
//
//         #[test]
//         #[should_panic(expected = "[ERR] [INSUFFICIENT_FUNDS]")]
//         fn partial_insufficient_unstaked_funds() {
//             let (mut ctx, mut staking_pool) = deploy_with_registered_account();
//
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx);
//             staking_pool.ops_restake(Some(YOCTO.into()));
//         }
//
//         #[test]
//         #[should_panic(expected = "[ERR] [ACCOUNT_NOT_REGISTERED]")]
//         fn all_with_unregistered_account() {
//             let (mut ctx, mut staking_pool) = deploy_with_unregistered_account();
//
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx);
//             staking_pool.ops_restake(None);
//         }
//
//         #[test]
//         #[should_panic(expected = "[ERR] [ACCOUNT_NOT_REGISTERED]")]
//         fn partial_with_unregistered_account() {
//             let (mut ctx, mut staking_pool) = deploy_with_unregistered_account();
//
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx);
//             staking_pool.ops_restake(Some(YOCTO.into()));
//         }
//     }
//
//     #[cfg(test)]
//     mod tests_withdraw_online {
//         use super::*;
//
//         fn stake_unstake(
//             ctx: VMContext,
//             staking_pool: &mut StakingPoolComponent,
//             stake_amount: YoctoNear,
//             unstake_amount: YoctoNear,
//         ) {
//             let stake_amount = *stake_amount;
//             if stake_amount == 0 {
//                 return;
//             }
//             let unstake_amount = *unstake_amount;
//             // stake
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = stake_amount;
//                 testing_env!(ctx);
//                 staking_pool.ops_stake();
//             }
//             // finalize stake
//             {
//                 let state = *StakingPoolComponent::state();
//                 println!("{}", serde_json::to_string_pretty(&state).unwrap());
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = 0;
//                 ctx.account_locked_balance = *state.total_staked_balance;
//                 testing_env_with_promise_result_success(ctx.clone());
//                 staking_pool.ops_stake_finalize(ctx.predecessor_account_id);
//                 let logs = test_utils::get_logs();
//                 println!("finalized stake: {:#?}", logs);
//             }
//             if unstake_amount > 0 {
//                 // unstake
//                 {
//                     let state = *StakingPoolComponent::state();
//                     println!("{}", serde_json::to_string_pretty(&state).unwrap());
//                     let mut ctx = ctx.clone();
//                     ctx.account_locked_balance = *state.total_staked_balance;
//                     testing_env!(ctx);
//                     staking_pool.ops_unstake(Some((unstake_amount).into()));
//                     let logs = test_utils::get_logs();
//                     println!("unstake: {:#?}", logs);
//                 }
//                 // finalize unstaking
//                 {
//                     let state = *StakingPoolComponent::state();
//                     println!("{}", serde_json::to_string_pretty(&state).unwrap());
//                     let mut ctx = ctx.clone();
//                     let total_staked_balance = state.treasury_balance
//                         + (stake_amount - *state.treasury_balance - unstake_amount);
//                     ctx.account_locked_balance = *total_staked_balance;
//                     testing_env_with_promise_result_success(ctx.clone());
//                     staking_pool.ops_unstake_finalize(ctx.predecessor_account_id);
//                     let logs = test_utils::get_logs();
//                     println!("finalized unstake {:#?}", logs);
//                     let state = *StakingPoolComponent::state();
//                     println!("{}", serde_json::to_string_pretty(&state).unwrap());
//                 }
//             }
//         }
//
//         #[test]
//         fn all_with_available_unstaked_funds() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//
//             stake_unstake(
//                 ctx.clone(),
//                 &mut staking_pool,
//                 YOCTO.into(),
//                 (YOCTO / 2).into(),
//             );
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             ctx.epoch_height = ctx.epoch_height + 4;
//             testing_env!(ctx.clone());
//             let starting_balances = staking_pool
//                 .ops_stake_balance(to_valid_account_id(ACCOUNT))
//                 .unwrap();
//             assert_eq!(
//                 starting_balances.unstaked.as_ref().unwrap().total,
//                 starting_balances.unstaked.as_ref().unwrap().available
//             );
//             let balances = staking_pool.ops_stake_withdraw(None);
//             println!("{:#?}", balances);
//             assert!(balances.unstaked.is_none());
//
//             let receipts = deserialize_receipts();
//             assert_eq!(receipts.len(), 1);
//             let receipt = &receipts[0];
//             assert_eq!(receipt.receiver_id, ACCOUNT.to_string());
//             assert_eq!(receipt.actions.len(), 1);
//             let action = &receipt.actions[0];
//             if let Action::Transfer(transfer) = action {
//                 assert_eq!(transfer.deposit, YOCTO / 2);
//             } else {
//                 panic!("expected transfer action");
//             }
//         }
//
//         #[test]
//         fn all_with_locked_unstaked_funds_and_zero_available() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//
//             stake_unstake(
//                 ctx.clone(),
//                 &mut staking_pool,
//                 YOCTO.into(),
//                 (YOCTO / 2).into(),
//             );
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx.clone());
//             let starting_balances = staking_pool
//                 .ops_stake_balance(to_valid_account_id(ACCOUNT))
//                 .unwrap();
//             assert_eq!(
//                 starting_balances.unstaked.as_ref().unwrap().available,
//                 YoctoNear::ZERO
//             );
//             let balances = staking_pool.ops_stake_withdraw(None);
//             assert_eq!(balances, starting_balances);
//             assert!(deserialize_receipts().is_empty())
//         }
//
//         #[test]
//         fn all_with_locked_unstaked_funds_with_liquidity_fully_available() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//
//             stake_unstake(
//                 ctx.clone(),
//                 &mut staking_pool,
//                 YOCTO.into(),
//                 (YOCTO / 2).into(),
//             );
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx.clone());
//             State::add_liquidity((YOCTO * 3 / 4).into());
//             assert_eq!(State::liquidity(), (YOCTO / 2).into());
//             let starting_balances = staking_pool
//                 .ops_stake_balance(to_valid_account_id(ACCOUNT))
//                 .unwrap();
//             assert_eq!(
//                 starting_balances.unstaked.as_ref().unwrap().available,
//                 YoctoNear::ZERO
//             );
//             let balances = staking_pool.ops_stake_withdraw(None);
//             println!("{:#?}", balances);
//             assert!(balances.unstaked.is_none());
//
//             let receipts = deserialize_receipts();
//             assert_eq!(receipts.len(), 1);
//             let receipt = &receipts[0];
//             assert_eq!(receipt.receiver_id, ACCOUNT.to_string());
//             assert_eq!(receipt.actions.len(), 1);
//             let action = &receipt.actions[0];
//             if let Action::Transfer(transfer) = action {
//                 assert_eq!(transfer.deposit, YOCTO / 2);
//             } else {
//                 panic!("expected transfer action");
//             }
//             assert_eq!(State::liquidity(), YoctoNear::ZERO);
//         }
//
//         #[test]
//         fn all_with_locked_unstaked_funds_with_liquidity_partially_available_with_unlocked_funds() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//
//             stake_unstake(
//                 ctx.clone(),
//                 &mut staking_pool,
//                 YOCTO.into(),
//                 (YOCTO / 2).into(),
//             );
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx.clone());
//             State::add_liquidity((YOCTO / 4).into());
//             assert_eq!(State::liquidity(), (YOCTO / 4).into());
//             let starting_balances = staking_pool
//                 .ops_stake_balance(to_valid_account_id(ACCOUNT))
//                 .unwrap();
//             assert_eq!(
//                 starting_balances.unstaked.as_ref().unwrap().available,
//                 YoctoNear::ZERO
//             );
//
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             ctx.epoch_height += 4;
//             testing_env!(ctx.clone());
//             let balances = staking_pool.ops_stake_withdraw(None);
//             println!("{:#?}", balances);
//             assert!(balances.unstaked.is_none());
//
//             let receipts = deserialize_receipts();
//             assert_eq!(receipts.len(), 1);
//             let receipt = &receipts[0];
//             assert_eq!(receipt.receiver_id, ACCOUNT.to_string());
//             assert_eq!(receipt.actions.len(), 1);
//             let action = &receipt.actions[0];
//             if let Action::Transfer(transfer) = action {
//                 assert_eq!(transfer.deposit, YOCTO / 2);
//             } else {
//                 panic!("expected transfer action");
//             }
//             assert_eq!(State::liquidity(), YoctoNear::ZERO);
//         }
//
//         #[test]
//         fn all_with_zero_unstaked_funds() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//
//             stake_unstake(ctx.clone(), &mut staking_pool, YOCTO.into(), 0.into());
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx.clone());
//
//             let starting_balances = staking_pool
//                 .ops_stake_balance(to_valid_account_id(ACCOUNT))
//                 .unwrap();
//
//             let balances = staking_pool.ops_stake_withdraw(None);
//             assert_eq!(balances, starting_balances);
//
//             assert!(test_utils::get_logs().is_empty());
//             assert!(deserialize_receipts().is_empty());
//         }
//
//         #[test]
//         fn all_with_zero_staked_and_unstaked_funds() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//
//             stake_unstake(ctx.clone(), &mut staking_pool, 0.into(), 0.into());
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx.clone());
//
//             let starting_balances = staking_pool
//                 .ops_stake_balance(to_valid_account_id(ACCOUNT))
//                 .unwrap();
//
//             let balances = staking_pool.ops_stake_withdraw(None);
//             assert_eq!(balances, starting_balances);
//
//             assert!(test_utils::get_logs().is_empty());
//             assert!(deserialize_receipts().is_empty());
//         }
//
//         #[test]
//         #[should_panic(expected = "[ERR] [ACCOUNT_NOT_REGISTERED]")]
//         fn all_with_unregistered_account() {
//             let (ctx, mut staking_pool) = deploy_with_unregistered_account();
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx);
//             staking_pool.ops_stake_withdraw(None);
//         }
//
//         #[test]
//         fn partial_with_available_unstaked_funds() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//
//             stake_unstake(
//                 ctx.clone(),
//                 &mut staking_pool,
//                 YOCTO.into(),
//                 (YOCTO / 2).into(),
//             );
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             ctx.epoch_height = ctx.epoch_height + 4;
//             testing_env!(ctx.clone());
//             let starting_balances = staking_pool
//                 .ops_stake_balance(to_valid_account_id(ACCOUNT))
//                 .unwrap();
//             assert_eq!(
//                 starting_balances.unstaked.as_ref().unwrap().total,
//                 starting_balances.unstaked.as_ref().unwrap().available
//             );
//             let balances = staking_pool.ops_stake_withdraw(Some((YOCTO / 8).into()));
//             println!("{:#?}", balances);
//             assert_eq!(balances.unstaked.unwrap().available, (YOCTO * 3 / 8).into());
//
//             let receipts = deserialize_receipts();
//             assert_eq!(receipts.len(), 1);
//             let receipt = &receipts[0];
//             assert_eq!(receipt.receiver_id, ACCOUNT.to_string());
//             assert_eq!(receipt.actions.len(), 1);
//             let action = &receipt.actions[0];
//             if let Action::Transfer(transfer) = action {
//                 assert_eq!(transfer.deposit, YOCTO / 8);
//             } else {
//                 panic!("expected transfer action");
//             }
//         }
//
//         #[test]
//         #[should_panic(expected = "[ERR] [INSUFFICIENT_FUNDS]")]
//         fn partial_with_locked_unstaked_funds_and_zero_available() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//
//             stake_unstake(
//                 ctx.clone(),
//                 &mut staking_pool,
//                 YOCTO.into(),
//                 (YOCTO / 2).into(),
//             );
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx.clone());
//             staking_pool.ops_stake_withdraw(Some((YOCTO / 8).into()));
//         }
//
//         #[test]
//         fn partial_with_locked_unstaked_funds_with_liquidity_fully_available() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//
//             stake_unstake(
//                 ctx.clone(),
//                 &mut staking_pool,
//                 YOCTO.into(),
//                 (YOCTO / 2).into(),
//             );
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx.clone());
//             State::add_liquidity(YOCTO.into());
//             assert_eq!(State::liquidity(), (YOCTO / 2).into());
//             let starting_balances = staking_pool
//                 .ops_stake_balance(to_valid_account_id(ACCOUNT))
//                 .unwrap();
//             assert_eq!(
//                 starting_balances.unstaked.as_ref().unwrap().available,
//                 YoctoNear::ZERO
//             );
//             let balances = staking_pool.ops_stake_withdraw(Some((YOCTO / 8).into()));
//             println!("{:#?}", balances);
//             assert_eq!(balances.unstaked.unwrap().total, (YOCTO * 3 / 8).into());
//
//             let receipts = deserialize_receipts();
//             assert_eq!(receipts.len(), 1);
//             let receipt = &receipts[0];
//             assert_eq!(receipt.receiver_id, ACCOUNT.to_string());
//             assert_eq!(receipt.actions.len(), 1);
//             let action = &receipt.actions[0];
//             if let Action::Transfer(transfer) = action {
//                 assert_eq!(transfer.deposit, YOCTO / 8);
//             } else {
//                 panic!("expected transfer action");
//             }
//             assert_eq!(State::liquidity(), (YOCTO * 3 / 8).into());
//         }
//
//         #[test]
//         fn partial_with_locked_unstaked_funds_with_liquidity_partially_available_and_unlocked_funds(
//         ) {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//
//             stake_unstake(
//                 ctx.clone(),
//                 &mut staking_pool,
//                 YOCTO.into(),
//                 (YOCTO / 2).into(),
//             );
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             ctx.epoch_height += 4;
//             testing_env!(ctx.clone());
//             State::add_liquidity((YOCTO / 16).into());
//             assert_eq!(State::liquidity(), (YOCTO / 16).into());
//             let starting_balances = staking_pool
//                 .ops_stake_balance(to_valid_account_id(ACCOUNT))
//                 .unwrap();
//             assert_eq!(
//                 starting_balances.unstaked.as_ref().unwrap().available,
//                 (YOCTO / 2).into()
//             );
//             let balances = staking_pool.ops_stake_withdraw(Some((YOCTO / 8).into()));
//             println!("{:#?}", balances);
//             assert_eq!(balances.unstaked.unwrap().total, (YOCTO * 3 / 8).into());
//
//             let receipts = deserialize_receipts();
//             assert_eq!(receipts.len(), 1);
//             let receipt = &receipts[0];
//             assert_eq!(receipt.receiver_id, ACCOUNT.to_string());
//             assert_eq!(receipt.actions.len(), 1);
//             let action = &receipt.actions[0];
//             if let Action::Transfer(transfer) = action {
//                 assert_eq!(transfer.deposit, YOCTO / 8);
//             } else {
//                 panic!("expected transfer action");
//             }
//             assert_eq!(State::liquidity(), YoctoNear::ZERO);
//         }
//
//         #[test]
//         #[should_panic(expected = "[ERR] [INSUFFICIENT_FUNDS]")]
//         fn partial_with_insufficient_funds() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//
//             stake_unstake(
//                 ctx.clone(),
//                 &mut staking_pool,
//                 YOCTO.into(),
//                 (YOCTO / 8).into(),
//             );
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx.clone());
//             staking_pool.ops_stake_withdraw(Some(YOCTO.into()));
//         }
//
//         #[test]
//         #[should_panic(expected = "[ERR] [INSUFFICIENT_FUNDS]")]
//         fn partial_with_zero_unstaked_funds() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//
//             stake_unstake(
//                 ctx.clone(),
//                 &mut staking_pool,
//                 YOCTO.into(),
//                 YoctoNear::ZERO,
//             );
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx.clone());
//             staking_pool.ops_stake_withdraw(Some((YOCTO / 8).into()));
//         }
//
//         #[test]
//         #[should_panic(expected = "[ERR] [INSUFFICIENT_FUNDS]")]
//         fn partial_with_zero_staked_and_unstaked_funds() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//
//             stake_unstake(
//                 ctx.clone(),
//                 &mut staking_pool,
//                 YoctoNear::ZERO,
//                 YoctoNear::ZERO,
//             );
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx.clone());
//             staking_pool.ops_stake_withdraw(Some((YOCTO / 8).into()));
//         }
//
//         #[test]
//         #[should_panic(expected = "[ERR] [ACCOUNT_NOT_REGISTERED]")]
//         fn partial_with_unregistered_account() {
//             let (ctx, mut staking_pool) = deploy_with_unregistered_account();
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx.clone());
//             staking_pool.ops_stake_withdraw(Some((YOCTO / 8).into()));
//         }
//     }
//
//     #[cfg(test)]
//     mod tests_withdraw_offline {
//         use super::*;
//
//         fn stake_unstake(
//             ctx: VMContext,
//             staking_pool: &mut StakingPoolComponent,
//             stake_amount: YoctoNear,
//             unstake_amount: YoctoNear,
//         ) {
//             let stake_amount = *stake_amount;
//             if stake_amount == 0 {
//                 return;
//             }
//             let unstake_amount = *unstake_amount;
//             // stake
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = stake_amount;
//                 testing_env!(ctx);
//                 staking_pool.ops_stake();
//             }
//             if unstake_amount > 0 {
//                 // unstake
//                 {
//                     let state = *StakingPoolComponent::state();
//                     println!("{}", serde_json::to_string_pretty(&state).unwrap());
//                     let mut ctx = ctx.clone();
//                     ctx.account_locked_balance = *state.total_staked_balance;
//                     testing_env!(ctx);
//                     staking_pool.ops_unstake(Some((unstake_amount).into()));
//                     let logs = test_utils::get_logs();
//                     println!("unstake: {:#?}", logs);
//                 }
//             }
//         }
//
//         #[test]
//         fn all_with_available_unstaked_funds() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//
//             stake_unstake(
//                 ctx.clone(),
//                 &mut staking_pool,
//                 YOCTO.into(),
//                 (YOCTO / 2).into(),
//             );
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             ctx.epoch_height = ctx.epoch_height + 4;
//             testing_env!(ctx.clone());
//             let starting_balances = staking_pool
//                 .ops_stake_balance(to_valid_account_id(ACCOUNT))
//                 .unwrap();
//             assert_eq!(
//                 starting_balances.unstaked.as_ref().unwrap().total,
//                 starting_balances.unstaked.as_ref().unwrap().available
//             );
//             let balances = staking_pool.ops_stake_withdraw(None);
//             println!("{:#?}", balances);
//             assert!(balances.unstaked.is_none());
//
//             let receipts = deserialize_receipts();
//             assert_eq!(receipts.len(), 1);
//             let receipt = &receipts[0];
//             assert_eq!(receipt.receiver_id, ACCOUNT.to_string());
//             assert_eq!(receipt.actions.len(), 1);
//             let action = &receipt.actions[0];
//             if let Action::Transfer(transfer) = action {
//                 assert_eq!(transfer.deposit, YOCTO / 2);
//             } else {
//                 panic!("expected transfer action");
//             }
//         }
//
//         #[test]
//         fn all_with_locked_unstaked_funds_and_zero_available() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//
//             stake_unstake(
//                 ctx.clone(),
//                 &mut staking_pool,
//                 YOCTO.into(),
//                 (YOCTO / 2).into(),
//             );
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx.clone());
//             let starting_balances = staking_pool
//                 .ops_stake_balance(to_valid_account_id(ACCOUNT))
//                 .unwrap();
//             assert_eq!(
//                 starting_balances.unstaked.as_ref().unwrap().available,
//                 YoctoNear::ZERO
//             );
//             let balances = staking_pool.ops_stake_withdraw(None);
//             assert_eq!(balances, starting_balances);
//             assert!(deserialize_receipts().is_empty())
//         }
//
//         #[test]
//         fn all_with_locked_unstaked_funds_with_liquidity_fully_available() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//
//             stake_unstake(
//                 ctx.clone(),
//                 &mut staking_pool,
//                 YOCTO.into(),
//                 (YOCTO / 2).into(),
//             );
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx.clone());
//             State::add_liquidity((YOCTO * 3 / 4).into());
//             assert_eq!(State::liquidity(), (YOCTO / 2).into());
//             let starting_balances = staking_pool
//                 .ops_stake_balance(to_valid_account_id(ACCOUNT))
//                 .unwrap();
//             assert_eq!(
//                 starting_balances.unstaked.as_ref().unwrap().available,
//                 YoctoNear::ZERO
//             );
//             let balances = staking_pool.ops_stake_withdraw(None);
//             println!("{:#?}", balances);
//             assert!(balances.unstaked.is_none());
//
//             let receipts = deserialize_receipts();
//             assert_eq!(receipts.len(), 1);
//             let receipt = &receipts[0];
//             assert_eq!(receipt.receiver_id, ACCOUNT.to_string());
//             assert_eq!(receipt.actions.len(), 1);
//             let action = &receipt.actions[0];
//             if let Action::Transfer(transfer) = action {
//                 assert_eq!(transfer.deposit, YOCTO / 2);
//             } else {
//                 panic!("expected transfer action");
//             }
//             assert_eq!(State::liquidity(), YoctoNear::ZERO);
//         }
//
//         #[test]
//         fn all_with_locked_unstaked_funds_with_liquidity_partially_available_with_unlocked_funds() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//
//             stake_unstake(
//                 ctx.clone(),
//                 &mut staking_pool,
//                 YOCTO.into(),
//                 (YOCTO / 2).into(),
//             );
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx.clone());
//             State::add_liquidity((YOCTO / 4).into());
//             assert_eq!(State::liquidity(), (YOCTO / 4).into());
//             let starting_balances = staking_pool
//                 .ops_stake_balance(to_valid_account_id(ACCOUNT))
//                 .unwrap();
//             assert_eq!(
//                 starting_balances.unstaked.as_ref().unwrap().available,
//                 YoctoNear::ZERO
//             );
//
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             ctx.epoch_height += 4;
//             testing_env!(ctx.clone());
//             let balances = staking_pool.ops_stake_withdraw(None);
//             println!("{:#?}", balances);
//             assert!(balances.unstaked.is_none());
//
//             let receipts = deserialize_receipts();
//             assert_eq!(receipts.len(), 1);
//             let receipt = &receipts[0];
//             assert_eq!(receipt.receiver_id, ACCOUNT.to_string());
//             assert_eq!(receipt.actions.len(), 1);
//             let action = &receipt.actions[0];
//             if let Action::Transfer(transfer) = action {
//                 assert_eq!(transfer.deposit, YOCTO / 2);
//             } else {
//                 panic!("expected transfer action");
//             }
//             assert_eq!(State::liquidity(), YoctoNear::ZERO);
//         }
//
//         #[test]
//         fn all_with_zero_unstaked_funds() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//
//             stake_unstake(ctx.clone(), &mut staking_pool, YOCTO.into(), 0.into());
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx.clone());
//
//             let starting_balances = staking_pool
//                 .ops_stake_balance(to_valid_account_id(ACCOUNT))
//                 .unwrap();
//
//             let balances = staking_pool.ops_stake_withdraw(None);
//             assert_eq!(balances, starting_balances);
//
//             assert!(test_utils::get_logs().is_empty());
//             assert!(deserialize_receipts().is_empty());
//         }
//
//         #[test]
//         fn all_with_zero_staked_and_unstaked_funds() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//
//             stake_unstake(ctx.clone(), &mut staking_pool, 0.into(), 0.into());
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx.clone());
//
//             let starting_balances = staking_pool
//                 .ops_stake_balance(to_valid_account_id(ACCOUNT))
//                 .unwrap();
//
//             let balances = staking_pool.ops_stake_withdraw(None);
//             assert_eq!(balances, starting_balances);
//
//             assert!(test_utils::get_logs().is_empty());
//             assert!(deserialize_receipts().is_empty());
//         }
//
//         #[test]
//         #[should_panic(expected = "[ERR] [ACCOUNT_NOT_REGISTERED]")]
//         fn all_with_unregistered_account() {
//             let (ctx, mut staking_pool) = deploy_with_unregistered_account();
//
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx);
//             staking_pool.ops_stake_withdraw(None);
//         }
//
//         #[test]
//         fn partial_with_available_unstaked_funds() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//
//             stake_unstake(
//                 ctx.clone(),
//                 &mut staking_pool,
//                 YOCTO.into(),
//                 (YOCTO / 2).into(),
//             );
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             ctx.epoch_height = ctx.epoch_height + 4;
//             testing_env!(ctx.clone());
//             let starting_balances = staking_pool
//                 .ops_stake_balance(to_valid_account_id(ACCOUNT))
//                 .unwrap();
//             assert_eq!(
//                 starting_balances.unstaked.as_ref().unwrap().total,
//                 starting_balances.unstaked.as_ref().unwrap().available
//             );
//             let balances = staking_pool.ops_stake_withdraw(Some((YOCTO / 8).into()));
//             println!("{:#?}", balances);
//             assert_eq!(balances.unstaked.unwrap().available, (YOCTO * 3 / 8).into());
//
//             let receipts = deserialize_receipts();
//             assert_eq!(receipts.len(), 1);
//             let receipt = &receipts[0];
//             assert_eq!(receipt.receiver_id, ACCOUNT.to_string());
//             assert_eq!(receipt.actions.len(), 1);
//             let action = &receipt.actions[0];
//             if let Action::Transfer(transfer) = action {
//                 assert_eq!(transfer.deposit, YOCTO / 8);
//             } else {
//                 panic!("expected transfer action");
//             }
//         }
//
//         #[test]
//         #[should_panic(expected = "[ERR] [INSUFFICIENT_FUNDS]")]
//         fn partial_with_locked_unstaked_funds_and_zero_available() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//
//             stake_unstake(
//                 ctx.clone(),
//                 &mut staking_pool,
//                 YOCTO.into(),
//                 (YOCTO / 2).into(),
//             );
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx.clone());
//             staking_pool.ops_stake_withdraw(Some((YOCTO / 8).into()));
//         }
//
//         #[test]
//         fn partial_with_locked_unstaked_funds_with_liquidity_fully_available() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//
//             stake_unstake(
//                 ctx.clone(),
//                 &mut staking_pool,
//                 YOCTO.into(),
//                 (YOCTO / 2).into(),
//             );
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx.clone());
//             State::add_liquidity(YOCTO.into());
//             assert_eq!(State::liquidity(), (YOCTO / 2).into());
//             let starting_balances = staking_pool
//                 .ops_stake_balance(to_valid_account_id(ACCOUNT))
//                 .unwrap();
//             assert_eq!(
//                 starting_balances.unstaked.as_ref().unwrap().available,
//                 YoctoNear::ZERO
//             );
//             let balances = staking_pool.ops_stake_withdraw(Some((YOCTO / 8).into()));
//             println!("{:#?}", balances);
//             assert_eq!(balances.unstaked.unwrap().total, (YOCTO * 3 / 8).into());
//
//             let receipts = deserialize_receipts();
//             assert_eq!(receipts.len(), 1);
//             let receipt = &receipts[0];
//             assert_eq!(receipt.receiver_id, ACCOUNT.to_string());
//             assert_eq!(receipt.actions.len(), 1);
//             let action = &receipt.actions[0];
//             if let Action::Transfer(transfer) = action {
//                 assert_eq!(transfer.deposit, YOCTO / 8);
//             } else {
//                 panic!("expected transfer action");
//             }
//             assert_eq!(State::liquidity(), (YOCTO * 3 / 8).into());
//         }
//
//         #[test]
//         fn partial_with_locked_unstaked_funds_with_liquidity_partially_available_and_unlocked_funds(
//         ) {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//
//             stake_unstake(
//                 ctx.clone(),
//                 &mut staking_pool,
//                 YOCTO.into(),
//                 (YOCTO / 2).into(),
//             );
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             ctx.epoch_height += 4;
//             testing_env!(ctx.clone());
//             State::add_liquidity((YOCTO / 16).into());
//             assert_eq!(State::liquidity(), (YOCTO / 16).into());
//             let starting_balances = staking_pool
//                 .ops_stake_balance(to_valid_account_id(ACCOUNT))
//                 .unwrap();
//             assert_eq!(
//                 starting_balances.unstaked.as_ref().unwrap().available,
//                 (YOCTO / 2).into()
//             );
//             let balances = staking_pool.ops_stake_withdraw(Some((YOCTO / 8).into()));
//             println!("{:#?}", balances);
//             assert_eq!(balances.unstaked.unwrap().total, (YOCTO * 3 / 8).into());
//
//             let receipts = deserialize_receipts();
//             assert_eq!(receipts.len(), 1);
//             let receipt = &receipts[0];
//             assert_eq!(receipt.receiver_id, ACCOUNT.to_string());
//             assert_eq!(receipt.actions.len(), 1);
//             let action = &receipt.actions[0];
//             if let Action::Transfer(transfer) = action {
//                 assert_eq!(transfer.deposit, YOCTO / 8);
//             } else {
//                 panic!("expected transfer action");
//             }
//             assert_eq!(State::liquidity(), YoctoNear::ZERO);
//         }
//
//         #[test]
//         #[should_panic(expected = "[ERR] [INSUFFICIENT_FUNDS]")]
//         fn partial_with_insufficient_funds() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//
//             stake_unstake(
//                 ctx.clone(),
//                 &mut staking_pool,
//                 YOCTO.into(),
//                 (YOCTO / 8).into(),
//             );
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx.clone());
//             staking_pool.ops_stake_withdraw(Some(YOCTO.into()));
//         }
//
//         #[test]
//         #[should_panic(expected = "[ERR] [INSUFFICIENT_FUNDS]")]
//         fn partial_with_zero_unstaked_funds() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//
//             stake_unstake(
//                 ctx.clone(),
//                 &mut staking_pool,
//                 YOCTO.into(),
//                 YoctoNear::ZERO,
//             );
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx.clone());
//             staking_pool.ops_stake_withdraw(Some((YOCTO / 8).into()));
//         }
//
//         #[test]
//         #[should_panic(expected = "[ERR] [INSUFFICIENT_FUNDS]")]
//         fn partial_with_zero_staked_and_unstaked_funds() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//
//             stake_unstake(
//                 ctx.clone(),
//                 &mut staking_pool,
//                 YoctoNear::ZERO,
//                 YoctoNear::ZERO,
//             );
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx.clone());
//             staking_pool.ops_stake_withdraw(Some((YOCTO / 8).into()));
//         }
//
//         #[test]
//         #[should_panic(expected = "[ERR] [ACCOUNT_NOT_REGISTERED]")]
//         fn partial_with_unregistered_account() {
//             let (ctx, mut staking_pool) = deploy_with_unregistered_account();
//
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx.clone());
//             staking_pool.ops_stake_withdraw(Some((YOCTO / 8).into()));
//         }
//     }
//
//     #[cfg(test)]
//     mod tests_owner_online {
//         use super::*;
//
//         #[test]
//         fn stake_all_available_balance() {
//             let (ctx, mut staking_pool) = deploy_with_unregistered_account();
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = OWNER.to_string();
//             testing_env!(ctx);
//
//             let initial_owner_balance = ContractOwnershipComponent.ops_owner_balance();
//             println!("initial {:?}", initial_owner_balance);
//
//             let initial_staking_pool_balances = staking_pool.ops_stake_pool_balances();
//             println!(
//                 "initial_staking_pool_balances {:#?}",
//                 initial_staking_pool_balances
//             );
//
//             let state = *StakingPoolComponent::state();
//             println!(
//                 "initial state: {}",
//                 serde_json::to_string_pretty(&state).unwrap()
//             );
//
//             // Act
//             if let PromiseOrValue::Value(_) = staking_pool.ops_stake_owner_balance(None) {
//                 panic!("expected Promise");
//             }
//
//             let owner_balance = ContractOwnershipComponent.ops_owner_balance();
//             println!("after staking {:#?}", owner_balance);
//             println!("{:#?}", StakingPoolBalances::load());
//             assert_eq!(owner_balance.available, YoctoNear::ZERO);
//
//             // Assert
//             let logs = test_utils::get_logs();
//             println!("{:#?}", logs);
//
//             let receipts = deserialize_receipts();
//             let action = &receipts[0].actions[0];
//             if let Action::Stake(stake) = action {
//                 assert_eq!(
//                     stake.stake,
//                     *staking_pool.ops_stake_pool_balances().total_staked
//                 );
//             } else {
//                 panic!("expected stake action")
//             }
//
//             let action = &receipts[1].actions[0];
//             if let Action::FunctionCall(function_call) = action {
//                 assert_eq!(function_call.method_name, "ops_stake_finalize");
//                 let args: StakeActionCallbackArgs =
//                     serde_json::from_str(function_call.args.as_str()).unwrap();
//                 assert_eq!(args.account_id, ContractOwnershipComponent.ops_owner());
//             } else {
//                 panic!("expected stake action")
//             }
//         }
//
//         #[test]
//         fn stake_partial_available_balance() {
//             let (ctx, mut staking_pool) = deploy_with_unregistered_account();
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//
//             let initial_owner_balance = ContractOwnershipComponent.ops_owner_balance();
//             println!("initial {:#?}", initial_owner_balance);
//
//             let initial_staking_pool_balances = staking_pool.ops_stake_pool_balances();
//             println!(
//                 "initial_staking_pool_balances {:#?}",
//                 initial_staking_pool_balances
//             );
//
//             let state = *StakingPoolComponent::state();
//             println!(
//                 "initial state: {}",
//                 serde_json::to_string_pretty(&state).unwrap()
//             );
//
//             let owner_balance = {
//                 let mut ctx = ctx.clone();
//                 ctx.predecessor_account_id = OWNER.to_string();
//                 testing_env!(ctx.clone());
//
//                 // Act
//                 if let PromiseOrValue::Value(_) =
//                     staking_pool.ops_stake_owner_balance(Some(YOCTO.into()))
//                 {
//                     panic!("expected Promise");
//                 }
//
//                 let owner_balance = ContractOwnershipComponent.ops_owner_balance();
//                 println!("after staking {:#?}", owner_balance);
//                 println!("{:#?}", StakingPoolBalances::load());
//
//                 // Assert
//                 let logs = test_utils::get_logs();
//                 println!("{:#?}", logs);
//
//                 let receipts = deserialize_receipts();
//                 let action = &receipts[0].actions[0];
//                 if let Action::Stake(stake) = action {
//                     assert_eq!(
//                         stake.stake,
//                         *staking_pool.ops_stake_pool_balances().total_staked
//                     );
//                 } else {
//                     panic!("expected stake action")
//                 }
//
//                 let action = &receipts[1].actions[0];
//                 if let Action::FunctionCall(function_call) = action {
//                     assert_eq!(function_call.method_name, "ops_stake_finalize");
//                     let args: StakeActionCallbackArgs =
//                         serde_json::from_str(function_call.args.as_str()).unwrap();
//                     assert_eq!(args.account_id, ContractOwnershipComponent.ops_owner());
//                 } else {
//                     panic!("expected stake action")
//                 }
//                 owner_balance
//             };
//
//             {
//                 let mut ctx = ctx.clone();
//                 ctx.predecessor_account_id = OWNER.to_string();
//                 testing_env!(ctx.clone());
//
//                 let expected_lower_than = initial_owner_balance.available
//                     - YOCTO
//                     - account_manager()
//                         .storage_balance_of(to_valid_account_id(
//                             ContractOwnershipComponent.ops_owner().as_str(),
//                         ))
//                         .unwrap()
//                         .total;
//                 println!("*** expected_lower_than = {}", expected_lower_than);
//                 assert!(owner_balance.available < expected_lower_than);
//             }
//         }
//
//         #[test]
//         #[should_panic(expected = "[ERR] [INSUFFICIENT_FUNDS]")]
//         fn insufficient_funds() {
//             let (ctx, mut staking_pool) = deploy_with_unregistered_account();
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//
//             let initial_owner_balance = ContractOwnershipComponent.ops_owner_balance();
//
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = OWNER.to_string();
//             testing_env!(ctx);
//             staking_pool.ops_stake_owner_balance(Some(initial_owner_balance.total));
//         }
//
//         #[test]
//         #[should_panic(expected = "[ERR] [OWNER_ACCESS_REQUIRED]")]
//         fn not_owner() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//             bring_pool_online(ctx.clone(), &mut staking_pool);
//
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx);
//             staking_pool.ops_stake_owner_balance(None);
//         }
//     }
//
//     #[cfg(test)]
//     mod tests_owner_offline {
//         use super::*;
//
//         #[test]
//         fn stake_all_available_balance() {
//             let (ctx, mut staking_pool) = deploy_with_unregistered_account();
//
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = OWNER.to_string();
//             testing_env!(ctx);
//
//             let initial_owner_balance = ContractOwnershipComponent.ops_owner_balance();
//             println!("initial {:?}", initial_owner_balance);
//
//             let initial_staking_pool_balances = staking_pool.ops_stake_pool_balances();
//             println!(
//                 "initial_staking_pool_balances {:#?}",
//                 initial_staking_pool_balances
//             );
//
//             let state = *StakingPoolComponent::state();
//             println!(
//                 "initial state: {}",
//                 serde_json::to_string_pretty(&state).unwrap()
//             );
//
//             // Act
//             if let PromiseOrValue::Value(balances) = staking_pool.ops_stake_owner_balance(None) {
//                 let owner_balance = ContractOwnershipComponent.ops_owner_balance();
//                 println!("after staking {:#?}", owner_balance);
//                 println!("{:#?}", StakingPoolBalances::load());
//                 assert_eq!(owner_balance.available, YoctoNear::ZERO);
//                 assert_eq!(
//                     balances.staked.unwrap().near_value,
//                     9996918230000000000000000000.into()
//                 );
//
//                 // Assert
//                 let logs = test_utils::get_logs();
//                 println!("{:#?}", logs);
//
//                 assert!(deserialize_receipts().is_empty());
//             } else {
//                 panic!("expected value");
//             }
//         }
//
//         #[test]
//         fn stake_partial_available_balance() {
//             let (ctx, mut staking_pool) = deploy_with_unregistered_account();
//
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = OWNER.to_string();
//             testing_env!(ctx);
//
//             let initial_owner_balance = ContractOwnershipComponent.ops_owner_balance();
//             println!("initial {:?}", initial_owner_balance);
//
//             let initial_staking_pool_balances = staking_pool.ops_stake_pool_balances();
//             println!(
//                 "initial_staking_pool_balances {:#?}",
//                 initial_staking_pool_balances
//             );
//
//             let state = *StakingPoolComponent::state();
//             println!(
//                 "initial state: {}",
//                 serde_json::to_string_pretty(&state).unwrap()
//             );
//
//             // Act
//             if let PromiseOrValue::Value(balances) =
//                 staking_pool.ops_stake_owner_balance(Some(YOCTO.into()))
//             {
//                 let logs = test_utils::get_logs();
//                 println!("{:#?}", logs);
//
//                 println!("{:#?}", balances);
//
//                 let owner_balance = ContractOwnershipComponent.ops_owner_balance();
//                 println!("after staking {:#?}", owner_balance);
//                 println!("{:#?}", StakingPoolBalances::load());
//                 assert!(
//                     owner_balance.available
//                         < initial_owner_balance.available
//                             - YOCTO
//                             - account_manager()
//                                 .storage_balance_of(to_valid_account_id(
//                                     ContractOwnershipComponent.ops_owner().as_str()
//                                 ))
//                                 .unwrap()
//                                 .total
//                 );
//                 assert!(deserialize_receipts().is_empty());
//             } else {
//                 panic!("expected Promise");
//             }
//         }
//
//         #[test]
//         #[should_panic(expected = "[ERR] [INSUFFICIENT_FUNDS]")]
//         fn insufficient_funds() {
//             let (ctx, mut staking_pool) = deploy_with_unregistered_account();
//
//             let initial_owner_balance = ContractOwnershipComponent.ops_owner_balance();
//
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = OWNER.to_string();
//             testing_env!(ctx);
//             staking_pool.ops_stake_owner_balance(Some(initial_owner_balance.total));
//         }
//
//         #[test]
//         #[should_panic(expected = "[ERR] [OWNER_ACCESS_REQUIRED]")]
//         fn not_owner() {
//             let (ctx, mut staking_pool) = deploy_with_registered_account();
//
//             let mut ctx = ctx.clone();
//             ctx.predecessor_account_id = ACCOUNT.to_string();
//             testing_env!(ctx);
//             staking_pool.ops_stake_owner_balance(None);
//         }
//     }
//
//     #[cfg(test)]
//     mod tests_treasury_online {
//         use super::*;
//
//         #[cfg(test)]
//         mod tests_ops_stake_treasury_deposit {
//             use super::*;
//
//             #[test]
//             fn with_attached_deposit() {
//                 let (ctx, mut staking_pool) = deploy_with_unregistered_account();
//                 bring_pool_online(ctx.clone(), &mut staking_pool);
//
//                 let state = *StakingPoolComponent::state();
//                 assert_eq!(state.treasury_balance, YoctoNear::ZERO);
//
//                 // Act
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.attached_deposit = YOCTO;
//                     testing_env!(ctx);
//                     if let PromiseOrValue::Value(_) = staking_pool.ops_stake_treasury_deposit() {
//                         panic!("expected Promise")
//                     }
//                     let logs = test_utils::get_logs();
//                     println!("{:#?}", logs);
//                     let staking_pool_balances = staking_pool.ops_stake_pool_balances();
//                     println!("{:#?}", staking_pool_balances);
//                     assert_eq!(staking_pool_balances.total_staked, YOCTO.into());
//                     let state = *StakingPoolComponent::state();
//                     println!("{}", serde_json::to_string_pretty(&state).unwrap());
//                     assert_eq!(state.treasury_balance, YoctoNear::ZERO);
//
//                     let treasury_stake_balance =
//                         ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
//                     assert_eq!(treasury_stake_balance, TokenAmount::ZERO);
//
//                     deserialize_receipts();
//                 }
//                 // finalize the staked treasury deposit
//                 {
//                     let staking_pool_balances = staking_pool.ops_stake_pool_balances();
//
//                     let mut ctx = ctx.clone();
//                     ctx.attached_deposit = 0;
//                     ctx.account_locked_balance = *staking_pool_balances.total_staked;
//                     testing_env_with_promise_result_success(ctx);
//                     let balances = staking_pool.ops_stake_finalize(env::current_account_id());
//                     let logs = test_utils::get_logs();
//                     println!("{:#?}", logs);
//
//                     println!("{:#?}", balances);
//                     assert_eq!(balances.staked.unwrap().near_value, YOCTO.into());
//
//                     let treasury_stake_balance =
//                         ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
//                     assert_eq!(treasury_stake_balance, YOCTO.into());
//
//                     assert_eq!(
//                         *StakingPoolComponent::state().treasury_balance,
//                         YOCTO.into()
//                     );
//                 }
//             }
//
//             #[test]
//             #[should_panic(expected = "[ERR] [NEAR_DEPOSIT_REQUIRED]")]
//             fn with_zero_attached_deposit() {
//                 let (ctx, mut staking_pool) = deploy_with_unregistered_account();
//                 bring_pool_online(ctx.clone(), &mut staking_pool);
//
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = 0;
//                 testing_env!(ctx);
//                 staking_pool.ops_stake_treasury_deposit();
//             }
//         }
//
//         #[cfg(test)]
//         mod tests_ops_stake_treasury_distribution {
//             use super::*;
//
//             #[test]
//             fn with_attached_deposit() {
//                 let (ctx, mut staking_pool) = deploy_with_unregistered_account();
//                 bring_pool_online(ctx.clone(), &mut staking_pool);
//
//                 let state = *StakingPoolComponent::state();
//                 assert_eq!(state.treasury_balance, YoctoNear::ZERO);
//
//                 // Act
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.attached_deposit = YOCTO;
//                     testing_env!(ctx);
//                     staking_pool.ops_stake_treasury_distribution();
//                     let logs = test_utils::get_logs();
//                     println!("{:#?}", logs);
//                     let staking_pool_balances = staking_pool.ops_stake_pool_balances();
//                     println!("{:#?}", staking_pool_balances);
//                     assert_eq!(staking_pool_balances.total_staked, YOCTO.into());
//                     let state = *StakingPoolComponent::state();
//                     println!("{}", serde_json::to_string_pretty(&state).unwrap());
//                     assert_eq!(state.treasury_balance, YoctoNear::ZERO);
//
//                     let treasury_stake_balance =
//                         ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
//                     assert_eq!(treasury_stake_balance, TokenAmount::ZERO);
//
//                     let receipts = deserialize_receipts();
//                     assert_eq!(receipts.len(), 2);
//                     {
//                         let receipt = &receipts[0];
//                         assert_eq!(receipt.receiver_id, env::current_account_id());
//                         let action = &receipt.actions[0];
//                         match action {
//                             Action::Stake(action) => {
//                                 assert_eq!(
//                                     action.stake,
//                                     *staking_pool.ops_stake_pool_balances().total_staked
//                                 );
//                             }
//                             _ => panic!("expected StakeAction"),
//                         }
//                     }
//                     {
//                         let receipt = &receipts[1];
//                         assert_eq!(receipt.receiver_id, env::current_account_id());
//                         let action = &receipt.actions[0];
//                         match action {
//                             Action::FunctionCall(action) => {
//                                 let args: StakeActionCallbackArgs =
//                                     serde_json::from_str(&action.args).unwrap();
//                                 assert_eq!(args.account_id, env::current_account_id());
//                             }
//                             _ => panic!("expected FunctionCallAction"),
//                         }
//                     }
//                 }
//                 // finalize the staked treasury deposit
//                 {
//                     let staking_pool_balances = staking_pool.ops_stake_pool_balances();
//
//                     let mut ctx = ctx.clone();
//                     ctx.attached_deposit = 0;
//                     ctx.account_locked_balance = *staking_pool_balances.total_staked;
//                     testing_env_with_promise_result_success(ctx);
//                     let balances = staking_pool.ops_stake_finalize(env::current_account_id());
//                     let logs = test_utils::get_logs();
//                     println!("{:#?}", logs);
//
//                     println!("{:#?}", balances);
//                     assert!(balances.staked.is_none());
//
//                     let treasury_stake_balance =
//                         ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
//                     assert_eq!(treasury_stake_balance, TokenAmount::ZERO);
//
//                     assert_eq!(
//                         StakingPoolComponent::state().treasury_balance,
//                         YoctoNear::ZERO
//                     );
//
//                     assert_eq!(
//                         StakingPoolComponent::state().total_staked_balance,
//                         YOCTO.into()
//                     );
//                 }
//             }
//
//             #[test]
//             #[should_panic(expected = "[ERR] [NEAR_DEPOSIT_REQUIRED]")]
//             fn with_zero_attached_deposit() {
//                 let (ctx, mut staking_pool) = deploy_with_unregistered_account();
//                 bring_pool_online(ctx.clone(), &mut staking_pool);
//
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = 0;
//                 testing_env!(ctx);
//                 staking_pool.ops_stake_treasury_distribution();
//             }
//         }
//
//         #[cfg(test)]
//         mod ops_stake_treasury_transfer_to_owner {
//             use super::*;
//
//             #[test]
//             fn transfer_all_as_admin() {
//                 let (ctx, mut staking_pool) = deploy_with_unregistered_account();
//                 bring_pool_online(ctx.clone(), &mut staking_pool);
//
//                 let state = *StakingPoolComponent::state();
//                 assert_eq!(state.treasury_balance, YoctoNear::ZERO);
//
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.attached_deposit = YOCTO;
//                     testing_env!(ctx);
//                     if let PromiseOrValue::Value(_) = staking_pool.ops_stake_treasury_deposit() {
//                         panic!("expected Promise")
//                     }
//                     let logs = test_utils::get_logs();
//                     println!("{:#?}", logs);
//                     let staking_pool_balances = staking_pool.ops_stake_pool_balances();
//                     println!("{:#?}", staking_pool_balances);
//                     assert_eq!(staking_pool_balances.total_staked, YOCTO.into());
//                     let state = *StakingPoolComponent::state();
//                     println!("{}", serde_json::to_string_pretty(&state).unwrap());
//                     assert_eq!(state.treasury_balance, YoctoNear::ZERO);
//
//                     let treasury_stake_balance =
//                         ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
//                     assert_eq!(treasury_stake_balance, TokenAmount::ZERO);
//
//                     deserialize_receipts();
//                 }
//                 {
//                     let staking_pool_balances = staking_pool.ops_stake_pool_balances();
//
//                     let mut ctx = ctx.clone();
//                     ctx.attached_deposit = 0;
//                     ctx.account_locked_balance = *staking_pool_balances.total_staked;
//                     testing_env_with_promise_result_success(ctx);
//                     let balances = staking_pool.ops_stake_finalize(env::current_account_id());
//                     let logs = test_utils::get_logs();
//                     println!("{:#?}", logs);
//
//                     println!("{:#?}", balances);
//                     assert_eq!(balances.staked.unwrap().near_value, YOCTO.into());
//
//                     let treasury_stake_balance =
//                         ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
//                     assert_eq!(treasury_stake_balance, YOCTO.into());
//
//                     assert_eq!(
//                         *StakingPoolComponent::state().treasury_balance,
//                         YOCTO.into()
//                     );
//                 }
//
//                 // Act
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ADMIN.to_string();
//                     testing_env!(ctx.clone());
//                     staking_pool.ops_stake_treasury_transfer_to_owner(None);
//
//                     let treasury_stake_balance =
//                         ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
//                     assert_eq!(treasury_stake_balance, TokenAmount::ZERO);
//
//                     let owner_stake_balance = ft_stake().ft_balance_of(to_valid_account_id(
//                         &ContractOwnershipComponent.ops_owner(),
//                     ));
//                     assert_eq!(owner_stake_balance, YOCTO.into());
//
//                     let state = *StakingPoolComponent::state();
//                     assert_eq!(state.treasury_balance, YoctoNear::ZERO);
//                 }
//             }
//
//             #[test]
//             fn transfer_all_as_treasurer() {
//                 let (ctx, mut staking_pool) = deploy_with_registered_account();
//                 bring_pool_online(ctx.clone(), &mut staking_pool);
//
//                 let state = *StakingPoolComponent::state();
//                 assert_eq!(state.treasury_balance, YoctoNear::ZERO);
//
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.attached_deposit = YOCTO;
//                     testing_env!(ctx);
//                     if let PromiseOrValue::Value(_) = staking_pool.ops_stake_treasury_deposit() {
//                         panic!("expected Promise")
//                     }
//                     let logs = test_utils::get_logs();
//                     println!("{:#?}", logs);
//                     let staking_pool_balances = staking_pool.ops_stake_pool_balances();
//                     println!("{:#?}", staking_pool_balances);
//                     assert_eq!(staking_pool_balances.total_staked, YOCTO.into());
//                     let state = *StakingPoolComponent::state();
//                     println!("{}", serde_json::to_string_pretty(&state).unwrap());
//                     assert_eq!(state.treasury_balance, YoctoNear::ZERO);
//
//                     let treasury_stake_balance =
//                         ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
//                     assert_eq!(treasury_stake_balance, TokenAmount::ZERO);
//
//                     deserialize_receipts();
//                 }
//                 {
//                     let staking_pool_balances = staking_pool.ops_stake_pool_balances();
//
//                     let mut ctx = ctx.clone();
//                     ctx.attached_deposit = 0;
//                     ctx.account_locked_balance = *staking_pool_balances.total_staked;
//                     testing_env_with_promise_result_success(ctx);
//                     let balances = staking_pool.ops_stake_finalize(env::current_account_id());
//                     let logs = test_utils::get_logs();
//                     println!("{:#?}", logs);
//
//                     println!("{:#?}", balances);
//                     assert_eq!(balances.staked.unwrap().near_value, YOCTO.into());
//
//                     let treasury_stake_balance =
//                         ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
//                     assert_eq!(treasury_stake_balance, YOCTO.into());
//
//                     assert_eq!(
//                         *StakingPoolComponent::state().treasury_balance,
//                         YOCTO.into()
//                     );
//                 }
//
//                 // Arrange - grant account treasurer
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ADMIN.to_string();
//                     testing_env!(ctx.clone());
//                     account_manager().ops_permissions_grant(
//                         to_valid_account_id(ACCOUNT),
//                         (1 << TREASURER_PERMISSION_BIT).into(),
//                     );
//                 }
//
//                 // Act
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ACCOUNT.to_string();
//                     testing_env!(ctx.clone());
//                     staking_pool.ops_stake_treasury_transfer_to_owner(None);
//
//                     let treasury_stake_balance =
//                         ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
//                     assert_eq!(treasury_stake_balance, TokenAmount::ZERO);
//
//                     let owner_stake_balance = ft_stake().ft_balance_of(to_valid_account_id(
//                         &ContractOwnershipComponent.ops_owner(),
//                     ));
//                     assert_eq!(owner_stake_balance, YOCTO.into());
//
//                     let state = *StakingPoolComponent::state();
//                     assert_eq!(state.treasury_balance, YoctoNear::ZERO);
//                 }
//             }
//
//             #[test]
//             fn transfer_all_as_owner() {
//                 let (ctx, mut staking_pool) = deploy_with_registered_account();
//                 bring_pool_online(ctx.clone(), &mut staking_pool);
//
//                 let state = *StakingPoolComponent::state();
//                 assert_eq!(state.treasury_balance, YoctoNear::ZERO);
//
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.attached_deposit = YOCTO;
//                     testing_env!(ctx);
//                     if let PromiseOrValue::Value(_) = staking_pool.ops_stake_treasury_deposit() {
//                         panic!("expected Promise")
//                     }
//                     let logs = test_utils::get_logs();
//                     println!("{:#?}", logs);
//                     let staking_pool_balances = staking_pool.ops_stake_pool_balances();
//                     println!("{:#?}", staking_pool_balances);
//                     assert_eq!(staking_pool_balances.total_staked, YOCTO.into());
//                     let state = *StakingPoolComponent::state();
//                     println!("{}", serde_json::to_string_pretty(&state).unwrap());
//                     assert_eq!(state.treasury_balance, YoctoNear::ZERO);
//
//                     let treasury_stake_balance =
//                         ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
//                     assert_eq!(treasury_stake_balance, TokenAmount::ZERO);
//
//                     deserialize_receipts();
//                 }
//                 {
//                     let staking_pool_balances = staking_pool.ops_stake_pool_balances();
//
//                     let mut ctx = ctx.clone();
//                     ctx.attached_deposit = 0;
//                     ctx.account_locked_balance = *staking_pool_balances.total_staked;
//                     testing_env_with_promise_result_success(ctx);
//                     let balances = staking_pool.ops_stake_finalize(env::current_account_id());
//                     let logs = test_utils::get_logs();
//                     println!("{:#?}", logs);
//
//                     println!("{:#?}", balances);
//                     assert_eq!(balances.staked.unwrap().near_value, YOCTO.into());
//
//                     let treasury_stake_balance =
//                         ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
//                     assert_eq!(treasury_stake_balance, YOCTO.into());
//
//                     assert_eq!(
//                         *StakingPoolComponent::state().treasury_balance,
//                         YOCTO.into()
//                     );
//                 }
//
//                 // Act
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = OWNER.to_string();
//                     testing_env!(ctx.clone());
//                     staking_pool.ops_stake_treasury_transfer_to_owner(None);
//
//                     let treasury_stake_balance =
//                         ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
//                     assert_eq!(treasury_stake_balance, TokenAmount::ZERO);
//
//                     let owner_stake_balance = ft_stake().ft_balance_of(to_valid_account_id(
//                         &ContractOwnershipComponent.ops_owner(),
//                     ));
//                     assert_eq!(owner_stake_balance, YOCTO.into());
//
//                     let state = *StakingPoolComponent::state();
//                     assert_eq!(state.treasury_balance, YoctoNear::ZERO);
//                 }
//             }
//
//             #[test]
//             fn transfer_partial_as_admin() {
//                 let (ctx, mut staking_pool) = deploy_with_registered_account();
//                 bring_pool_online(ctx.clone(), &mut staking_pool);
//
//                 let state = *StakingPoolComponent::state();
//                 assert_eq!(state.treasury_balance, YoctoNear::ZERO);
//
//                 const TREASURY_DEPOSIT: YoctoNear = YoctoNear(3 * YOCTO);
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.attached_deposit = *TREASURY_DEPOSIT;
//                     testing_env!(ctx);
//                     if let PromiseOrValue::Value(_) = staking_pool.ops_stake_treasury_deposit() {
//                         panic!("expected Promise")
//                     }
//                     let logs = test_utils::get_logs();
//                     println!("{:#?}", logs);
//                     let staking_pool_balances = staking_pool.ops_stake_pool_balances();
//                     println!("{:#?}", staking_pool_balances);
//                     assert_eq!(staking_pool_balances.total_staked, TREASURY_DEPOSIT);
//                     let state = *StakingPoolComponent::state();
//                     println!("{}", serde_json::to_string_pretty(&state).unwrap());
//                     assert_eq!(state.treasury_balance, YoctoNear::ZERO);
//
//                     let treasury_stake_balance =
//                         ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
//                     assert_eq!(treasury_stake_balance, TokenAmount::ZERO);
//
//                     deserialize_receipts();
//                 }
//                 {
//                     let staking_pool_balances = staking_pool.ops_stake_pool_balances();
//
//                     let mut ctx = ctx.clone();
//                     ctx.attached_deposit = 0;
//                     ctx.account_locked_balance = *staking_pool_balances.total_staked;
//                     testing_env_with_promise_result_success(ctx);
//                     let balances = staking_pool.ops_stake_finalize(env::current_account_id());
//                     let logs = test_utils::get_logs();
//                     println!("{:#?}", logs);
//
//                     println!("{:#?}", balances);
//                     assert_eq!(balances.staked.unwrap().near_value, TREASURY_DEPOSIT);
//
//                     let treasury_stake_balance =
//                         ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
//                     assert_eq!(treasury_stake_balance, (*TREASURY_DEPOSIT).into());
//
//                     assert_eq!(
//                         StakingPoolComponent::state().treasury_balance,
//                         TREASURY_DEPOSIT
//                     );
//                 }
//
//                 // Act
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ADMIN.to_string();
//                     testing_env!(ctx.clone());
//                     staking_pool.ops_stake_treasury_transfer_to_owner(Some(YOCTO.into()));
//
//                     let treasury_stake_balance =
//                         ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
//                     assert_eq!(treasury_stake_balance, (2 * YOCTO).into());
//
//                     let owner_stake_balance = ft_stake().ft_balance_of(to_valid_account_id(
//                         &ContractOwnershipComponent.ops_owner(),
//                     ));
//                     assert_eq!(owner_stake_balance, YOCTO.into());
//
//                     let state = *StakingPoolComponent::state();
//                     assert_eq!(state.treasury_balance, (2 * YOCTO).into());
//                 }
//             }
//
//             #[test]
//             #[should_panic(expected = "[ERR] [NOT_AUTHORIZED]")]
//             fn as_unauthorized_account() {
//                 let (ctx, mut staking_pool) = deploy_with_registered_account();
//                 bring_pool_online(ctx.clone(), &mut staking_pool);
//
//                 let mut ctx = ctx.clone();
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 testing_env!(ctx.clone());
//                 staking_pool.ops_stake_treasury_transfer_to_owner(None);
//             }
//         }
//     }
//
//     #[cfg(test)]
//     mod tests_treasury_offline {
//         use super::*;
//
//         #[cfg(test)]
//         mod tests_ops_stake_treasury_deposit {
//             use super::*;
//
//             #[test]
//             fn with_attached_deposit() {
//                 let (ctx, mut staking_pool) = deploy_with_unregistered_account();
//
//                 let state = *StakingPoolComponent::state();
//                 assert_eq!(state.treasury_balance, YoctoNear::ZERO);
//
//                 // Act
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.attached_deposit = YOCTO;
//                     testing_env!(ctx.clone());
//                     if let PromiseOrValue::Value(balances) =
//                         staking_pool.ops_stake_treasury_deposit()
//                     {
//                         println!("{:#?}", balances);
//                         assert_eq!(balances.staked.as_ref().unwrap().near_value, YOCTO.into());
//
//                         assert_eq!(
//                             balances,
//                             staking_pool
//                                 .ops_stake_balance(to_valid_account_id(
//                                     env::current_account_id().as_str()
//                                 ))
//                                 .unwrap()
//                         );
//                     } else {
//                         panic!("expected Promise")
//                     }
//                     let logs = test_utils::get_logs();
//                     println!("{:#?}", logs);
//
//                     let treasury_stake_balance =
//                         ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
//                     assert_eq!(treasury_stake_balance, YOCTO.into());
//
//                     assert_eq!(
//                         *StakingPoolComponent::state().treasury_balance,
//                         YOCTO.into()
//                     );
//
//                     assert!(deserialize_receipts().is_empty());
//                 }
//             }
//
//             #[test]
//             #[should_panic(expected = "[ERR] [NEAR_DEPOSIT_REQUIRED]")]
//             fn with_zero_attached_deposit() {
//                 let (ctx, mut staking_pool) = deploy_with_unregistered_account();
//
//                 let mut ctx = ctx.clone();
//                 ctx.attached_deposit = 0;
//                 testing_env!(ctx);
//                 staking_pool.ops_stake_treasury_deposit();
//             }
//         }
//
//         #[cfg(test)]
//         mod ops_stake_treasury_transfer_to_owner {
//             use super::*;
//
//             #[test]
//             fn transfer_all_as_admin() {
//                 let (ctx, mut staking_pool) = deploy_with_unregistered_account();
//
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.attached_deposit = YOCTO;
//                     testing_env!(ctx);
//                     if let PromiseOrValue::Promise(_) = staking_pool.ops_stake_treasury_deposit() {
//                         panic!("expected Value")
//                     }
//                 }
//
//                 // Act
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ADMIN.to_string();
//                     testing_env!(ctx.clone());
//                     staking_pool.ops_stake_treasury_transfer_to_owner(None);
//
//                     let treasury_stake_balance =
//                         ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
//                     assert_eq!(treasury_stake_balance, TokenAmount::ZERO);
//
//                     let owner_stake_balance = ft_stake().ft_balance_of(to_valid_account_id(
//                         &ContractOwnershipComponent.ops_owner(),
//                     ));
//                     assert_eq!(owner_stake_balance, YOCTO.into());
//
//                     let state = *StakingPoolComponent::state();
//                     assert_eq!(state.treasury_balance, YoctoNear::ZERO);
//                 }
//             }
//
//             #[test]
//             fn transfer_all_as_treasurer() {
//                 let (ctx, mut staking_pool) = deploy_with_registered_account();
//
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.attached_deposit = YOCTO;
//                     testing_env!(ctx);
//                     if let PromiseOrValue::Promise(_) = staking_pool.ops_stake_treasury_deposit() {
//                         panic!("expected Value")
//                     }
//                 }
//
//                 // Arrange - grant account treasurer
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ADMIN.to_string();
//                     testing_env!(ctx.clone());
//                     account_manager().ops_permissions_grant(
//                         to_valid_account_id(ACCOUNT),
//                         (1 << TREASURER_PERMISSION_BIT).into(),
//                     );
//                 }
//
//                 // Act
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ACCOUNT.to_string();
//                     testing_env!(ctx.clone());
//                     staking_pool.ops_stake_treasury_transfer_to_owner(None);
//
//                     let treasury_stake_balance =
//                         ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
//                     assert_eq!(treasury_stake_balance, TokenAmount::ZERO);
//
//                     let owner_stake_balance = ft_stake().ft_balance_of(to_valid_account_id(
//                         &ContractOwnershipComponent.ops_owner(),
//                     ));
//                     assert_eq!(owner_stake_balance, YOCTO.into());
//
//                     let state = *StakingPoolComponent::state();
//                     assert_eq!(state.treasury_balance, YoctoNear::ZERO);
//                 }
//             }
//
//             #[test]
//             fn transfer_partial_as_admin() {
//                 let (ctx, mut staking_pool) = deploy_with_registered_account();
//
//                 const TREASURY_DEPOSIT: YoctoNear = YoctoNear(3 * YOCTO);
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.attached_deposit = *TREASURY_DEPOSIT;
//                     testing_env!(ctx);
//                     if let PromiseOrValue::Promise(_) = staking_pool.ops_stake_treasury_deposit() {
//                         panic!("expected Value")
//                     }
//                 }
//
//                 // Act
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ADMIN.to_string();
//                     testing_env!(ctx.clone());
//                     staking_pool.ops_stake_treasury_transfer_to_owner(Some(YOCTO.into()));
//
//                     let treasury_stake_balance =
//                         ft_stake().ft_balance_of(to_valid_account_id(&env::current_account_id()));
//                     assert_eq!(treasury_stake_balance, (2 * YOCTO).into());
//
//                     let owner_stake_balance = ft_stake().ft_balance_of(to_valid_account_id(
//                         &ContractOwnershipComponent.ops_owner(),
//                     ));
//                     assert_eq!(owner_stake_balance, YOCTO.into());
//
//                     let state = *StakingPoolComponent::state();
//                     assert_eq!(state.treasury_balance, (2 * YOCTO).into());
//                 }
//             }
//
//             #[test]
//             #[should_panic(expected = "[ERR] [NOT_AUTHORIZED]")]
//             fn as_unauthorized_account() {
//                 let (ctx, mut staking_pool) = deploy_with_registered_account();
//
//                 let mut ctx = ctx.clone();
//                 ctx.predecessor_account_id = ACCOUNT.to_string();
//                 testing_env!(ctx.clone());
//                 staking_pool.ops_stake_treasury_transfer_to_owner(None);
//             }
//         }
//     }
//
//     #[cfg(test)]
//     mod tests_operator {
//         use super::*;
//
//         #[cfg(test)]
//         mod start_and_stop_staking_commands {
//             use super::*;
//
//             #[test]
//             fn starting_stopping_staking_with_nothing_staked() {
//                 let (ctx, mut staking_pool) = deploy_with_registered_account();
//
//                 assert_eq!(
//                     staking_pool.ops_stake_status(),
//                     Status::Offline(OfflineReason::Stopped)
//                 );
//
//                 // Act - start staking
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ADMIN.to_string();
//                     testing_env!(ctx);
//                     staking_pool
//                         .ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
//                     let logs = test_utils::get_logs();
//                     println!("{:#?}", logs);
//                     assert_eq!(logs, vec!["[INFO] [STATUS_ONLINE] ",]);
//                     // Assert
//                     assert_eq!(staking_pool.ops_stake_status(), Status::Online);
//                     assert_eq!(
//                         staking_pool.ops_stake_pool_balances().total_unstaked,
//                         YoctoNear::ZERO
//                     );
//                     // since there is nothing staked, then we expect no stake actions
//                     assert!(deserialize_receipts().is_empty());
//                 }
//                 // Act - start staking while already staking
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ADMIN.to_string();
//                     testing_env!(ctx);
//                     staking_pool
//                         .ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
//                     assert!(test_utils::get_logs().is_empty());
//                     // Assert
//                     assert_eq!(staking_pool.ops_stake_status(), Status::Online);
//                     assert!(deserialize_receipts().is_empty());
//                 }
//                 // Act - stop staking
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ADMIN.to_string();
//                     testing_env!(ctx);
//                     staking_pool
//                         .ops_stake_operator_command(StakingPoolOperatorCommand::StopStaking);
//                     let logs = test_utils::get_logs();
//                     println!("{:#?}", logs);
//                     assert_eq!(logs, vec!["[WARN] [STATUS_OFFLINE] Stopped",]);
//                     // Assert
//                     assert_eq!(
//                         staking_pool.ops_stake_status(),
//                         Status::Offline(OfflineReason::Stopped)
//                     );
//                     assert_eq!(
//                         staking_pool.ops_stake_pool_balances().total_unstaked,
//                         YoctoNear::ZERO
//                     );
//                     // since there is nothing staked, then we expect no stake actions
//                     assert!(deserialize_receipts().is_empty());
//                 }
//                 // Act - stop staking - when already stopped
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ADMIN.to_string();
//                     testing_env!(ctx);
//                     staking_pool
//                         .ops_stake_operator_command(StakingPoolOperatorCommand::StopStaking);
//                     assert!(test_utils::get_logs().is_empty());
//                     // Assert
//                     assert_eq!(
//                         staking_pool.ops_stake_status(),
//                         Status::Offline(OfflineReason::Stopped)
//                     );
//                     assert_eq!(
//                         staking_pool.ops_stake_pool_balances().total_unstaked,
//                         YoctoNear::ZERO
//                     );
//                     // since there is nothing staked, then we expect no stake actions
//                     assert!(deserialize_receipts().is_empty());
//                 }
//                 // Act - start staking after being stopped by operator
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ADMIN.to_string();
//                     testing_env!(ctx);
//                     staking_pool
//                         .ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
//                     let logs = test_utils::get_logs();
//                     println!("{:#?}", logs);
//                     assert_eq!(logs, vec!["[INFO] [STATUS_ONLINE] ",]);
//                     // Assert
//                     assert_eq!(staking_pool.ops_stake_status(), Status::Online);
//                     assert_eq!(
//                         staking_pool.ops_stake_pool_balances().total_unstaked,
//                         YoctoNear::ZERO
//                     );
//                     // since there is nothing staked, then we expect no stake actions
//                     assert!(deserialize_receipts().is_empty());
//                 }
//             }
//
//             #[test]
//             #[should_panic(expected = "[ERR] [NOT_AUTHORIZED]")]
//             fn not_authorized() {
//                 let (_ctx, mut staking_pool) = deploy_with_registered_account();
//                 staking_pool.ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
//             }
//
//             /// - deploy the staking pool
//             /// - stake 1 NEAR while pool is offline
//             /// - start staking pool
//             /// - stake 1 NEAR while pool is online
//             ///   - total staked balance should be 2 NEAR
//             ///   - treasury balance should contain staking fees for total staked balance
//             #[test]
//             fn stake_while_stopped_then_start() {
//                 let (ctx, mut staking_pool) = deploy_with_registered_account();
//
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ADMIN.to_string();
//                     ctx.attached_deposit = YOCTO;
//                     testing_env!(ctx);
//                     let state = *StakingPoolComponent::state();
//                     if let PromiseOrValue::Value(balances) = staking_pool.ops_stake() {
//                         assert_eq!(
//                             balances.staked.as_ref().unwrap().stake,
//                             (YOCTO - *(state.staking_fee * YOCTO.into())).into()
//                         );
//                     } else {
//                         panic!("expected Value")
//                     }
//                 }
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ADMIN.to_string();
//                     ctx.attached_deposit = 0;
//                     testing_env!(ctx);
//                     staking_pool
//                         .ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
//                     let logs = test_utils::get_logs();
//                     println!("{:#?}", logs);
//                     assert_eq!(logs, vec!["[INFO] [STATUS_ONLINE] ",]);
//
//                     let staking_pool_balances = staking_pool.ops_stake_pool_balances();
//                     assert_eq!(staking_pool_balances.total_staked, YOCTO.into());
//                     let receipts = deserialize_receipts();
//                     assert_eq!(receipts.len(), 2);
//                     {
//                         let receipt = &receipts[0];
//                         assert_eq!(receipt.receiver_id, env::current_account_id());
//                         assert_eq!(receipt.actions.len(), 1);
//                         let action = &receipt.actions[0];
//                         if let Action::Stake(action) = action {
//                             assert_eq!(action.stake, *staking_pool_balances.total_staked);
//                         } else {
//                             panic!("expected stake action");
//                         }
//                     }
//                     {
//                         let receipt = &receipts[1];
//                         assert_eq!(receipt.receiver_id, env::current_account_id());
//                         assert_eq!(receipt.actions.len(), 1);
//                         let action = &receipt.actions[0];
//                         if let Action::FunctionCall(action) = action {
//                             assert_eq!(action.method_name, "ops_stake_start_finalize");
//                         } else {
//                             panic!("expected function call action");
//                         }
//                     }
//                 }
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ADMIN.to_string();
//                     ctx.attached_deposit = YOCTO;
//                     ctx.account_locked_balance = YOCTO;
//                     testing_env!(ctx);
//                     if let PromiseOrValue::Value(_) = staking_pool.ops_stake() {
//                         panic!("expected Promise")
//                     }
//                     let logs = test_utils::get_logs();
//                     println!("{:#?}", logs);
//                     assert_eq!(
//                         logs,
//                         vec![
//                             "[INFO] [STAKE] near_amount=1000000000000000000000000, stake_token_amount=1000000000000000000000000",
//                         ]
//                     );
//                     let staking_pool_balances = staking_pool.ops_stake_pool_balances();
//                     assert_eq!(staking_pool_balances.total_staked, YOCTO.into());
//                 }
//             }
//
//             /// - deploy staking pool
//             /// - start staking pool
//             /// - stake 1 NEAR
//             /// - stop staking pool
//             /// - run `ops_stake_finalize()` callback
//             /// - stake 1 NEAR
//             ///   - total staked balance should be 2 NEAR
//             ///   - treasury balance should contain staking fees for total staked balance
//             #[test]
//             fn stop_staking_pool_while_stake_action_in_flight() {
//                 let (ctx, mut staking_pool) = deploy_with_registered_account();
//                 bring_pool_online(ctx.clone(), &mut staking_pool);
//
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ADMIN.to_string();
//                     ctx.attached_deposit = YOCTO;
//                     testing_env!(ctx);
//                     if let PromiseOrValue::Value(_balances) = staking_pool.ops_stake() {
//                         panic!("expected Promise")
//                     }
//                 }
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ADMIN.to_string();
//                     ctx.attached_deposit = 0;
//                     ctx.account_locked_balance = YOCTO;
//                     testing_env!(ctx);
//                     staking_pool
//                         .ops_stake_operator_command(StakingPoolOperatorCommand::StopStaking);
//                     let logs = test_utils::get_logs();
//                     println!("{:#?}", logs);
//                     assert_eq!(logs, vec!["[WARN] [STATUS_OFFLINE] Stopped",]);
//
//                     let receipts = deserialize_receipts();
//                     assert_eq!(receipts.len(), 2);
//                     {
//                         let receipt = &receipts[0];
//                         assert_eq!(receipt.receiver_id, env::current_account_id());
//                         assert_eq!(receipt.actions.len(), 1);
//                         let action = &receipt.actions[0];
//                         if let Action::Stake(action) = action {
//                             assert_eq!(action.stake, 0);
//                         } else {
//                             panic!("expected stake action");
//                         }
//                     }
//                     {
//                         let receipt = &receipts[1];
//                         assert_eq!(receipt.receiver_id, env::current_account_id());
//                         assert_eq!(receipt.actions.len(), 1);
//                         let action = &receipt.actions[0];
//                         if let Action::FunctionCall(action) = action {
//                             assert_eq!(action.method_name, "ops_stake_stop_finalize");
//                         } else {
//                             panic!("expected function call action");
//                         }
//                     }
//                 }
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ADMIN.to_string();
//                     ctx.attached_deposit = 0;
//                     ctx.account_locked_balance = YOCTO;
//                     testing_env!(ctx);
//                     let balances = staking_pool.ops_stake_finalize(ADMIN.to_string());
//                     let logs = test_utils::get_logs();
//                     println!("{:#?}", logs);
//                     assert_eq!(
//                         logs,
//                         vec![
//                             "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
//                             "[INFO] [FT_MINT] account: admin, amount: 1000000000000000000000000",
//                             "[INFO] [FT_BURN] account: admin, amount: 8000000000000000000000",
//                             "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(104)",
//                             "[INFO] [FT_MINT] account: contract.near, amount: 8000000000000000000000",
//                         ]
//                     );
//                     let state = *StakingPoolComponent::state();
//                     println!("{}", serde_json::to_string_pretty(&state).unwrap());
//
//                     println!("{:#?}", balances);
//                     let amount = YOCTO - *(state.staking_fee * YOCTO.into());
//                     assert_eq!(balances.staked.as_ref().unwrap().near_value, amount.into());
//                     assert_eq!(balances.staked.as_ref().unwrap().stake, amount.into())
//                 }
//             }
//
//             /// - deploy staking pool
//             /// - start staking pool
//             /// - stake 10 NEAR
//             /// - finalize stake
//             /// - unstake 1 NEAR
//             /// - stop staking pool
//             /// - run `ops_unstake_finalize()` callback
//             /// - stake 1 NEAR
//             ///   - total staked balance should be 2 NEAR
//             ///   - treasury balance should contain staking fees for total staked balance
//             #[test]
//             fn stop_staking_pool_while_unstake_action_in_flight() {
//                 let (ctx, mut staking_pool) = deploy_with_registered_account();
//
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ADMIN.to_string();
//                     ctx.attached_deposit = 2 * YOCTO;
//                     testing_env!(ctx);
//                     if let PromiseOrValue::Promise(_balances) = staking_pool.ops_stake() {
//                         panic!("expected value")
//                     }
//                 }
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ADMIN.to_string();
//                     ctx.attached_deposit = 0;
//                     testing_env!(ctx);
//                     staking_pool
//                         .ops_stake_operator_command(StakingPoolOperatorCommand::StartStaking);
//                 }
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ADMIN.to_string();
//                     ctx.attached_deposit = 0;
//                     ctx.account_locked_balance = 2 * YOCTO;
//                     testing_env!(ctx);
//                     if let PromiseOrValue::Value(_) = staking_pool.ops_unstake(Some(YOCTO.into())) {
//                         panic!("expected Promise")
//                     }
//                 }
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ADMIN.to_string();
//                     ctx.attached_deposit = 0;
//                     ctx.account_locked_balance = 2 * YOCTO;
//                     testing_env!(ctx);
//                     staking_pool
//                         .ops_stake_operator_command(StakingPoolOperatorCommand::StopStaking);
//                 }
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ADMIN.to_string();
//                     ctx.attached_deposit = 0;
//                     ctx.account_locked_balance = YOCTO;
//                     testing_env_with_promise_result_success(ctx);
//                     println!(
//                         "{}",
//                         serde_json::to_string_pretty(&*StakingPoolComponent::state()).unwrap()
//                     );
//                     println!("{:#?}", staking_pool.ops_stake_pool_balances());
//                     let stake_account_balances =
//                         staking_pool.ops_unstake_finalize(ADMIN.to_string());
//                     let logs = test_utils::get_logs();
//                     println!("{:#?}", logs);
//                     assert_eq!(
//                         logs,
//                         vec![
//                             "[INFO] [FT_BURN] account: admin, amount: 1000000000000000000000000",
//                             "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(184)",
//                         ]
//                     );
//                     assert_eq!(
//                         stake_account_balances.staked.as_ref().unwrap().near_value,
//                         (YOCTO - *(StakingPoolComponent::state().staking_fee * (2 * YOCTO).into()))
//                             .into()
//                     );
//                     assert_eq!(
//                         stake_account_balances.unstaked.as_ref().unwrap().total,
//                         YOCTO.into()
//                     );
//                 }
//             }
//         }
//
//         #[cfg(test)]
//         mod update_public_key {
//             use super::*;
//
//             #[test]
//             fn pool_is_offline_as_operator() {
//                 let (ctx, mut staking_pool) = deploy_with_registered_account();
//
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ADMIN.to_string();
//                     testing_env!(ctx.clone());
//                     account_manager().ops_permissions_grant_operator(to_valid_account_id(ACCOUNT));
//                 }
//
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ACCOUNT.to_string();
//                     testing_env!(ctx);
//                     let key = [1_u8; 65];
//                     let key: PublicKey = key[..].try_into().unwrap();
//                     staking_pool.ops_stake_operator_command(
//                         StakingPoolOperatorCommand::UpdatePublicKey(key),
//                     );
//                     assert_eq!(StakingPoolComponent::state().stake_public_key, key);
//                 }
//             }
//
//             #[test]
//             #[should_panic(
//                 expected = "[ERR] [ILLEGAL_STATE] staking pool must be paused to update the staking public key"
//             )]
//             fn pool_is_online_as_operator() {
//                 let (ctx, mut staking_pool) = deploy_with_registered_account();
//                 bring_pool_online(ctx.clone(), &mut staking_pool);
//
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ADMIN.to_string();
//                     testing_env!(ctx.clone());
//                     account_manager().ops_permissions_grant_operator(to_valid_account_id(ACCOUNT));
//                 }
//
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ACCOUNT.to_string();
//                     testing_env!(ctx);
//                     let key = [1_u8; 65];
//                     let key: PublicKey = key[..].try_into().unwrap();
//                     staking_pool.ops_stake_operator_command(
//                         StakingPoolOperatorCommand::UpdatePublicKey(key),
//                     );
//                 }
//             }
//
//             #[test]
//             #[should_panic(expected = "[ERR] [NOT_AUTHORIZED]")]
//             fn not_authorized() {
//                 let (ctx, mut staking_pool) = deploy_with_registered_account();
//
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ACCOUNT.to_string();
//                     testing_env!(ctx);
//                     let key = [1_u8; 65];
//                     let key: PublicKey = key[..].try_into().unwrap();
//                     staking_pool.ops_stake_operator_command(
//                         StakingPoolOperatorCommand::UpdatePublicKey(key),
//                     );
//                 }
//             }
//         }
//
//         #[cfg(test)]
//         mod update_staking_fee {
//             use super::*;
//
//             #[test]
//             fn as_operator() {
//                 let (ctx, mut staking_pool) = deploy_with_registered_account();
//
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ADMIN.to_string();
//                     testing_env!(ctx.clone());
//                     account_manager().ops_permissions_grant_operator(to_valid_account_id(ACCOUNT));
//                 }
//
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ACCOUNT.to_string();
//                     testing_env!(ctx);
//                     staking_pool.ops_stake_operator_command(
//                         StakingPoolOperatorCommand::UpdateStakingFee(100.into()),
//                     );
//                     assert_eq!(StakingPoolComponent::state().staking_fee, 100.into());
//                 }
//             }
//
//             #[test]
//             #[should_panic(expected = "[ERR] [INVALID] max staking fee is 1000 BPS (10%)")]
//             fn as_operator_above_max() {
//                 let (ctx, mut staking_pool) = deploy_with_registered_account();
//
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ADMIN.to_string();
//                     testing_env!(ctx.clone());
//                     account_manager().ops_permissions_grant_operator(to_valid_account_id(ACCOUNT));
//                 }
//
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ACCOUNT.to_string();
//                     testing_env!(ctx);
//                     staking_pool.ops_stake_operator_command(
//                         StakingPoolOperatorCommand::UpdateStakingFee(1001.into()),
//                     );
//                 }
//             }
//
//             #[test]
//             #[should_panic(expected = "[ERR] [NOT_AUTHORIZED]")]
//             fn not_authorized() {
//                 let (ctx, mut staking_pool) = deploy_with_registered_account();
//
//                 {
//                     let mut ctx = ctx.clone();
//                     ctx.predecessor_account_id = ACCOUNT.to_string();
//                     testing_env!(ctx);
//                     staking_pool.ops_stake_operator_command(
//                         StakingPoolOperatorCommand::UpdateStakingFee(100.into()),
//                     );
//                 }
//             }
//         }
//     }
// }

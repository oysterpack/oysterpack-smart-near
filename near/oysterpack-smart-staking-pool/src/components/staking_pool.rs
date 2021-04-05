use crate::{
    StakeAccountBalances, StakeActionCallbacks, StakeBalance, StakingPool, StakingPoolOperator,
    StakingPoolOperatorCommand, StakingPoolOwner, ERR_STAKED_BALANCE_TOO_LOW_TO_UNSTAKE,
    ERR_STAKE_ACTION_FAILED, LOG_EVENT_NOT_ENOUGH_TO_STAKE, LOG_EVENT_STATUS_OFFLINE,
    LOG_EVENT_STATUS_ONLINE,
};
use oysterpack_smart_account_management::{
    components::account_management::AccountManagementComponent, AccountRepository,
    StorageManagement, ERR_ACCOUNT_NOT_REGISTERED,
};
use oysterpack_smart_contract::{
    components::contract_ownership::ContractOwnershipComponent, BalanceId, ContractNearBalances,
    ContractOwnerObject, ContractOwnership,
};
use oysterpack_smart_fungible_token::{
    components::fungible_token::FungibleTokenComponent, FungibleToken, TokenAmount, TokenService,
};
use oysterpack_smart_near::near_sdk::{AccountId, PromiseOrValue};
use oysterpack_smart_near::{
    asserts::{assert_near_attached, ERR_INSUFFICIENT_FUNDS, ERR_INVALID},
    component::{Component, ComponentState, Deploy},
    data::numbers::U256,
    domain::{
        ActionType, ByteLen, Gas, PublicKey, SenderIsReceiver, TransactionResource, YoctoNear,
        ZERO_NEAR,
    },
    json_function_callback,
    near_sdk::{
        borsh::{self, BorshDeserialize, BorshSerialize},
        env, is_promise_success,
        json_types::ValidAccountId,
        serde::{Deserialize, Serialize},
        Promise,
    },
    to_valid_account_id, TERA, YOCTO,
};

type StakeAccountData = ();

pub struct StakingPoolComponent {
    account_manager: AccountManagementComponent<StakeAccountData>,
    stake_token: FungibleTokenComponent<StakeAccountData>,
    contract_ownership: ContractOwnershipComponent,
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

    // tracks how funds have been staked and unstaked and pending the stake action
    // - applies only when the status is online - when offline, the values will be zero
    pub staked: YoctoNear,
    pub unstaked: YoctoNear,

    // used to override the built in computed gas amount
    pub callback_gas: Option<Gas>,
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
                (
                    TransactionResource::DataReceipt(SenderIsReceiver(false), ByteLen(200)),
                    1,
                ),
            ]) + TERA.into()
        }
    }

    fn total_staked_balance(&self) -> YoctoNear {
        match self.status {
            Status::Online => {
                YoctoNear(env::account_locked_balance()) + self.staked - self.unstaked
            }
            Status::Offline(_) => ContractNearBalances::load_near_balances()
                .get(&Self::TOTAL_STAKED_BALANCE)
                .cloned()
                .unwrap_or(ZERO_NEAR),
        }
    }
}

#[derive(
    BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug,
)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub enum Status {
    /// While offline, accounts can still stake, but the funds are held until are held until the
    /// staking pool goes online
    /// - when the pool goes back online, then the staked funds are staked
    Offline(OfflineReason),
    /// the pool is actively staking
    Online,
}

impl Status {
    pub fn is_online(&self) -> bool {
        match self {
            Status::Offline(_) => false,
            Status::Online => true,
        }
    }
}

#[derive(
    BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug,
)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
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
            staked: ZERO_NEAR,
            unstaked: ZERO_NEAR,
            callback_gas: None,
        };
        let state = Self::new_state(state);
        state.save();
    }
}

pub struct StakingPoolComponentConfig {
    pub stake_public_key: PublicKey,
}

pub const MIN_STAKE_AMOUNT: YoctoNear = YoctoNear(1000);

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

    fn ops_stake(&mut self) -> PromiseOrValue<StakeAccountBalances> {
        assert_near_attached("deposit is required to stake");
        let account_id = env::predecessor_account_id();
        let mut account = self
            .account_manager
            .registered_account_near_data(&account_id);

        // all of the account's storage available balance will be staked
        let (near_amount, stake_token_amount) = {
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

            (stake_near_value, stake)
        };

        if *near_amount == 0 {
            LOG_EVENT_NOT_ENOUGH_TO_STAKE.log("");
            return PromiseOrValue::Value(
                self.ops_stake_balance(to_valid_account_id(&account_id))
                    .unwrap(),
            );
        }

        let mut state = Self::state();
        match state.status {
            Status::Online => {
                state.staked += near_amount;
                state.save();
                PromiseOrValue::Promise(Self::stake_funds(
                    *state,
                    &account_id,
                    near_amount,
                    stake_token_amount,
                ))
            }
            Status::Offline(_) => {
                LOG_EVENT_STATUS_OFFLINE.log("");
                ContractNearBalances::incr_balance(State::TOTAL_STAKED_BALANCE, near_amount);
                self.stake_token.ft_mint(&account_id, stake_token_amount);

                PromiseOrValue::Value(
                    self.ops_stake_balance(to_valid_account_id(&account_id))
                        .unwrap(),
                )
            }
        }
    }

    fn ops_unstake(&mut self, amount: Option<YoctoNear>) -> PromiseOrValue<StakeAccountBalances> {
        let account_id = env::predecessor_account_id();
        ERR_ACCOUNT_NOT_REGISTERED.assert(|| self.account_manager.account_exists(&account_id));

        let stake_balance = self
            .stake_token
            .ft_balance_of(to_valid_account_id(&account_id));
        let stake_near_value = self.stake_near_value_rounded_down(stake_balance);
        let near_amount = amount.unwrap_or(stake_near_value);

        ERR_INSUFFICIENT_FUNDS.assert(|| stake_near_value >= near_amount);
        let stake_token_amount = self.near_stake_value_rounded_up(near_amount);
        ERR_STAKED_BALANCE_TOO_LOW_TO_UNSTAKE.assert(|| stake_balance >= stake_token_amount);

        let mut state = Self::state();
        match state.status {
            Status::Online => {
                state.unstaked += near_amount;
                state.save();
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
                ContractNearBalances::decr_balance(State::TOTAL_STAKED_BALANCE, near_amount);
                self.stake_token.ft_burn(&account_id, stake_token_amount);

                PromiseOrValue::Value(
                    self.ops_stake_balance(to_valid_account_id(&account_id))
                        .unwrap(),
                )
            }
        }
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
}

impl StakingPoolOperator for StakingPoolComponent {
    fn ops_stake_operator_command(&mut self, command: StakingPoolOperatorCommand) {
        self.account_manager.assert_operator();

        match command {
            StakingPoolOperatorCommand::Pause => {
                let mut state = Self::state();
                if let Status::Online = state.status {
                    // unstake all
                    {
                        let staked_balance = state.total_staked_balance();
                        if *staked_balance > 0 {
                            ContractNearBalances::set_balance(
                                State::TOTAL_STAKED_BALANCE,
                                staked_balance,
                            );
                            Promise::new(env::current_account_id())
                                .stake(0, state.stake_public_key.into());
                        }
                    }
                    // update the status
                    {
                        state.status = Status::Offline(OfflineReason::Paused);
                        state.save();
                    }
                    LOG_EVENT_STATUS_OFFLINE.log("");
                }
            }
            StakingPoolOperatorCommand::Resume => {
                let mut state = Self::state();
                if let Status::Offline(_) = state.status {
                    // update status
                    {
                        state.status = Status::Online;
                        state.save();
                    }
                    LOG_EVENT_STATUS_ONLINE.log("");

                    // stake
                    {
                        let total_staked_balance =
                            ContractNearBalances::near_balance(State::TOTAL_STAKED_BALANCE);
                        if total_staked_balance > ZERO_NEAR {
                            ContractNearBalances::clear_balance(State::TOTAL_STAKED_BALANCE);
                            Promise::new(env::current_account_id())
                                .stake(*total_staked_balance, state.stake_public_key.into())
                                .then(json_function_callback(
                                    "ops_stake_resume_finalize",
                                    Some(RestakeActionCallbackArgs {
                                        total_staked_balance,
                                    }),
                                    ZERO_NEAR,
                                    state.callback_gas(),
                                ));
                        }
                    }
                }
            }
            StakingPoolOperatorCommand::SetStakeCallbackGas(gas) => {
                let min_callback_gas = State::min_callback_gas();
                ERR_INVALID.assert(
                    || gas >= min_callback_gas,
                    || format!("minimum callback gas required is: {}", min_callback_gas),
                );
                let mut state = Self::state();
                state.callback_gas = Some(gas);
                state.save();
            }
            StakingPoolOperatorCommand::ClearStakeCallbackGas => {
                let mut state = Self::state();
                state.callback_gas = None;
                state.save();
            }
        }
    }

    fn ops_stake_callback_gas(&self) -> Gas {
        Self::state().callback_gas()
    }

    fn ops_stake_state(&self) -> State {
        *Self::state()
    }
}

impl StakingPoolOwner for StakingPoolComponent {
    fn ops_stake_owner_balance(&mut self, amount: Option<YoctoNear>) -> StakeAccountBalances {
        ContractOwnerObject::assert_owner_access();
        let balance = self.contract_ownership.ops_owner_balance();
        let amount = amount.unwrap_or_else(|| balance.available);
        ERR_INSUFFICIENT_FUNDS.assert(|| balance.available >= amount);
        let owner_account_id = env::predecessor_account_id();

        match self
            .account_manager
            .load_account_near_data(&owner_account_id)
        {
            None => {
                self.account_manager
                    .register_account(&owner_account_id, amount, false);
            }
            Some(mut account) => {
                account.incr_near_balance(amount);
                account.save();
            }
        }

        self.ops_stake_balance(to_valid_account_id(&owner_account_id))
            .unwrap()
    }
}

impl StakeActionCallbacks for StakingPoolComponent {
    fn ops_stake_finalize(
        &mut self,
        account_id: AccountId,
        amount: YoctoNear,
        stake_token_amount: TokenAmount,
        total_staked_balance: YoctoNear,
    ) -> StakeAccountBalances {
        if is_promise_success() {
            let mut state = Self::state();
            state.staked -= amount;
            self.stake_token.ft_mint(&account_id, stake_token_amount);
        } else {
            Self::handle_stake_action_failure(total_staked_balance);
        }

        self.ops_stake_balance(to_valid_account_id(&account_id))
            .unwrap()
    }

    fn ops_unstake_finalize(
        &mut self,
        account_id: AccountId,
        amount: YoctoNear,
        stake_token_amount: TokenAmount,
        total_staked_balance: YoctoNear,
    ) -> StakeAccountBalances {
        if is_promise_success() {
            let mut state = Self::state();
            state.unstaked -= amount;

            self.stake_token.ft_burn(&account_id, stake_token_amount);

            let mut account = self
                .account_manager
                .registered_account_near_data(&account_id);
            account.incr_near_balance(amount);
            account.save();
        } else {
            Self::handle_stake_action_failure(total_staked_balance);
        }

        self.ops_stake_balance(to_valid_account_id(&account_id))
            .unwrap()
    }

    fn ops_stake_resume_finalize(&mut self, total_staked_balance: YoctoNear) {
        if !is_promise_success() {
            Self::handle_stake_action_failure(total_staked_balance);
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
struct StakeActionCallbackArgs {
    account_id: AccountId,
    amount: YoctoNear,
    stake_token_amount: TokenAmount,
    total_staked_balance: YoctoNear,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
struct RestakeActionCallbackArgs {
    pub total_staked_balance: YoctoNear,
}

impl StakingPoolComponent {
    fn state() -> ComponentState<State> {
        Self::load_state().expect("component has not been deployed")
    }

    fn set_status(status: Status) {
        let mut state = Self::state();
        state.status = status;
        state.save();
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
                total_staked_balance,
            }),
            ZERO_NEAR,
            state.callback_gas(),
        );
        stake.then(finalize)
    }

    fn handle_stake_action_failure(total_staked_balance: YoctoNear) {
        ERR_STAKE_ACTION_FAILED.log("");
        ContractNearBalances::set_balance(State::TOTAL_STAKED_BALANCE, total_staked_balance);
        if env::account_locked_balance() > 0 {
            Promise::new(env::current_account_id()).stake(0, Self::state().stake_public_key.into());
            Self::set_status(Status::Offline(OfflineReason::StakeActionFailed));
        }
    }

    fn stake_near_value_rounded_down(&self, stake: TokenAmount) -> YoctoNear {
        if *stake == 0 {
            return ZERO_NEAR;
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

    fn near_stake_value_rounded_down(&self, amount: YoctoNear) -> TokenAmount {
        if *amount == 0 {
            return 0.into();
        }

        let total_staked_near_balance = *Self::state().total_staked_balance();
        let ft_total_supply = *self.stake_token.ft_total_supply();
        if total_staked_near_balance == 0 || ft_total_supply == 0 {
            return (*amount).into();
        }

        (U256::from(ft_total_supply) * U256::from(*amount) / U256::from(total_staked_near_balance))
            .as_u128()
            .into()
    }

    fn near_stake_value_rounded_up(&self, amount: YoctoNear) -> TokenAmount {
        if *amount == 0 {
            return 0.into();
        }

        let total_staked_near_balance = *Self::state().total_staked_balance();
        let ft_total_supply = *self.stake_token.ft_total_supply();
        if total_staked_near_balance == 0 || ft_total_supply == 0 {
            return amount.value().into();
        }

        ((U256::from(ft_total_supply) * U256::from(*amount)
            + U256::from(total_staked_near_balance - 1))
            / U256::from(total_staked_near_balance))
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
    use oysterpack_smart_near::near_sdk::{env, serde_json, VMContext};
    use oysterpack_smart_near::{component::*, *};
    use oysterpack_smart_near_test::*;
    use std::convert::*;

    type AccountManager = AccountManagementComponent<()>;
    type StakeFungibleToken = FungibleTokenComponent<()>;

    fn account_manager() -> AccountManager {
        StakeFungibleToken::register_storage_management_event_handler();
        AccountManager::default()
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
                    available: ZERO_NEAR
                },
                stake_token_balance: None
            })
        );

        // Act - accounts can stake while the pool is offline
        {
            let mut ctx = ctx.clone();
            ctx.attached_deposit = YOCTO;
            testing_env!(ctx.clone());
            assert_eq!(env::account_locked_balance(), 0);
            if let PromiseOrValue::Value(balance) = staking_pool.ops_stake() {
                assert_eq!(
                    balance.stake_token_balance,
                    Some(StakeBalance {
                        stake: YOCTO.into(),
                        near_value: YOCTO.into()
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
            staking_pool.ops_stake_operator_command(StakingPoolOperatorCommand::Resume);
            assert!(staking_pool.ops_stake_status().is_online());
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
        }
    }
}

use crate::Status;
use crate::{StakeAccountBalances, StakingPoolBalances};
use oysterpack_smart_fungible_token::{Memo, TokenAmount, TransferCallMessage};
use oysterpack_smart_near::domain::{BasisPoints, PublicKey, YoctoNear};
use oysterpack_smart_near::near_sdk::json_types::ValidAccountId;
use oysterpack_smart_near::near_sdk::{Promise, PromiseOrValue};
use oysterpack_smart_near::{ErrCode, ErrorConst, Level, LogEvent};

/// # **Contract Interface**: Staking Pool API
///
/// Staking pools enable accounts to delegate NEAR to stake with a validator. The main benefits of using
/// this staking pool are:
/// 1. STAKE fungible token is provided for staked NEAR. This enables staked NEAR value to be transferred
///    while still being staked.
/// 2. Unstaked NEAR is locked for 4 epochs before it becomes available to be withdrawn, but is tracked
///    per epoch. Thus, more funds can be unstaked without affecting funds that were unstaked in previous
///    epochs. Compare this to the NEAR provided staking pool, where each time you unstake, it resets
///    the lockup period to 4 epochs for the total unstaked NEAR balance. For example, if 100 NEAR is
///    unstaked in EPOCH 1 and 10 NEAR is unstaked in EPOCH 3. Then 100 NEAR is available for
///    withdrawal in EPOCH 5 and 10 NEAR in EPOCH 7. In the current NEAR provided staking pool
///    implementation, unstaking in the 10 NEAR in EPOCH 3 would reset the lock period for the total
///    unstaked, i.e., you would not be able to withdraw the 100 NEAR that was unstaked in EPOCH 1
///    until EPOCH 7.
/// 3. Staking adds liquidity for withdrawing unstaked NEAR that is locked on a first come, first
///    withdraw basis.
/// 4. Transaction fee based model
///    - instead of charging delegators a percentage of the staking rewards, a transaction fee based
///      model is used:
///      - configurable staking fee (0.8%)
///    - fees are deposited in the staking pool treasury fund
/// 5. Profit sharing through dividends
///    - staking rewards earned by the treasury are distributed as dividends by burning STAKE tokens,
///      which boosts the yield, i.e., STAKE token value
///    - dividends are paid out on each staking event, i.e., if the treasury has received staking
///      rewards since the last staking, then the STAKE token equivalent will be burned
///
/// The staking pool is integrated with the storage management API:
/// - accounts must be registered with the contract in order to stake
/// - when staking, the account's available storage balance will be staked in addition to the
///   attached deposit
///
/// The staking pool is integrated with fungible token API to support treasury deposits:
/// - STAKE received through direct transfers are treated as treasury distributions. The next time
///   funds are staked, the funds will be distributed as a treasury dividend.
/// - STAKE received through transfer calls are treated as treasury deposits, i.e., the treasury
///   balance will be updated to reflect the deposit.
///
pub trait StakingPool {
    /// Consolidates the account's storage balance with the STAKE token balance
    ///
    /// Returns None if the account is not registered with the contract
    fn ops_stake_balance(&self, account_id: ValidAccountId) -> Option<StakeAccountBalances>;

    /// Used to stake NEAR for the predecessor's account.
    ///
    /// Any attached deposit will be fully staked in addition to any available account storage balance.
    ///
    /// Returns the account's updated stake account balance after the contract's stake action completes
    ///
    /// ## NOTES
    /// - When NEAR is staked, STAKE tokens are minted. Because values are rounded down, only the actual
    ///   STAKE NEAR value is minted. The remainder is credited to the account's storage balance. The
    ///   algorithm is:
    ///    1. NEAR stake amount = attached deposit + account storage available balance
    ///    2. compute the  NEAR stake amount in STAKE
    ///    3. convert the STAKE back to NEAR
    ///    4. STAKE NEAR value is staked
    ///    5. remainder is credited to the account's storage balance
    /// - When NEAR is staked, it is first converted to STAKE and rounded down. Thus, based on the current
    ///   exchange ratio, a minimum amount of NEAR is required to stake. If there is not enough to stake
    ///   then the funds will be transferred over to the account's storage balance.
    ///   - [`LOG_EVENT_NOT_ENOUGH_TO_STAKE`] event will be logged
    /// - the NEAR stake action is async - thus, balances will not be updated until the stake action
    ///   has completed
    /// - if there was no attached deposit and zero available storage balance, then the current balances
    ///   are simply returned
    ///
    /// ## Panics
    /// - if the account is not registered
    ///
    /// `#[payable]`
    fn ops_stake(&mut self) -> PromiseOrValue<StakeAccountBalances>;

    /// Used to unstake staked NEAR.
    ///
    /// If amount is not specified, then the full staked balance will be unstaked.
    ///
    /// ## Notes
    /// - If unstaking all, i.e., `amount` is None, then a zero staked balance is fine. However, if
    ///   an `amount` is specified, then the method will panic if there are insufficient staked funds
    ///   to fulfill the request
    ///
    /// ## Panics
    /// - if account is not registered
    /// - if there are insufficient staked funds to fulfill the request to unstake the specified amount
    fn ops_unstake(&mut self, amount: Option<YoctoNear>) -> PromiseOrValue<StakeAccountBalances>;

    /// Re-stakes unstaked funds
    ///
    /// If amount is not specified, then the full unstaked balance will be re-staked.
    ///
    /// ## Notes
    /// - If unstaking all, i.e., `amount` is None, then a zero staked balance is fine. However, if
    ///   an `amount` is specified, then the method will panic if there are insufficient staked funds
    ///   to fulfill the request
    /// - the actual NEAR amount restaked might be less than the requested specified amount because
    ///   of rounding when converting the restake NEAR amount to STAKE - check balances to confirm
    ///   the actual amount restaked
    ///
    /// ## Panics
    /// - if account is not registered
    /// - if there are insufficient funds to fulfill the request
    fn ops_restake(&mut self, amount: Option<YoctoNear>) -> PromiseOrValue<StakeAccountBalances>;

    /// Withdraws unstaked NEAR that is not locked.
    ///
    /// If no amount is specified, then all available unstaked NEAR will be withdrawn.
    ///
    /// ## Panics
    /// - if account is not registered
    /// - if there are insufficient funds to fulfill the request
    fn ops_stake_withdraw(&mut self, amount: Option<YoctoNear>) -> StakeAccountBalances;

    /// converts the specified NEAR amount to STAKE and transfers the funds to the specified receiver
    /// account
    /// - proxies [`oysterpack_smart_fungible_token::FungibleToken::ft_transfer`] as a convenience
    ///   method that enables the staker to transfer STAKE by specifying the transfer amount in NEAR vs in STAKE
    /// - returns the amount of STAKE tokens that were transferred based on the current STAKE token value
    ///
    /// ## NOTES
    /// - earnings are applied before the transfer
    ///
    /// `#[payable]`
    fn ops_stake_transfer(
        &mut self,
        receiver_id: ValidAccountId,
        amount: YoctoNear,
        memo: Option<Memo>,
    ) -> TokenAmount;

    /// converts the specified NEAR amount to STAKE and transfers the funds to the specified receiver
    /// account via the FT transfer call mechanism
    /// - proxies [`oysterpack_smart_fungible_token::FungibleToken::ft_transfer_call`] a convenience
    ///   method that enables the staker to transfer STAKE by specifying the transfer amount in NEAR vs in STAKE
    /// - returns the amount of STAKE tokens that were transferred
    ///
    /// ## NOTES
    /// - earnings are applied before the transfer
    ///
    /// `#[payable]`
    fn ops_stake_transfer_call(
        &mut self,
        receiver_id: ValidAccountId,
        amount: YoctoNear,
        memo: Option<Memo>,
        msg: TransferCallMessage,
    ) -> Promise;

    /// returns the current NEAR value for the specified amount
    /// - if no amount is specified, then the value for 1 STAKE token will be returned
    /// - value includes estimated earnings minus dividend payouts
    /// - if you need the most accurate STAKE token value, then use `[Self::ops_stake_token_value_with_updated_earnings`],
    ///   which is not a view method
    fn ops_stake_token_value(&self, amount: Option<TokenAmount>) -> YoctoNear;

    /// Collects earnings and pays dividend before computing and returning the STAKE token value.
    ///
    /// To avoid gas fees to get a STAKE token estimate minus dividends you can use [`Self::`ops_stake_token_value].
    ///
    /// ## NOTES
    /// Calling this consecutive times should return an increasing STAKE token value because the
    /// transaction gas fees earned from this call are applied on the next.
    fn ops_stake_token_value_with_updated_earnings(
        &mut self,
        amount: Option<TokenAmount>,
    ) -> YoctoNear;

    fn ops_stake_status(&self) -> Status;

    fn ops_stake_pool_balances(&self) -> StakingPoolBalances;

    fn ops_stake_fee(&self) -> BasisPoints;

    fn ops_stake_public_key(&self) -> PublicKey;
}

pub const LOG_EVENT_STATUS_ONLINE: LogEvent = LogEvent(Level::INFO, "STATUS_ONLINE");
pub const LOG_EVENT_STATUS_OFFLINE: LogEvent = LogEvent(Level::WARN, "STATUS_OFFLINE");

pub const LOG_EVENT_NOT_ENOUGH_TO_STAKE: LogEvent = LogEvent(Level::INFO, "NOT_ENOUGH_TO_STAKE");

pub const LOG_EVENT_STAKE: LogEvent = LogEvent(Level::INFO, "STAKE");
pub const LOG_EVENT_UNSTAKE: LogEvent = LogEvent(Level::INFO, "UNSTAKE");

pub const LOG_EVENT_TREASURY_DIVIDEND: LogEvent = LogEvent(Level::INFO, "TREASURY_DIVIDEND");
pub const LOG_EVENT_TREASURY_DEPOSIT: LogEvent = LogEvent(Level::INFO, "TREASURY_DEPOSIT");
pub const LOG_EVENT_EARNINGS: LogEvent = LogEvent(Level::INFO, "EARNINGS");

pub const LOG_EVENT_LIQUIDITY: LogEvent = LogEvent(Level::INFO, "LIQUIDITY");

pub const ERR_STAKED_BALANCE_TOO_LOW_TO_UNSTAKE: ErrorConst =
    ErrorConst(ErrCode("STAKED_BALANCE_TOO_LOW_TO_UNSTAKE"), "");

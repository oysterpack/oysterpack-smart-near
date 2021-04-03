use crate::components::staking_pool::Status;
use crate::StakeAccountBalances;
use oysterpack_smart_near::domain::YoctoNear;
use oysterpack_smart_near::near_sdk::json_types::ValidAccountId;
use oysterpack_smart_near::{ErrCode, ErrorConst, Level, LogEvent};

/// # **Contract Interface**: Staking Pool API
///
/// Staking pools enable accounts to delegate NEAR to stake with a validator. The main benefits of using
/// this staking pool are:
/// 1. The staking pool does not support lockup contracts. The benefit is this lifts the restriction
///    to lock unstaked NEAR for withdrawal. Unstaked NEAR can be immediately withdrawn. The tradeoff
///    is that lockup contracts will not be allowed to delegate their NEAR to stake with this staking
///    pool because they are only permitted to go through a NEAR managed whitelisted staking pool
///    that guarantees that unstaked NEAR will be locked for 4 epochs.
/// 2. STAKE fungible token is provided for staked NEAR. This enables staked NEAR value to be transferred
///    while still being staked.
///
/// The staking pool works with the storage management API:
/// - accounts must be registered with the contract in order to stake
/// - when staking, the account's available storage balance will be staked in addition to the attached deposit
/// - the storage management APIs provide the withdrawal functionality, i.e., when unstaked near becomes
///   available to withdraw, then it will appear as available balance on the storage management API
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
    /// Returns the account's updated stake account balance
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
    ///
    /// ## Panics
    /// - if the account is not registered
    /// - if there is no attached deposit
    ///
    /// `#[payable]`
    fn ops_stake(&mut self) -> StakeAccountBalances;

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
    fn ops_unstake(&mut self, amount: Option<YoctoNear>) -> StakeAccountBalances;

    /// returns the current NEAR value of 1 STAKE token
    fn ops_stake_token_value(&self) -> YoctoNear;

    fn ops_stake_status(&self) -> Status;
}

pub const LOG_EVENT_STATUS_ONLINE: LogEvent = LogEvent(Level::INFO, "STATUS_ONLINE");
pub const LOG_EVENT_STATUS_OFFLINE: LogEvent = LogEvent(Level::WARN, "STATUS_OFFLINE");

pub const LOG_EVENT_NOT_ENOUGH_TO_STAKE: LogEvent = LogEvent(Level::INFO, "NOT_ENOUGH_TO_STAKE");

pub const ERR_STAKED_BALANCE_TOO_LOW_TO_UNSTAKE: ErrorConst =
    ErrorConst(ErrCode("STAKED_BALANCE_TOO_LOW_TO_UNSTAKE"), "");

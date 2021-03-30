use crate::StakeAccountBalances;
use oysterpack_smart_near::domain::YoctoNear;
use oysterpack_smart_near::near_sdk::json_types::ValidAccountId;
use oysterpack_smart_near::{Level, LogEvent};

/// # **Contract Interface**: Staking Pool API
///
/// Staking pools enable accounts to delegate NEAR to stake with a validator.
///
/// The staking pool works with the storage management API:
/// - when funds are deposited, accounts will automatically be registered, i.e., a portion of the
///   deposit will be used to pay for the account's contract storage
/// - the storage management APIs provide the withdrawal functionality, i.e., when unstaked near becomes
///   available to withdraw, then it will appear as available balance on the storage management API
///
///
pub trait StakingPool {
    /// Looks up the account's stake account balance
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
    /// When NEAR is staked, STAKE tokens are minted. Because values are rounded down, only the actual
    /// STAKE NEAR value is minted. The remainder is credited to the account's storage balance. The
    /// algorithm is:
    /// 1. NEAR stake amount = attached deposit + account storage available balance
    /// 2. compute the  NEAR stake amount in STAKE
    /// 3. convert the STAKE back to NEAR
    /// 4. STAKE NEAR value is staked
    /// 5. remainder is credited to the account's storage balance
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

    /// Used to withdraw unstaked NEAR funds
    /// - if the unstaked NEAR is still locked, then the liquidity pool will be checked
    ///
    /// If amount is not specified, then all available unstaked NEAR will be withdrawn.
    ///
    /// ## Panics
    /// - if account is not registered
    /// - if exactly 1 yoctoNEAR is not attached
    /// - If the specified withdrawal amount is greater than the account's available unstaked balance
    ///
    /// `#[payable]`
    fn ops_stake_withdraw(&mut self, amount: Option<YoctoNear>) -> StakeAccountBalances;

    /// returns the current NEAR value of 1 STAKE token
    fn ops_stake_token_value(&self) -> YoctoNear;

    /// Returns the amount of liquidity that is available for withdrawing unstaked NEAR.
    ///
    /// Liquidity is automatically added by delegators when they stake their NEAR while there is
    /// locked unstaked NEAR.
    fn ops_stake_available_liquidity(&self) -> YoctoNear;
}

pub const LOG_EVENT_STAKE: LogEvent = LogEvent(Level::INFO, "STAKE");
pub const LOG_EVENT_UNSTAKE: LogEvent = LogEvent(Level::INFO, "UNSTAKE");

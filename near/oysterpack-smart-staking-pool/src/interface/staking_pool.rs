use crate::StakeAccountBalances;
use oysterpack_smart_near::domain::YoctoNear;
use oysterpack_smart_near::near_sdk::json_types::ValidAccountId;
use oysterpack_smart_near::near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
};

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
    /// Any attached deposit will be fully staked.
    ///
    /// The specified amount is used to specify how much to stake from the account's available and
    /// unstaked balances.
    ///
    /// Returns the account's updated stake account balance
    ///
    /// ## Panics
    /// - if the account is not registered
    /// - if there is no attached deposit and no amount is specified - at least 1 is required
    ///
    /// `#[payable]`
    fn ops_stake(&mut self, amount: Option<StakeAmount>) -> StakeAccountBalances;

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

#[derive(
    BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, Clone, Copy, PartialEq,
)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub enum StakeAmount {
    /// stakes all unstaked NEAR and all available balance from account storage
    All,
    /// re-stakes all of the unstaked balance
    AllUnstaked,

    /// stakes from the account's available and unstaked balances - starting from the most recent
    /// unstaked balance
    Total(YoctoNear),
    /// re-stakes the specified unstaked amount - starting from the most recent unstaked balance
    Unstaked(YoctoNear),
}

use crate::StakeAccountBalances;
use oysterpack_smart_fungible_token::TokenAmount;
use oysterpack_smart_near::domain::YoctoNear;
use oysterpack_smart_near::near_sdk::AccountId;
use oysterpack_smart_near::ErrCode;

/// # **Contract Interface**: Private Staking Pool Callbacks
pub trait StakeActionCallbacks {
    /// Finalizes the stake action when funds are staked
    ///
    /// If the stake action failed, then the contract will fully unstake and go offline.
    ///
    /// `#[private]`
    fn ops_stake_finalize(
        &mut self,
        account_id: AccountId,
        amount: YoctoNear,
        stake_token_amount: TokenAmount,
    ) -> StakeAccountBalances;

    /// Finalizes the stake action when funds are unstaked
    ///
    /// If the stake action failed, then the contract will fully unstake and go offline.
    ///
    /// `#[private]`
    fn ops_unstake_finalize(
        &mut self,
        account_id: AccountId,
        amount: YoctoNear,
        stake_token_amount: TokenAmount,
    ) -> StakeAccountBalances;

    /// invoked when the staking pool is brought back online and staking is resumed
    /// - the callback ensures that the retaking succeeded
    ///
    /// `#[private]`
    fn ops_stake_start_finalize(&mut self);

    /// invoked when the staking pool is taken offline and all NEAR is unstaked
    /// - the callback ensures that the unstaking succeeded
    ///
    /// `#[private]`
    fn ops_stake_stop_finalize(&mut self);
}

pub const ERR_STAKE_ACTION_FAILED: ErrCode = ErrCode("STAKE_ACTION_FAILED");

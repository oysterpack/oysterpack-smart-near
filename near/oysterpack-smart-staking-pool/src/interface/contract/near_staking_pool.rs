use oysterpack_smart_near::domain::YoctoNear;
use oysterpack_smart_near::near_sdk::AccountId;
use oysterpack_smart_near::near_sdk::{
    json_types::ValidAccountId,
    serde::{Deserialize, Serialize},
};

/// NEAR Staking Pool interface adapter
/// - https://github.com/near/core-contracts/tree/master/staking-pool
pub trait NearStakingPool {
    /// Returns the account's staked NEAR balance
    /// - If the account is not registered then zero is returned.
    fn get_account_staked_balance(&self, account_id: ValidAccountId) -> YoctoNear;

    /// Returns total unstaked plus account storage available balance
    /// - If the account is not registered then zero is returned.
    fn get_account_unstaked_balance(&self, account_id: ValidAccountId) -> YoctoNear;

    /// If account is not registered or has zero unstaked, then true is returned
    fn is_account_unstaked_balance_available(&self, account_id: ValidAccountId) -> bool;

    fn get_account_total_balance(&self, account_id: ValidAccountId) -> YoctoNear;

    fn get_account(&self, account_id: ValidAccountId) -> NearStakingPoolAccount;

    fn deposit(&mut self);

    fn deposit_and_stake(&mut self);

    fn withdraw(&mut self, amount: YoctoNear);

    fn withdraw_all(&mut self);

    /// restakes unstaked balance and includes account storage available balance
    fn stake(&mut self, amount: YoctoNear);

    fn unstake(&mut self, amount: YoctoNear);

    fn unstake_all(&mut self);
}

/// Represents an account structure readable by humans.
#[derive(Serialize, Deserialize)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub struct NearStakingPoolAccount {
    pub account_id: AccountId,
    /// The unstaked balance that can be withdrawn or staked.
    pub unstaked_balance: YoctoNear,
    /// The amount balance staked at the current "stake" share price.
    pub staked_balance: YoctoNear,
    /// Whether the unstaked balance is available for withdrawal now.
    pub can_withdraw: bool,
}

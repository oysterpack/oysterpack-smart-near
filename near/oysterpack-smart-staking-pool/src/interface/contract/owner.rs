use crate::StakeAccountBalances;
use oysterpack_smart_near::domain::YoctoNear;

pub trait StakingPoolOwner {
    /// stakes the owner's available balance
    /// - if amount is None, then the owner's entire balance is staked
    ///
    /// ## Panics
    /// - if predecessor account is not the owner
    /// - if specified amount is more than the owner's available balance
    fn ops_stake_owner_balance(&mut self, amount: Option<YoctoNear>) -> StakeAccountBalances;
}

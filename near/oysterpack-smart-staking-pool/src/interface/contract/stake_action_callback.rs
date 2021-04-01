use oysterpack_smart_near::domain::YoctoNear;
use oysterpack_smart_near::ErrCode;

pub trait StakeActionCallback {
    /// Used to confirm that the stake action was successful.
    ///
    /// If the stake action failed, then the contract will fully unstake.
    ///
    /// `#[private]`
    fn ops_stake_callback(&mut self, staked_balance: YoctoNear);
}

pub const ERR_STAKE_ACTION_FAILED: ErrCode = ErrCode("STAKE_ACTION_FAILED");

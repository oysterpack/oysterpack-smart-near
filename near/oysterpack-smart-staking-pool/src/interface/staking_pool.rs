use oysterpack_smart_near::domain::YoctoNear;
use oysterpack_smart_near::{Err, ErrCode, ErrorConst};

pub trait StakingPool {
    /// Deposits the attached amount into the predecessor's account
    ///
    /// If the account is not registered, then the account will be automatically registered using
    /// a portion of the deposit to pay for account storage.
    ///
    /// `#[payable]`
    fn deposit(&mut self);

    /// Deposits the attached amount into the predecessor's account and stakes it.
    ///
    /// If the account is not registered, then the account will be automatically registered using
    /// a portion of the deposit to pay for account storage.
    ///
    /// `#[payable]`
    fn deposit_and_stake(&mut self);

    /// Withdraws the entire unstaked balance from the predecessor account.
    /// It's only allowed if the `unstake` action was not performed in the four most recent epochs.
    ///
    /// Returns the amount that was transferred or an error explaining why the withdraw failed:
    /// - ERR_ACCOUNT_NOT_REGISTERED
    /// - ERR_ZERO_UNSTAKED_BALANCE
    /// - ERR_UNSTAKED_NEAR_LOCKED
    fn withdraw_all(&mut self) -> Result<YoctoNear, Err>;

    /// Withdraws the specified unstaked balance from the predecessor account.
    /// It's only allowed if the `unstake` action was not performed in the four most recent epochs.
    ///
    /// If the withdrawal fails, then an error is returned
    /// - ERR_ACCOUNT_NOT_REGISTERED
    /// - ERR_ZERO_UNSTAKED_BALANCE
    /// - ERR_UNSTAKED_NEAR_LOCKED
    /// - ERR_INSUFFICIENT_FUNDS
    fn withdraw(&mut self, amount: YoctoNear) -> Option<Err>;
}

pub const ERR_ZERO_UNSTAKED_BALANCE: ErrorConst = ErrorConst(ErrCode("ZERO_UNSTAKED_BALANCE"), "");

pub const ERR_UNSTAKED_NEAR_LOCKED: ErrCode = ErrCode("UNSTAKED_NEAR_LOCKED");

pub const ERR_INSUFFICIENT_FUNDS: ErrorConst = ErrorConst(ErrCode("INSUFFICIENT_FUNDS"), "");

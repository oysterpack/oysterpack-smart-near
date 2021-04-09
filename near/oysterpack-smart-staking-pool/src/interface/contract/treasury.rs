use oysterpack_smart_near::domain::YoctoNear;
use oysterpack_smart_near::{Level, LogEvent};

/// # **Contract Interface**: Staking Pool Treasury API
pub trait Treasury {
    /// Deposits any attached deposit into the treasury.
    /// This will effectively stake the deposit and mint STAKE for the treasury.
    ///
    /// This enables external sources of revenue to be deposited into the treasury.
    ///
    /// `#[payable]`
    fn ops_stake_treasury_deposit(&mut self);

    /// transfers the specified amount from the treasury to the contract owners account
    /// - if no amount is specified, then the total treasury balance is transferred to the owner's account
    ///
    /// ## Panics
    /// - requires [`PERMISSION_TREASURER`] permission
    /// - if there are insufficient funds   
    fn ops_stake_treasury_transfer_to_owner(&mut self, amount: Option<YoctoNear>);
}

pub const PERMISSION_TREASURER: &str = "treasurer";

pub const LOG_EVENT_TREASURY_DEPOSIT: LogEvent = LogEvent(Level::INFO, "TREASURY_DEPOSIT");

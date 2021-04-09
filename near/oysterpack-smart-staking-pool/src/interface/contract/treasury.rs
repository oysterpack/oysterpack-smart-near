/// # **Contract Interface**: Staking Pool Treasury API
pub trait Treasury {
    /// Deposits any attached deposit into the treasury.
    ///
    /// `#[payable]`
    fn ops_stake_treasury_deposit(&mut self);
}

pub const PERMISSION_TREASURER: &str = "treasurer";

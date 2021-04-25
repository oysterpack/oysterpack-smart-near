use crate::StakeAccountBalances;
use oysterpack_smart_near::domain::YoctoNear;
use oysterpack_smart_near::near_sdk::json_types::ValidAccountId;
use oysterpack_smart_near::near_sdk::PromiseOrValue;

/// # **Contract Interface**: Staking Pool Treasury API
pub trait Treasury {
    /// Deposits any attached deposit into the treasury.
    /// This will effectively stake the deposit and mint STAKE for the treasury.
    ///
    /// This enables external sources of revenue to be deposited into the treasury.
    ///
    /// ## Notes
    /// - The entire deposit is staked. When minting STAKE, the conversion from NEAR -> STAKE is rounded
    ///   down. Thus, the NEAR deposit remainder will also get staked, effectively distributing the
    ///   funds to the current stakers.
    ///
    /// ## Panics
    /// if no deposit is attached
    ///
    /// `#[payable]`
    fn ops_stake_treasury_deposit(&mut self) -> PromiseOrValue<StakeAccountBalances>;

    /// Deposits and stakes any attached deposit, which effectively distributes the funds to all current
    /// STAKE owners. As a side effect this will also boost the dividend yield.
    ///
    /// This enables external sources of revenue to be distributed to STAKE owners.
    ///
    /// ## Notes
    /// - An alternative way to make a distribution is to transfer STAKE to the treasury account
    ///   directly, i.e., via the [`oysterpack_smart_fungible_token::FungibleToken::ft_transfer`] API
    /// - if no deposit is made, then any collected earnings will simply be staked
    ///
    /// `#[payable]`
    fn ops_stake_treasury_distribution(&mut self);

    /// Transfers the specified amount from the treasury to the contract owners account
    /// - if no amount is specified, then the total treasury balance is transferred to the owner's account
    ///
    /// ## Notes
    /// - dividend is paid out before transfer
    ///
    /// ## Panics
    /// - requires [`PERMISSION_TREASURER`] permission or the owner
    /// - if there are insufficient funds   
    fn ops_stake_treasury_transfer_to_owner(&mut self, amount: Option<YoctoNear>);

    /// grants treasurer permission to specified account
    ///
    /// ## Panics
    /// - if not invoked by admin
    /// - if specified account is not registered
    fn ops_stake_grant_treasurer(&mut self, account_id: ValidAccountId);

    /// revokes treasurer permission to specified account
    ///
    /// ## Panics
    /// - if not invoked by admin
    /// - if specified account is not registered
    fn ops_stake_revoke_treasurer(&mut self, account_id: ValidAccountId);

    /// revokes treasurer permission to specified account
    fn ops_stake_is_treasurer(&mut self, account_id: ValidAccountId) -> bool;
}

pub const PERMISSION_TREASURER: &str = "treasurer";

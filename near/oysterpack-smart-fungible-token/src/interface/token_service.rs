use crate::TokenAmount;
use oysterpack_smart_near::{Level, LogEvent};

/// Provides basic functions to operate the fungible token.
pub trait TokenService {
    /// Mints new tokens and credits them to the specified account ID, which increases the total supply.
    /// - logs [`LOG_EVENT_FT_MINT`]
    ///
    /// **Use Case:** STAKE tokens are minted when NEAR is staked.
    ///
    /// ## Panics
    /// - if the account is not registered
    /// - if amount is zero
    fn ft_mint(&mut self, account_id: &str, amount: TokenAmount);

    /// Debits tokens from the specified account ID and burns them, which decreases the total supply.
    /// - logs [`LOG_EVENT_FT_BURN`]
    ///
    /// **Use Case:** STAKE tokens are burned when they are unstaked and converted back to NEAR.
    ///
    /// ## Notes
    /// - burns locked tokens first
    ///
    /// ## Panics
    /// - if the account is not registered
    /// - if amount is zero
    fn ft_burn(&mut self, account_id: &str, amount: TokenAmount);

    /// Attempts to burn the account's total token balance.
    /// - logs [`LOG_EVENT_FT_BURN`]
    fn ft_burn_all(&mut self, account_id: &str);

    /// Locks the specified number of tokens on the specified account.
    /// - logs [`LOG_EVENT_FT_LOCK`]
    ///
    /// The account owns the tokens but they cannot be transferred while locked.
    ///
    /// ## Panics
    /// - if the account is not registered
    /// - if amount is zero
    fn ft_lock(&mut self, account_id: &str, amount: TokenAmount);

    /// Unlocks the specified amount of tokens and makes them available for transfers
    /// - logs [`LOG_EVENT_FT_UNLOCK`]
    ///
    ///
    /// ## Panics
    /// - if the account is not registered
    /// - if amount is zero
    fn ft_unlock(&mut self, account_id: &str, amount: TokenAmount);

    /// Attempts to unlock the account's total locked token balance.
    /// - logs [`LOG_EVENT_FT_UNLOCK`]
    ///
    /// ## Panics
    /// - if the account is not registered
    fn ft_unlock_all(&mut self, account_id: &str);

    /// Returns the accounts locked balance or None if the account is not registered.
    fn ft_locked_balance(&mut self, account_id: &str) -> Option<TokenAmount>;
}

pub const LOG_EVENT_FT_MINT: LogEvent = LogEvent(Level::INFO, "FT_MINT");
pub const LOG_EVENT_FT_BURN: LogEvent = LogEvent(Level::INFO, "FT_BURN");

pub const LOG_EVENT_FT_LOCK: LogEvent = LogEvent(Level::INFO, "FT_LOCK");
pub const LOG_EVENT_FT_UNLOCK: LogEvent = LogEvent(Level::INFO, "FT_UNLOCK");

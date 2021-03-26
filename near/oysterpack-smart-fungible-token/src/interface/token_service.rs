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
    /// ## Panics
    /// - if the account is not registered
    /// - if amount is zero
    fn ft_burn(&mut self, account_id: &str, amount: TokenAmount);

    /// Attempts to burn the account's total token balance.
    /// - logs [`LOG_EVENT_FT_BURN`]
    fn ft_burn_all(&mut self, account_id: &str);
}

pub const LOG_EVENT_FT_MINT: LogEvent = LogEvent(Level::INFO, "FT_MINT");

pub const LOG_EVENT_FT_BURN: LogEvent = LogEvent(Level::INFO, "FT_BURN");

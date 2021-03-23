use crate::TokenAmount;
use near_sdk::json_types::ValidAccountId;

/// Provides basic functions to operate the fungible token.
pub trait TokenService {
    /// Mints new tokens and credits them to the specified account ID, which increases the total supply.
    ///
    /// **Use Case:** STAKE tokens are minted when NEAR is staked.
    ///
    /// ## Panics
    /// - if the account is not registered
    /// - if amount is zero
    fn ft_mint(&mut self, account_id: ValidAccountId, amount: TokenAmount);

    /// Debits tokens from the specified account ID and burns them, which decreases the total supply.
    ///
    /// **Use Case:** STAKE tokens are burned when they are unstaked and converted back to NEAR.
    ///
    /// ## Panics
    /// - if the account is not registered
    /// - if amount is zero
    fn ft_burn(&mut self, account_id: ValidAccountId, amount: TokenAmount);
}

use crate::*;
use near_sdk::json_types::ValidAccountId;
use near_sdk::PromiseOrValue;

/// Receiver of the Fungible Token for [`FungibleToken::ft_transfer_call`] calls.
pub trait TransferReceiver {
    /// Callback to receive tokens.
    ///
    /// Called by fungible token contract `env::predecessor_account_id` after `transfer_call` was initiated by
    /// `sender_id` of the given `amount` with the transfer message given in `msg` field.
    /// The `amount` of tokens were already transferred to this contract account and ready to be used.
    ///
    /// The method must return the amount of tokens that are not used/accepted by this contract from
    /// the transferred amount, e.g.:
    /// - The transferred amount was `500`, the contract completely takes it and must return `0`.
    /// - The transferred amount was `500`, but this transfer call only needs `450` for the action passed in the `msg`
    ///   field, then the method must return `50`.
    /// - The transferred amount was `500`, but the action in `msg` field has expired and the transfer must be
    ///   cancelled. The method must return `500` or panic.
    ///
    /// Arguments:
    /// - `sender_id` - the account ID that initiated the transfer.
    /// - `amount` - the amount of tokens that were transferred to this account.
    /// - `msg` - a string message that was passed with this transfer call.
    ///
    /// Returns the amount of tokens that are used/accepted by this contract from the transferred amount.
    fn ft_on_transfer(
        &mut self,
        sender_id: ValidAccountId,
        amount: TokenAmount,
        msg: TransferCallMessage,
    ) -> PromiseOrValue<TokenAmount>;
}

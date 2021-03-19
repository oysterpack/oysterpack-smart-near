use crate::TokenAmount;
use near_sdk::json_types::ValidAccountId;
use near_sdk::PromiseOrValue;

pub struct FungibleTokenComponent {}

/// Callback on fungible token contract to resolve transfer.
pub trait ResolveTransferCall {
    /// Callback to resolve transfer.
    /// Private method (`env::predecessor_account_id == env::current_account_id`).
    ///
    /// Called after the receiver handles the transfer call and returns unused token amount.
    ///
    /// This method must get `unused_amount` from the receiver's promise result and refund the
    /// `unused_amount` from the receiver's account back to the `sender_id` account.
    ///
    /// Arguments:
    /// - `sender_id` - the account ID that initiated the transfer.
    /// - `receiver_id` - the account ID of the receiver contract.
    /// - `amount` - the amount of tokens that were transferred to receiver's account.
    ///
    /// Promise result data dependency (`unused_amount`):
    /// - the amount of tokens that were unused by receiver's contract.
    /// - Received from `on_ft_receive`
    /// - `unused_amount` must be `U128` in range from `0` to `amount`. All other invalid values
    ///   are considered to be equal to be the total transfer amount.
    ///
    /// Returns amount that was refunded back to the sender.
    ///
    /// The callback should be designed to never panic.
    /// - if the `sender_id` is not registered, then refunded STAKE tokens will be burned
    /// - if the `receiver_id` is not registered, then the contract should be handle it
    ///
    /// #\[private\]
    fn ft_resolve_transfer_call(
        &mut self,
        sender_id: ValidAccountId,
        receiver_id: ValidAccountId,
        amount: TokenAmount,
        // NOTE: #[callback_result] is not supported yet and has to be handled using lower level interface.
        //
        // #[callback_result]
        // unused_amount: CallbackResult<TokenAmount>,
    ) -> PromiseOrValue<TokenAmount>;
}

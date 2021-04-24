use crate::*;
use oysterpack_smart_fungible_token::{
    FungibleToken, FungibleTokenMetadataProvider, FungibleTokenOperator, Memo, Metadata,
    OperatorCommand, ResolveTransferCall, TokenAmount, TransferCallMessage,
};
use oysterpack_smart_near::domain::Gas;
use oysterpack_smart_near::near_sdk::Promise;

#[near_bindgen]
impl FungibleToken for Contract {
    #[payable]
    fn ft_transfer(
        &mut self,
        receiver_id: ValidAccountId,
        amount: TokenAmount,
        memo: Option<Memo>,
    ) {
        Self::ft_stake().ft_transfer(receiver_id, amount, memo)
    }

    #[payable]
    fn ft_transfer_call(
        &mut self,
        receiver_id: ValidAccountId,
        amount: TokenAmount,
        memo: Option<Memo>,
        msg: TransferCallMessage,
    ) -> Promise {
        Self::ft_stake().ft_transfer_call(receiver_id, amount, memo, msg)
    }

    fn ft_total_supply(&self) -> TokenAmount {
        Self::ft_stake().ft_total_supply()
    }

    fn ft_balance_of(&self, account_id: ValidAccountId) -> TokenAmount {
        Self::ft_stake().ft_balance_of(account_id)
    }
}

#[near_bindgen]
impl ResolveTransferCall for Contract {
    #[private]
    fn ft_resolve_transfer_call(
        &mut self,
        sender_id: ValidAccountId,
        receiver_id: ValidAccountId,
        amount: TokenAmount,
    ) -> TokenAmount {
        Self::ft_stake().ft_resolve_transfer_call(sender_id, receiver_id, amount)
    }
}

#[near_bindgen]
impl FungibleTokenMetadataProvider for Contract {
    fn ft_metadata(&self) -> Metadata {
        Self::ft_stake().ft_metadata()
    }
}

#[near_bindgen]
impl FungibleTokenOperator for Contract {
    fn ft_operator_command(&mut self, command: OperatorCommand) {
        Self::ft_stake().ft_operator_command(command)
    }

    fn ft_operator_transfer_callback_gas(&self) -> Gas {
        Self::ft_stake().ft_operator_transfer_callback_gas()
    }
}

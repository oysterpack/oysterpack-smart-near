use crate::{Icon, Reference};
use oysterpack_smart_near::domain::{Gas, TGas};
use oysterpack_smart_near::near_sdk::serde::{Deserialize, Serialize};
use oysterpack_smart_near::Hash;

/// # **Contract Interface**: Fungible Token Operator API
pub trait FungibleTokenOperator {
    /// Executes the specified operator command
    ///
    /// ## Panics
    /// - if predecessor account is not registered
    /// - if predecessor account is not authorized - requires operator permission
    fn ft_operator_command(&mut self, command: OperatorCommand);

    /// returns the amount of gas configured for the resolve transfer callback
    fn ft_operator_transfer_callback_gas(&self) -> Gas;
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub enum OperatorCommand {
    SetIcon(Icon),
    ClearIcon,
    SetReference(Reference, Hash),
    ClearReference,
    SetTransferCallbackGas(TGas),
}

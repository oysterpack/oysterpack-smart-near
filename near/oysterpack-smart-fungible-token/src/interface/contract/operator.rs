use crate::{Icon, Reference};
use near_sdk::serde::{Deserialize, Serialize};
use oysterpack_smart_near::Hash;

/// # **Contract Interface**: Fungible Token Operator API
pub trait FungibleTokenOperator {
    /// Updates the icon [data URL][1]
    ///
    /// ## Panics
    /// - if predecessor account is not registered
    /// - if predecessor account is not authorized - requires operator permission
    ///
    /// [1]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Basics_of_HTTP/Data_URIs
    fn ft_operator_command(&mut self, command: OperatorCommand);
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(crate = "near_sdk::serde")]
pub enum OperatorCommand {
    SetIcon(Icon),
    ClearIcon,
    SetReference(Reference, Hash),
    ClearReference,
}

use crate::{Icon, Reference};
use oysterpack_smart_near::{ErrCode, Hash};

/// # **Contract Interface**: Fungible Token Operator API
pub trait Operator {
    /// Updates the icon [data URL][1]
    ///
    /// ## Panics
    /// - if not authorized - requires operator permission
    /// - if icon is not a valid data URL
    ///
    /// [1]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Basics_of_HTTP/Data_URIs
    fn ft_set_icon(&mut self, icon: Icon);

    /// Clears the icon
    ///
    /// ## Panics
    /// - if not authorized - requires operator permission
    fn ft_clear_icon(&mut self);

    /// Updates the reference metadata
    ///
    /// ## Panics
    /// - if not authorized - requires operator permission
    /// - if reference is not a valid URL
    ///
    /// [1]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Basics_of_HTTP/Data_URIs
    fn ft_set_reference(&mut self, reference: Reference, hash: Hash);

    /// Clears the `reference` and `reference_hash` metadata fields
    ///
    /// ## Panics
    /// - if not authorized - requires operator permission
    fn ft_clear_reference(&mut self);
}

pub const ERR_INVALID_ICON_DATA_URL: ErrCode = ErrCode("INVALID_ICON_DATA_URL");

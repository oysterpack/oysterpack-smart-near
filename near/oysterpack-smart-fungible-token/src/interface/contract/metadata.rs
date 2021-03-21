use crate::Metadata;

pub const FT_METADATA_SPEC: &'static str = "ft-1.0.0";

/// # **Contract Interface**: [Fungible Token Metadata API][1]
///
/// [1]: https://nomicon.io/Standards/Tokens/FungibleTokenMetadata.html
pub trait FungibleTokenMetadataProvider {
    fn ft_metadata(&self) -> Metadata;
}

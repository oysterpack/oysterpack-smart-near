use crate::Metadata;

pub const FT_METADATA_SPEC: &'static str = "ft-1.0.0";

pub trait FungibleTokenMetadataProvider {
    fn ft_metadata(&self) -> Metadata;
}

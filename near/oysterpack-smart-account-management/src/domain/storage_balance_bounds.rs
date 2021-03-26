use crate::StorageUsageBounds;
use oysterpack_smart_near::{
    domain::YoctoNear,
    near_sdk::{
        borsh::{self, BorshDeserialize, BorshSerialize},
        env,
        serde::{Deserialize, Serialize},
    },
};

/// Defines storage balance bounds for the contract
#[derive(
    BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, PartialEq, Clone, Copy,
)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub struct StorageBalanceBounds {
    /// the minimum balance that must be maintained for storage by the account on the contract
    /// - it is the amount of tokens required to start using this contract at all, e.g., to register with the contract
    pub min: YoctoNear,
    /// the maximum storage balance that is permitted
    ///
    /// A contract may implement `max` equal to `min` if it only charges for initial registration,
    /// and does not adjust per-user storage over time. A contract which implements `max` must
    /// refund deposits that would increase a user's storage balance beyond this amount.
    pub max: Option<YoctoNear>,
}

impl From<StorageUsageBounds> for StorageBalanceBounds {
    fn from(bounds: StorageUsageBounds) -> Self {
        let storage_byte_cost = env::storage_byte_cost();
        Self {
            min: (storage_byte_cost * bounds.min.value() as u128).into(),
            max: bounds
                .max
                .map(|max| (storage_byte_cost * max.value() as u128).into()),
        }
    }
}

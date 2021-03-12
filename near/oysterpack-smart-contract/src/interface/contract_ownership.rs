use crate::ContractOwner;
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env,
    json_types::ValidAccountId,
    serde::{Deserialize, Serialize},
};
use oysterpack_smart_near::asserts::assert_yocto_near_attached;
use oysterpack_smart_near::domain::{AccountIdHash, YoctoNear};
use oysterpack_smart_near::{ErrCode, ErrorConst};

/// Contract ownership is kept private. The contract owner ID sha256 hash is stored instead of the
/// account ID plain text.
/// - only the owner itself can verify that he is the contract owner
pub trait ContractOwnership {
    /// checks if the predecessor account ID is the current contract owner
    fn is_owner() -> bool {
        is_owner()
    }

    /// ## Panics
    /// - if the predecessor account is not the owner account
    /// - new owner account must be registered
    /// - if 1 yoctoNEAR is not attached
    ///
    /// `#[payable]` - requires exactly 1 yoctoNEAR to be attached
    fn transfer_ownership(&mut self, new_owner: ValidAccountId) {
        assert_yocto_near_attached();
        assert_owner_access();
        ContractOwner::update(new_owner);
    }

    /// Used by the contract owner to withdraw from the contract owner's available balance.
    ///
    /// If `amount` is None, then all available balance is withdrawn.
    ///
    /// Returns the updated contract owner NEAR balance.
    ///
    /// ## Panics
    /// - if the predecessor account is not the owner account
    /// - if 1 yoctoNEAR is not attached
    /// - if there are insufficient funds to fulfill the request
    ///
    /// `#[payable]` - requires exactly 1 yoctoNEAR to be attached
    fn withdraw_owner_balance(&mut self, amount: Option<YoctoNear>) -> ContractOwnerNearBalance;

    fn owner_balance(&self) -> ContractOwnerNearBalance;
}

#[derive(
    BorshSerialize, BorshDeserialize, Deserialize, Serialize, Clone, Debug, PartialEq, Default,
)]
#[serde(crate = "near_sdk::serde")]
pub struct ContractOwnerNearBalance {
    pub total: YoctoNear,
    pub available: YoctoNear,
}

pub const ERR_OWNER_ACCESS_REQUIRED: ErrorConst = ErrorConst(
    ErrCode("OWNER_ACCESS_REQUIRED"),
    "action requires owner access",
);

/// asserts that the predecessor account ID is the owner
pub fn assert_owner_access() -> ContractOwner {
    let owner = ContractOwner::load();
    ERR_OWNER_ACCESS_REQUIRED
        .assert(|| owner.account_id_hash() == AccountIdHash::from(env::predecessor_account_id()));
    owner
}

/// checks if the predecessor account ID is the current contract owner
pub fn is_owner() -> bool {
    let owner = ContractOwner::load();
    owner.account_id_hash() == AccountIdHash::from(env::predecessor_account_id())
}

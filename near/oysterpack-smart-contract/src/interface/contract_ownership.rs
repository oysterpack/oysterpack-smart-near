use crate::ContractOwner;
use near_sdk::{env, json_types::ValidAccountId};
use oysterpack_smart_near::domain::YoctoNear;
use oysterpack_smart_near::{ErrCode, ErrorConst, Hash};

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
    fn transfer_ownership(&mut self, new_owner: ValidAccountId) {
        let mut owner = ContractOwner::load();
        ERR_OWNER_ACCESS_REQUIRED.assert(|| {
            owner.account_id_hash() == Hash::from(env::predecessor_account_id().as_str())
        });
        ContractOwner::update(new_owner);
    }
}

pub const ERR_OWNER_ACCESS_REQUIRED: ErrorConst = ErrorConst(
    ErrCode("OWNER_ACCESS_REQUIRED"),
    "action requires owner access",
);

/// asserts that the predecessor account ID is the owner
pub fn assert_owner_access() {
    ERR_OWNER_ACCESS_REQUIRED.assert(|| is_owner())
}

/// checks if the predecessor account ID is the current contract owner
pub fn is_owner() -> bool {
    let owner = ContractOwner::load();
    owner.account_id_hash() == Hash::from(env::predecessor_account_id().as_str())
}

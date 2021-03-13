use crate::{
    ERR_CURRENT_OR_PROSPECTIVE_OWNER_ACCESS_REQUIRED, ERR_OWNER_ACCESS_REQUIRED,
    ERR_PROSPECTIVE_OWNER_ACCESS_REQUIRED,
};
use near_sdk::json_types::ValidAccountId;
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env,
};
use oysterpack_smart_near::domain::AccountIdHash;
use oysterpack_smart_near::{data::Object, ErrCode, ErrorConst};

/// Indicates that an attempt was made to initialize the contract with a different owner.
///
/// A contract can only be initialized, i.e., seeded, with the contract owner once after the contract
/// is deployed.
/// - see [`ContractOwner::initialize_contract`]
pub const ERR_CONTRACT_OWNER_ALREADY_INITIALIZED: ErrorConst = ErrorConst(
    ErrCode("CONTRACT_OWNER_ALREADY_INITIALIZED"),
    "contract owner is already initialized with a different owner",
);

const CONTRACT_OWNER_KEY: u128 = 1952995667402400813184690843862547707;

type DAO = Object<u128, ContractOwner>;

/// Every contract has an owner
#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq)]
pub struct ContractOwner(AccountIdHash, Option<AccountIdHash>);

impl ContractOwner {
    pub fn new(account_id: ValidAccountId) -> Self {
        Self(account_id.into(), None)
    }

    /// Used to initialize the contract with the specified owner.
    ///
    /// ## Panics
    /// if the contract owner has already been initialized with a different owner
    pub fn initialize_contract(account_id: ValidAccountId) {
        let owner = DAO::new(CONTRACT_OWNER_KEY, ContractOwner::new(account_id));
        match DAO::load(&CONTRACT_OWNER_KEY) {
            Some(current_owner) => {
                ERR_CONTRACT_OWNER_ALREADY_INITIALIZED.assert(|| current_owner == owner)
            }
            None => owner.save(),
        }
    }

    pub fn load() -> Self {
        *DAO::load(&CONTRACT_OWNER_KEY).unwrap()
    }

    pub fn account_id_hash(&self) -> AccountIdHash {
        self.0
    }

    pub fn prospective_owner_account_id_hash(&self) -> Option<AccountIdHash> {
        self.1
    }

    pub fn clear_prospective_owner(&mut self) {
        self.1 = None;
        DAO::new(CONTRACT_OWNER_KEY, *self).save();
    }

    pub(crate) fn update(new_owner: ValidAccountId) {
        let new_owner = DAO::new(CONTRACT_OWNER_KEY, ContractOwner::new(new_owner));
        new_owner.save();
    }

    /// asserts that the predecessor account ID is the owner
    pub fn assert_owner_access() -> ContractOwner {
        let owner = ContractOwner::load();
        ERR_OWNER_ACCESS_REQUIRED.assert(|| {
            owner.account_id_hash() == AccountIdHash::from(env::predecessor_account_id())
        });
        owner
    }

    /// asserts that the predecessor account ID is the prospective owner
    ///
    /// ## Panics
    /// if there is no contract ownership transfer in progress
    pub fn assert_prospective_owner_access() -> ContractOwner {
        let owner = ContractOwner::load();
        ERR_PROSPECTIVE_OWNER_ACCESS_REQUIRED.assert(|| {
            owner
                .prospective_owner_account_id_hash()
                .map_or(false, |account_id_hash| {
                    account_id_hash == AccountIdHash::from(env::predecessor_account_id())
                })
        });
        owner
    }

    /// asserts that the predecessor account ID is the current or prospective owner
    pub fn assert_current_or_prospective_owner_access() -> ContractOwner {
        let owner = ContractOwner::load();
        ERR_CURRENT_OR_PROSPECTIVE_OWNER_ACCESS_REQUIRED.assert(|| {
            owner.account_id_hash() == AccountIdHash::from(env::predecessor_account_id())
                || owner
                    .prospective_owner_account_id_hash()
                    .map_or(false, |account_id_hash| {
                        account_id_hash == AccountIdHash::from(env::predecessor_account_id())
                    })
        });
        owner
    }
}

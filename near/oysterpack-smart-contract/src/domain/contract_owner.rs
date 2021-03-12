use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::ValidAccountId;
use oysterpack_smart_near::domain::AccountIdHash;
use oysterpack_smart_near::{data::Object, ErrCode, ErrorConst};

pub const ERR_CONTRACT_OWNER_ALREADY_INITIALIZED: ErrorConst = ErrorConst(
    ErrCode("CONTRACT_OWNER_ALREADY_INITIALIZED"),
    "contract owner is already initialized with a different owner",
);

const CONTRACT_OWNER_KEY: u128 = 1952995667402400813184690843862547707;

type DAO = Object<u128, ContractOwner>;

#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq)]
pub struct ContractOwner(AccountIdHash);

impl ContractOwner {
    pub fn new(account_id: ValidAccountId) -> Self {
        Self(account_id.into())
    }

    /// Used to initialize the contract with the specified owner.
    ///
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

    pub(crate) fn update(new_owner: ValidAccountId) {
        let new_owner = DAO::new(CONTRACT_OWNER_KEY, ContractOwner::new(new_owner));
        new_owner.save();
    }
}

use crate::{
    ContractBid, ERR_CURRENT_OR_PROSPECTIVE_OWNER_ACCESS_REQUIRED, ERR_OWNER_ACCESS_REQUIRED,
    ERR_PROSPECTIVE_OWNER_ACCESS_REQUIRED,
};
use near_sdk::json_types::ValidAccountId;
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env,
};
use oysterpack_smart_near::domain::{AccountIdHash, YoctoNear};
use oysterpack_smart_near::{data::Object, ErrCode, ErrorConst};
use std::ops::{Deref, DerefMut};

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

pub struct ContractOwnerObject(DAO);

impl ContractOwnerObject {
    pub fn load() -> Self {
        Self(DAO::load(&CONTRACT_OWNER_KEY).unwrap())
    }

    /// Used to initialize the contract with the specified owner.
    ///
    /// ## Panics
    /// if the contract owner has already been initialized with a different owner
    pub fn initialize_contract(account_id: ValidAccountId) {
        let owner = DAO::new(CONTRACT_OWNER_KEY, ContractOwner::new(account_id.clone()));
        match DAO::load(&CONTRACT_OWNER_KEY) {
            Some(current_owner) => {
                ERR_CONTRACT_OWNER_ALREADY_INITIALIZED.assert(|| current_owner == owner)
            }
            None => {
                owner.save();
                let account_ids = ContractOwnershipAccountIds {
                    owner: account_id.as_ref().as_bytes().to_vec(),
                    prospective_owner: None,
                    buyer: None,
                };
                Object::new(CONTRACT_ACCOUNT_IDS_KEY, account_ids).save();
            }
        }
    }

    /// asserts that the predecessor account ID is the owner
    pub fn assert_owner_access() -> Self {
        let owner = Self::load();
        ERR_OWNER_ACCESS_REQUIRED.assert(|| {
            owner.account_id_hash() == AccountIdHash::from(env::predecessor_account_id())
        });
        owner
    }

    /// asserts that the predecessor account ID is the prospective owner
    ///
    /// ## Panics
    /// if there is no contract ownership transfer in progress
    pub fn assert_prospective_owner_access() -> Self {
        let owner = Self::load();
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
    pub fn assert_current_or_prospective_owner_access() -> Self {
        let owner = Self::load();
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

    pub(crate) fn set_owner(new_owner: ValidAccountId) {
        let new_owner = DAO::new(CONTRACT_OWNER_KEY, ContractOwner::new(new_owner));
        new_owner.save();
    }
}

impl Deref for ContractOwnerObject {
    type Target = DAO;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ContractOwnerObject {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Every contract has an owner
#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq)]
pub struct ContractOwner {
    pub(crate) account_id_hash: AccountIdHash,

    pub(crate) prospective_owner_account_id_hash: Option<AccountIdHash>,

    pub(crate) sale_price: Option<YoctoNear>,
    pub(crate) bid: Option<(AccountIdHash, ContractBid)>,
}

impl ContractOwner {
    pub fn new(account_id: ValidAccountId) -> Self {
        Self {
            account_id_hash: account_id.into(),
            prospective_owner_account_id_hash: None,
            sale_price: None,
            bid: None,
        }
    }

    pub fn account_id_hash(&self) -> AccountIdHash {
        self.account_id_hash
    }

    pub fn prospective_owner_account_id_hash(&self) -> Option<AccountIdHash> {
        self.prospective_owner_account_id_hash
    }

    /// if true, then it means that the transfer is awaiting finalization from the prospective owner
    pub fn transfer_initiated(&self) -> bool {
        self.prospective_owner_account_id_hash.is_some()
    }

    pub fn contract_sale_price(&self) -> Option<YoctoNear> {
        self.sale_price.as_ref().cloned()
    }

    pub fn bid(&self) -> Option<(AccountIdHash, ContractBid)> {
        self.bid.as_ref().cloned()
    }
}

const CONTRACT_ACCOUNT_IDS_KEY: u128 = 1953243214138465698448969404106238471;

type ContractOwnershipAccountIdsDAO = Object<u128, ContractOwnershipAccountIds>;

pub(crate) struct ContractOwnershipAccountIdsObject(ContractOwnershipAccountIdsDAO);

impl ContractOwnershipAccountIdsObject {
    pub fn load() -> Self {
        Self(ContractOwnershipAccountIdsDAO::load(&CONTRACT_ACCOUNT_IDS_KEY).unwrap())
    }
}

/// The contract ownership account IDs are being stored separately to avoid heap allocations when
/// using [`ContractOwner`], which enables it to be used by value vs by ref.
///
/// We need the account IDs to be able to transfer NEAR funds to the accounts
#[derive(BorshSerialize, BorshDeserialize, Clone, PartialEq, Debug)]
pub(crate) struct ContractOwnershipAccountIds {
    pub owner: Vec<u8>,
    pub prospective_owner: Option<Vec<u8>>,
    pub buyer: Option<Vec<u8>>,
}

impl ContractOwnershipAccountIds {
    pub fn owner(&self) -> String {
        String::try_from_slice(&self.owner).unwrap()
    }

    pub fn set_owner(&mut self, account_id: &str) {
        self.owner = account_id.as_bytes().to_vec();
    }

    pub fn prospective_owner(&self) -> Option<String> {
        self.prospective_owner
            .as_ref()
            .map(|account_id| String::try_from_slice(&account_id).unwrap())
    }

    pub fn set_prospective_owner(&mut self, account_id: &str) {
        self.prospective_owner = Some(account_id.as_bytes().to_vec());
    }

    pub fn buyer(&self) -> Option<String> {
        self.buyer
            .as_ref()
            .map(|account_id| String::try_from_slice(&account_id).unwrap())
    }

    pub fn set_buyer(&mut self, account_id: &str) {
        self.buyer = Some(account_id.as_bytes().to_vec());
    }
}

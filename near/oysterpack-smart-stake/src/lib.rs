mod account_metrics;
mod account_storage_usage;
mod components;
mod contract_ownership;
mod storage_management;

use components::*;
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env,
    json_types::ValidAccountId,
    near_bindgen, PanicOnDefault,
};
use oysterpack_smart_account_management::StorageUsageBounds;
use oysterpack_smart_contract::components::contract_ownership::ContractOwnershipComponent;
use oysterpack_smart_near::component::Deploy;
use std::convert::TryInto;

near_sdk::setup_alloc!();

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract;

#[near_bindgen]
impl Contract {
    /// Default config values:
    /// - contract owner = predecessor Account ID
    /// - account storage use bounds -  min storage will be determined by measuring account storage usage
    #[init]
    pub fn deploy(
        owner: Option<ValidAccountId>,
        storage_usage_bounds: Option<StorageUsageBounds>,
    ) -> Self {
        assert!(!env::state_exists(), "contract is already initialized");

        ContractOwnershipComponent::deploy(
            owner.unwrap_or_else(|| env::predecessor_account_id().try_into().unwrap()),
        );

        AccountManager::deploy(storage_usage_bounds.unwrap_or_else(|| StorageUsageBounds {
            min: AccountManager::measure_storage_usage(()),
            max: None,
        }));

        Self
    }
}

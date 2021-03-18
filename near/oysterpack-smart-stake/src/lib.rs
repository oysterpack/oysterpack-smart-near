mod account_metrics;
mod account_storage_usage;
mod components;
mod contract_ownership;
mod storage_management;

use components::*;
use near_sdk::json_types::U64;
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env,
    json_types::ValidAccountId,
    near_bindgen,
    serde::{Deserialize, Serialize},
    PanicOnDefault,
};
use oysterpack_smart_account_management::StorageUsageBounds;
use oysterpack_smart_contract::components::contract_ownership::ContractOwnershipComponent;
use oysterpack_smart_near::component::Deploy;
use std::convert::TryFrom;

near_sdk::setup_alloc!();

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract;

#[near_bindgen]
impl Contract {
    pub fn storage() -> U64 {
        env::storage_usage().into()
    }

    /// Default config values:
    /// - contract owner = predecessor Account ID
    /// - account storage use bounds -  min storage will be determined by measuring account storage usage
    #[init]
    pub fn deploy(config: Option<DeploymentConfig>) -> Self {
        assert!(!env::state_exists(), "contract is already initialized");

        {
            let owner = config
                .as_ref()
                .map_or_else(env::predecessor_account_id, |config| {
                    config.owner.as_ref().to_string()
                });
            let owner = ValidAccountId::try_from(owner).unwrap();
            ContractOwnershipComponent::deploy(Some(owner));
        }

        {
            let default_storage_usage_bounds = || StorageUsageBounds {
                min: AccountManager::measure_storage_usage(()),
                max: None,
            };

            let storage_usage_bounds = config.map_or_else(default_storage_usage_bounds, |config| {
                config
                    .storage_usage_bounds
                    .unwrap_or_else(default_storage_usage_bounds)
            });

            AccountManager::deploy(Some(storage_usage_bounds));
        }

        Self
    }
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct DeploymentConfig {
    owner: ValidAccountId,
    storage_usage_bounds: Option<StorageUsageBounds>,
}

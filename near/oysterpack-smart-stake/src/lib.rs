mod access_control;
mod account_storage_usage;
mod components;
mod contract_metrics;
mod contract_ownership;
mod contract_sale;
mod fungible_token;
mod storage_management;

use components::*;
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env,
    json_types::ValidAccountId,
    near_bindgen, PanicOnDefault,
};
use oysterpack_smart_account_management::components::account_management::AccountManagementComponentConfig;
use oysterpack_smart_account_management::StorageUsageBounds;
use oysterpack_smart_contract::components::contract_ownership::ContractOwnershipComponent;
use oysterpack_smart_fungible_token::components::fungible_token::FungibleTokenComponent;
use oysterpack_smart_near::component::{Deploy, ManagesAccountData};
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
    pub fn deploy(owner: Option<ValidAccountId>) -> Self {
        let owner = owner.unwrap_or_else(|| env::predecessor_account_id().try_into().unwrap());
        ContractOwnershipComponent::deploy(owner.clone());

        AccountManager::deploy(AccountManagementComponentConfig {
            storage_usage_bounds: None,
            admin_account: owner,
            component_account_storage_mins: Some(vec![StakeFungibleToken::account_storage_min]),
        });

        Self
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test() {}
}

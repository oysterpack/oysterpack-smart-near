mod access_control;
mod account_storage_usage;
mod components;
mod contract_metrics;
mod contract_operator;
mod contract_ownership;
mod contract_sale;
mod fungible_token;
mod staking_pool;
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
use oysterpack_smart_fungible_token::components::fungible_token::{
    FungibleTokenComponent, FungibleTokenConfig,
};
use oysterpack_smart_fungible_token::*;
use oysterpack_smart_near::component::{Deploy, ManagesAccountData};
use oysterpack_smart_near::domain::PublicKey;
use oysterpack_smart_staking_pool::components::staking_pool::{
    StakingPoolComponent, StakingPoolComponentConfig,
};
use std::convert::TryInto;

near_sdk::setup_alloc!();

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract;

#[near_bindgen]
impl Contract {
    /// If owner is not specified, then predecessor Account ID will be set as the contract owner.
    #[init]
    pub fn deploy(owner: Option<ValidAccountId>, stake_public_key: PublicKey) -> Self {
        let owner = owner.unwrap_or_else(|| env::predecessor_account_id().try_into().unwrap());
        ContractOwnershipComponent::deploy(owner.clone());

        AccountManager::deploy(AccountManagementComponentConfig {
            storage_usage_bounds: None,
            admin_account: owner,
            component_account_storage_mins: Some(vec![StakeFungibleToken::account_storage_min]),
        });

        StakeFungibleToken::deploy(FungibleTokenConfig {
            metadata: Metadata {
                spec: Spec(FT_METADATA_SPEC.to_string()),
                name: Name("STAKE".to_string()),
                symbol: Symbol("STAKE".to_string()),
                decimals: 24,
                icon: None,
                reference: None,
                reference_hash: None,
            },
            token_supply: 0,
        });

        StakingPoolComponent::deploy(StakingPoolComponentConfig { stake_public_key });

        Self
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test() {}
}

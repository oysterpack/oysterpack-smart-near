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
use oysterpack_smart_account_management::{
    components::account_management::AccountManagementComponentConfig, AccountRepository,
    StorageUsageBounds,
};
use oysterpack_smart_contract::{
    components::contract_operator::ContractOperatorComponent, ContractOperator,
};
use oysterpack_smart_contract::{
    components::contract_ownership::ContractOwnershipComponent, ContractOwnership,
};
use oysterpack_smart_fungible_token::components::fungible_token::{
    FungibleTokenComponent, FungibleTokenConfig,
};
use oysterpack_smart_fungible_token::*;
use oysterpack_smart_near::domain::BasisPoints;
use oysterpack_smart_near::{
    component::{Deploy, ManagesAccountData},
    domain::PublicKey,
};
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
    /// - owner account is granted admin permission
    /// - default fees: staking fee = 0 BPS, earnings fee = 100 BPS
    /// - STAKE FT symbol defaults to the first part of the contract account ID and uppercased, e.g. if the contract
    ///   account ID is "pearl.stake-v1.oysterpack.near", then the symbol will be "PEARL"
    #[init]
    pub fn deploy(
        stake_public_key: PublicKey,
        owner: Option<ValidAccountId>,
        staking_fee: Option<BasisPoints>,
        earnings_fee: Option<BasisPoints>,
        stake_symbol: Option<String>,
    ) -> Self {
        let owner = owner.unwrap_or_else(|| env::predecessor_account_id().try_into().unwrap());
        ContractOwnershipComponent::deploy(owner.clone());

        AccountManager::deploy(AccountManagementComponentConfig {
            storage_usage_bounds: None,
            admin_account: owner.clone(),
            component_account_storage_mins: Some(vec![StakeFungibleToken::account_storage_min]),
        });

        // transfer any contract balance to the owner - minus the contract operational balance
        {
            let mut contract_operator = ContractOperatorComponent::new(Self::account_manager());
            contract_operator.ops_operator_lock_storage_balance(10000.into());
            let account_manager = Self::account_manager();
            let mut owner_account = account_manager.registered_account_near_data(owner.as_ref());
            owner_account
                .incr_near_balance(ContractOwnershipComponent.ops_owner_balance().available);
            owner_account.save();
        }

        let stake_symbol = stake_symbol.unwrap_or_else(|| {
            let contract_account_id = env::current_account_id();
            match contract_account_id.as_str().find('.') {
                Some(index) => contract_account_id[0..index].to_string(),
                None => contract_account_id,
            }
        });

        StakeFungibleToken::deploy(FungibleTokenConfig {
            metadata: Metadata {
                spec: Spec(FT_METADATA_SPEC.to_string()),
                name: Name::from("STAKE"),
                symbol: Symbol(stake_symbol.to_uppercase()),
                decimals: 24,
                icon: None,
                reference: None,
                reference_hash: None,
            },
            token_supply: 0,
        });

        StakingPoolComponent::deploy(StakingPoolComponentConfig {
            stake_public_key,
            staking_fee: staking_fee.or(Some(0.into())),
            earnings_fee: earnings_fee.or(Some(100.into())),
        });

        Self
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn parsing_stake_symbol_from_account_id() {
        let contract_account_id = "dev-123".to_string();
        let symbol = match contract_account_id.as_str().find('.') {
            Some(index) => contract_account_id[0..index].to_string(),
            None => contract_account_id.clone(),
        };
        assert_eq!(symbol, contract_account_id);

        let contract_account_id = "stake.stake-v1.oysterpack".to_string();
        let symbol = match contract_account_id.as_str().find('.') {
            Some(index) => contract_account_id[0..index].to_string(),
            None => contract_account_id.clone(),
        };
        assert_eq!(symbol, "stake");
    }
}

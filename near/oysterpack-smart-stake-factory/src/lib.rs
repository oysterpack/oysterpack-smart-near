use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env,
    json_types::ValidAccountId,
    near_bindgen, PanicOnDefault, Promise,
};
use oysterpack_smart_near::{
    domain::{BasisPoints, PublicKey},
    to_valid_account_id, YOCTO,
};

near_sdk::setup_alloc!();

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract;

#[near_bindgen]
impl Contract {
    /// Used to deploy an instance of the STAKE pool contract
    ///
    /// ## Args
    /// - stake_symbol - will be used to create the child account ID, which will also be used as the STAKE FT symbol
    /// - stake_public_key - public key that binds the STAKE pool to the validator node
    /// - owner - STAKE pool owner
    /// - staking_fee - default is 0 BPS
    /// - earnings_fee - default is 100 BPS
    #[payable]
    pub fn deploy(
        stake_symbol: String,
        stake_public_key: PublicKey,
        owner: Option<ValidAccountId>,
        staking_fee: Option<BasisPoints>,
        earnings_fee: Option<BasisPoints>,
    ) -> Promise {
        let stake_pool_account_id = {
            let stake_pool_account_id = format!("{}.{}", stake_symbol, env::current_account_id());
            to_valid_account_id(&stake_pool_account_id)
        };

        let stake_contract_wasm_bytes = Self::stake_contract_wasm_bytes();
        let contract_storage_costs =
            stake_contract_wasm_bytes.len() as u128 * env::storage_byte_cost() + YOCTO;

        todo!()
    }
}

impl Contract {
    fn stake_contract_wasm_bytes() -> Vec<u8> {
        include_bytes!("../../target/wasm32-unknown-unknown/release/oysterpack_smart_stake.wasm")
            .to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn load_stake_contract_wasm_file() {
        let stake_contract_wasm_bytes = Contract::stake_contract_wasm_bytes();

        println!(
            "stake_contract_wasm_bytes.len() = {}",
            stake_contract_wasm_bytes.len()
        );
    }
}

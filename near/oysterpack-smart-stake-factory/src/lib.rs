use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env,
    json_types::ValidAccountId,
    near_bindgen, PanicOnDefault, Promise,
};
use oysterpack_smart_near::domain::{BasisPoints, PublicKey};

near_sdk::setup_alloc!();

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract;

#[near_bindgen]
impl Contract {
    #[payable]
    pub fn deploy(
        stake_pool_id: String,
        stake_public_key: PublicKey,
        owner: Option<ValidAccountId>,
        staking_fee: Option<BasisPoints>,
        earnings_fee: Option<BasisPoints>,
    ) -> Promise {
        let stake_contract_wasm_bytes = include_bytes!(
            "../../target/wasm32-unknown-unknown/release/oysterpack_smart_stake.wasm"
        );

        todo!()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn load_stake_contract_wasm_file() {
        let stake_contract_wasm_bytes = include_bytes!(
            "../../target/wasm32-unknown-unknown/release/oysterpack_smart_stake.wasm"
        )
        .to_vec();
        println!(
            "stake_contract_wasm_bytes.len() = {}",
            stake_contract_wasm_bytes.len()
        );
    }
}

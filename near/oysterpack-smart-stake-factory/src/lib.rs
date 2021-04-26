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
    pub fn deploy(
        stake_public_key: PublicKey,
        owner: Option<ValidAccountId>,
        staking_fee: Option<BasisPoints>,
        earnings_fee: Option<BasisPoints>,
    ) -> Promise {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}

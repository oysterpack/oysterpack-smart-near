use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
    AccountId,
};
use oysterpack_smart_near::domain::{Expiration, YoctoNear};

#[derive(
    BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, Copy, Debug, PartialEq,
)]
#[serde(crate = "near_sdk::serde")]
pub struct ContractBid {
    pub amount: YoctoNear,
    pub expiration: Option<Expiration>,
}

impl ContractBid {
    pub fn expired(&self) -> bool {
        self.expiration
            .map_or(false, |expiration| expiration.expired())
    }
}

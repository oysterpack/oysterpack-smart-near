use crate::{BalanceId, ContractNearBalances};
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
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

    /// Used to track the contract bid on the contract NEAR balance
    pub const CONTRACT_BID_BALANCE_ID: BalanceId = BalanceId(255);

    pub fn near_balance() -> YoctoNear {
        ContractNearBalances::near_balance(Self::CONTRACT_BID_BALANCE_ID)
    }

    pub(crate) fn set_near_balance(bid: YoctoNear) {
        ContractNearBalances::set_balance(Self::CONTRACT_BID_BALANCE_ID, bid);
    }

    pub(crate) fn clear_near_balance() {
        ContractNearBalances::clear_balance(Self::CONTRACT_BID_BALANCE_ID);
    }
}

use crate::{BalanceId, ContractNearBalances, ERR_BID_IS_EXPIRED};
use oysterpack_smart_near::domain::{Expiration, ExpirationSetting, YoctoNear};
use oysterpack_smart_near::near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
};

#[derive(
    BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, Copy, Debug, PartialEq,
)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub struct ContractBid {
    pub amount: YoctoNear,
    pub expiration: Option<Expiration>,
}

impl ContractBid {
    pub fn expired(&self) -> bool {
        self.expiration
            .map_or(false, |expiration| expiration.expired())
    }

    pub fn assert_not_expired(&self) {
        ERR_BID_IS_EXPIRED.assert(|| !self.expired());
    }

    /// ## Panics
    /// if bid becomes expired
    pub(crate) fn update_expiration(&mut self, expiration: Option<ExpirationSetting>) {
        if let Some(expiration) = expiration {
            let expiration: Expiration = expiration.into();
            ERR_BID_IS_EXPIRED.assert(|| !expiration.expired());
            self.expiration = Some(expiration);
        }
    }

    /// Used to track the contract bid on the contract NEAR balance
    pub const CONTRACT_BID_BALANCE_ID: BalanceId = BalanceId(255);

    pub fn near_balance() -> YoctoNear {
        ContractNearBalances::near_balance(Self::CONTRACT_BID_BALANCE_ID)
    }

    pub(crate) fn set_near_balance(bid: YoctoNear) {
        ContractNearBalances::set_balance(Self::CONTRACT_BID_BALANCE_ID, bid);
    }

    pub(crate) fn incr_near_balance(amount: YoctoNear) {
        ContractNearBalances::incr_balance(Self::CONTRACT_BID_BALANCE_ID, amount);
    }

    pub(crate) fn decr_near_balance(amount: YoctoNear) {
        ContractNearBalances::decr_balance(Self::CONTRACT_BID_BALANCE_ID, amount);
    }

    pub(crate) fn clear_near_balance() {
        ContractNearBalances::clear_balance(Self::CONTRACT_BID_BALANCE_ID);
    }
}

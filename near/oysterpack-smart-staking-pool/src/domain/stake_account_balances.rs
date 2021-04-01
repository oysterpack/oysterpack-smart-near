use oysterpack_smart_account_management::StorageBalance;
use oysterpack_smart_fungible_token::TokenAmount;
use oysterpack_smart_near::domain::YoctoNear;
use oysterpack_smart_near::near_sdk::serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub struct StakeAccountBalances {
    pub storage_balance: StorageBalance,
    pub stake_token_balance: Option<StakeBalance>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub struct StakeBalance {
    pub stake: TokenAmount,
    /// how much the STAKE balance is currently worth in NEAR
    pub near_value: YoctoNear,
}

use crate::UnstakedBalances;
use oysterpack_smart_near::domain::YoctoNear;
use oysterpack_smart_near::near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
};

#[derive(
    BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, Clone, Copy, PartialEq,
)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub struct StakeAccountBalances {
    /// account's total NEAR balance, which includes:
    /// - staked NEAR balance
    /// - unstaked NEAR balance
    pub total: YoctoNear,
    /// unstaked amount that is available to withdraw
    pub available: YoctoNear,
    /// amount that is currently staked
    pub staked: YoctoNear,
    /// amount that is unstaked and currently locked
    /// - key represents the epoch height when the unstaked NEAR will become available for withdrawal
    /// - each unstaking is tracked separately at the account's storage expense
    ///   - because the lockup period is 4 epochs, at most the map will contain 4 entries
    /// - the unstake amount can be updated within the same epoch
    pub unstaked: UnstakedBalances,
}

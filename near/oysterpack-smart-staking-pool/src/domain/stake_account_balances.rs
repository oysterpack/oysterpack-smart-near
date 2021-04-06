use crate::UnstakedBalances;
use oysterpack_smart_account_management::StorageBalance;
use oysterpack_smart_fungible_token::TokenAmount;
use oysterpack_smart_near::domain::{EpochHeight, YoctoNear};
use oysterpack_smart_near::near_sdk::serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub struct StakeAccountBalances {
    pub storage_balance: StorageBalance,
    pub staked: Option<StakedBalance>,
    pub unstaked: Option<UnstakedBalance>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub struct StakedBalance {
    pub stake: TokenAmount,
    /// how much the STAKE balance is currently worth in NEAR
    pub near_value: YoctoNear,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub struct UnstakedBalance {
    total: YoctoNear,
    available: YoctoNear,
    locked: Option<BTreeMap<EpochHeight, YoctoNear>>,
}

impl From<UnstakedBalances> for UnstakedBalance {
    fn from(mut balance: UnstakedBalances) -> Self {
        balance.unlock();
        Self {
            total: balance.total(),
            available: balance.available(),
            locked: balance.locked(),
        }
    }
}

use crate::UnstakedBalances;
use oysterpack_smart_near::domain::YoctoNear;
use oysterpack_smart_near::near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
};

#[derive(
    BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Default,
)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub struct StakeAccount {
    staked: YoctoNear,
    unstaked: Option<UnstakedBalances>,
}

impl StakeAccount {
    pub fn staked_balance(&self) -> YoctoNear {
        self.staked
    }

    pub fn unstaked_balances(&self) -> Option<UnstakedBalances> {
        self.unstaked
    }
}

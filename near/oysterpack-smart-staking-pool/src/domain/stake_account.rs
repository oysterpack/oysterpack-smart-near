use crate::UnstakedBalances;
use oysterpack_smart_near::near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};

#[derive(BorshDeserialize, BorshSerialize, Debug, Clone, PartialEq, Default)]
pub struct StakeAccountData {
    pub unstaked_balances: UnstakedBalances,
}

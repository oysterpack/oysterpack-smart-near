use crate::UnstakedBalances;
use oysterpack_smart_near::domain::{BasisPoints, BlockTimestamp};
use oysterpack_smart_near::near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};

#[derive(BorshDeserialize, BorshSerialize, Debug, Clone, Copy, PartialEq, Default)]
pub struct StakeAccountData {
    pub unstaked_balances: UnstakedBalances,
    pub staking_fee: Option<StakingFee>,
}

#[derive(BorshDeserialize, BorshSerialize, Debug, Clone, Copy, PartialEq, Default)]
pub struct StakingFee {
    pub fee: BasisPoints,
    pub expiration: BlockTimestamp,
}

use crate::*;
use near_sdk::near_bindgen;
use oysterpack_smart_near::domain::{Gas, YoctoNear};
use oysterpack_smart_near::near_sdk::{AccountId, PromiseOrValue};
use oysterpack_smart_staking_pool::components::staking_pool::{State, Status};
use oysterpack_smart_staking_pool::{
    StakeAccountBalances, StakeActionCallbacks, StakingPool, StakingPoolOperator,
    StakingPoolOperatorCommand, StakingPoolOwner,
};

#[near_bindgen]
impl StakingPool for Contract {
    fn ops_stake_balance(&self, account_id: ValidAccountId) -> Option<StakeAccountBalances> {
        Self::staking_pool().ops_stake_balance(account_id)
    }

    #[payable]
    fn ops_stake(&mut self) -> PromiseOrValue<StakeAccountBalances> {
        Self::staking_pool().ops_stake()
    }

    fn ops_unstake(&mut self, amount: Option<YoctoNear>) -> PromiseOrValue<StakeAccountBalances> {
        Self::staking_pool().ops_unstake(amount)
    }

    fn ops_restake(&mut self, amount: Option<YoctoNear>) -> PromiseOrValue<StakeAccountBalances> {
        Self::staking_pool().ops_restake(amount)
    }

    fn ops_stake_withdraw(&mut self, amount: Option<YoctoNear>) -> StakeAccountBalances {
        Self::staking_pool().ops_stake_withdraw(amount)
    }

    fn ops_stake_token_value(&self) -> YoctoNear {
        Self::staking_pool().ops_stake_token_value()
    }

    fn ops_stake_status(&self) -> Status {
        Self::staking_pool().ops_stake_status()
    }
}

#[near_bindgen]
impl StakeActionCallbacks for Contract {
    #[private]
    fn ops_stake_finalize(
        &mut self,
        account_id: AccountId,
        amount: YoctoNear,
        stake_token_amount: TokenAmount,
        total_staked_balance: YoctoNear,
    ) -> StakeAccountBalances {
        Self::staking_pool().ops_stake_finalize(
            account_id,
            amount,
            stake_token_amount,
            total_staked_balance,
        )
    }

    #[private]
    fn ops_unstake_finalize(
        &mut self,
        account_id: AccountId,
        amount: YoctoNear,
        stake_token_amount: TokenAmount,
        total_staked_balance: YoctoNear,
    ) -> StakeAccountBalances {
        Self::staking_pool().ops_unstake_finalize(
            account_id,
            amount,
            stake_token_amount,
            total_staked_balance,
        )
    }

    #[private]
    fn ops_stake_resume_finalize(&mut self, total_staked_balance: YoctoNear) {
        Self::staking_pool().ops_stake_resume_finalize(total_staked_balance);
    }

    #[private]
    fn ops_stake_pause_finalize(&mut self) {
        Self::staking_pool().ops_stake_pause_finalize()
    }
}

#[near_bindgen]
impl StakingPoolOperator for Contract {
    fn ops_stake_operator_command(&mut self, command: StakingPoolOperatorCommand) {
        Self::staking_pool().ops_stake_operator_command(command);
    }

    fn ops_stake_callback_gas(&self) -> Gas {
        Self::staking_pool().ops_stake_callback_gas()
    }

    fn ops_stake_state(&self) -> State {
        Self::staking_pool().ops_stake_state()
    }
}

#[near_bindgen]
impl StakingPoolOwner for Contract {
    fn ops_stake_owner_balance(&mut self, amount: Option<YoctoNear>) -> StakeAccountBalances {
        Self::staking_pool().ops_stake_owner_balance(amount)
    }
}

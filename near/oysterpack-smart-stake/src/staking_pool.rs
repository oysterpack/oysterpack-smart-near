use crate::*;
use near_sdk::near_bindgen;
use oysterpack_smart_near::domain::{BasisPoints, YoctoNear};
use oysterpack_smart_near::near_sdk::{AccountId, Promise, PromiseOrValue};
use oysterpack_smart_staking_pool::{
    StakeAccountBalances, StakeActionCallbacks, StakingPool, StakingPoolBalances,
    StakingPoolOperator, StakingPoolOperatorCommand, Status, Treasury,
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

    #[payable]
    fn ops_stake_transfer(
        &mut self,
        receiver_id: ValidAccountId,
        amount: YoctoNear,
        memo: Option<Memo>,
    ) -> TokenAmount {
        Self::staking_pool().ops_stake_transfer(receiver_id, amount, memo)
    }

    #[payable]
    fn ops_stake_transfer_call(
        &mut self,
        receiver_id: ValidAccountId,
        amount: YoctoNear,
        memo: Option<Memo>,
        msg: TransferCallMessage,
    ) -> Promise {
        Self::staking_pool().ops_stake_transfer_call(receiver_id, amount, memo, msg)
    }

    fn ops_stake_token_value(&self, amount: Option<TokenAmount>) -> YoctoNear {
        Self::staking_pool().ops_stake_token_value(amount)
    }

    fn ops_stake_token_value_with_earnings(&mut self, amount: Option<TokenAmount>) -> YoctoNear {
        Self::staking_pool().ops_stake_token_value_with_earnings(amount)
    }

    fn ops_stake_status(&self) -> Status {
        Self::staking_pool().ops_stake_status()
    }

    fn ops_stake_pool_balances(&self) -> StakingPoolBalances {
        Self::staking_pool().ops_stake_pool_balances()
    }

    fn ops_stake_fee(&self) -> BasisPoints {
        Self::staking_pool().ops_stake_fee()
    }

    fn ops_stake_public_key(&self) -> PublicKey {
        Self::staking_pool().ops_stake_public_key()
    }
}

#[near_bindgen]
impl StakeActionCallbacks for Contract {
    #[private]
    fn ops_stake_finalize(&mut self, account_id: AccountId) -> StakeAccountBalances {
        Self::staking_pool().ops_stake_finalize(account_id)
    }

    #[private]
    fn ops_stake_start_finalize(&mut self) {
        Self::staking_pool().ops_stake_start_finalize();
    }

    #[private]
    fn ops_stake_stop_finalize(&mut self) {
        Self::staking_pool().ops_stake_stop_finalize()
    }
}

#[near_bindgen]
impl StakingPoolOperator for Contract {
    fn ops_stake_operator_command(&mut self, command: StakingPoolOperatorCommand) {
        Self::staking_pool().ops_stake_operator_command(command);
    }
}

#[near_bindgen]
impl Treasury for Contract {
    #[payable]
    fn ops_stake_treasury_deposit(&mut self) -> PromiseOrValue<StakeAccountBalances> {
        Self::staking_pool().ops_stake_treasury_deposit()
    }

    #[payable]
    fn ops_stake_treasury_distribution(&mut self) {
        Self::staking_pool().ops_stake_treasury_distribution();
    }

    fn ops_stake_treasury_transfer_to_owner(&mut self, amount: Option<YoctoNear>) {
        Self::staking_pool().ops_stake_treasury_transfer_to_owner(amount);
    }

    fn ops_stake_treasury_grant_treasurer(&mut self, account_id: ValidAccountId) {
        Self::staking_pool().ops_stake_treasury_grant_treasurer(account_id);
    }

    fn ops_stake_treasury_revoke_treasurer(&mut self, account_id: ValidAccountId) {
        Self::staking_pool().ops_stake_treasury_revoke_treasurer(account_id);
    }
}

use crate::{StakeAccount, StakeAccountBalances, StakeAmount, StakingPool};
use oysterpack_smart_account_management::components::account_management::AccountManagementComponent;
use oysterpack_smart_contract::BalanceId;
use oysterpack_smart_near::domain::{PublicKey, YoctoNear};
use oysterpack_smart_near::near_sdk::json_types::ValidAccountId;

pub struct StakingPoolComponent {
    account_manager: AccountManagementComponent<StakeAccount>,
}

impl StakingPool for StakingPoolComponent {
    fn ops_stake_balance(&self, account_id: ValidAccountId) -> Option<StakeAccountBalances> {
        unimplemented!()
    }

    fn ops_stake(&mut self, amount: Option<StakeAmount>) -> StakeAccountBalances {
        unimplemented!()
    }

    fn ops_unstake(&mut self, amount: Option<YoctoNear>) -> StakeAccountBalances {
        unimplemented!()
    }
}

struct State {
    stake_public_key: PublicKey,
}

///
pub const UNSTAKED_WITHDRAWAL_LIQUIDITY: BalanceId = BalanceId(0);
pub const TOTAL_UNSTAKED_BALANCE: BalanceId = BalanceId(1);
pub const TOTAL_STAKED_BALANCE: BalanceId = BalanceId(2);

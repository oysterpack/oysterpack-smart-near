use oysterpack_smart_near::domain::Gas;

pub trait StakingPoolOperator {
    fn ops_stake_callback_gas(&self) -> Gas;
}

use crate::components::staking_pool::State;
use oysterpack_smart_near::domain::Gas;
use oysterpack_smart_near::near_sdk::serde::{Deserialize, Serialize};

pub trait StakingPoolOperator {
    /// Executes the specified operator command
    ///
    /// ## Panics
    /// - if predecessor account is not registered
    /// - if predecessor account is not authorized - requires operator permission
    fn ops_stake_operator_command(&mut self, command: StakingPoolOperatorCommand);

    /// returns the amount of gas that will be allotted for the stake callbacks
    fn ops_stake_callback_gas(&self) -> Gas;

    fn ops_stake_state(&self) -> State;
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub enum StakingPoolOperatorCommand {
    Pause,
    Resume,
    SetStakeCallbackGas(Gas),
    ClearStakeCallbackGas,
}

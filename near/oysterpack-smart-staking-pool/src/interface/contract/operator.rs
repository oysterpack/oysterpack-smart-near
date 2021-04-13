use crate::components::staking_pool::State;
use oysterpack_smart_near::domain::{BasisPoints, PublicKey};
use oysterpack_smart_near::near_sdk::serde::{Deserialize, Serialize};

/// # **Contract Interface**: Staking Pool Operator API
pub trait StakingPoolOperator {
    /// Executes the specified operator command
    ///
    /// ## Panics
    /// - if predecessor account is not registered
    /// - if predecessor account is not authorized - requires operator permission
    fn ops_stake_operator_command(&mut self, command: StakingPoolOperatorCommand);

    fn ops_stake_state(&self) -> State;
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub enum StakingPoolOperatorCommand {
    StopStaking,
    StartStaking,

    /// the staking pool public key can only be changed while the staking pool is offline
    UpdatePublicKey(PublicKey),
    UpdateStakingFee(BasisPoints),
}

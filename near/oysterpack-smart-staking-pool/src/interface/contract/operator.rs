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
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub enum StakingPoolOperatorCommand {
    StopStaking,
    StartStaking,

    /// the staking pool public key can only be changed while the staking pool is offline
    UpdatePublicKey(PublicKey),
    /// max fee is 1000 BPS (10%)
    UpdateStakingFee(BasisPoints),
}

/// 10%
pub const MAX_STAKING_FEE: BasisPoints = BasisPoints(1000);

#[cfg(test)]
mod tests {
    use super::*;
    use oysterpack_smart_near_test::near_sdk::serde_json;

    #[test]
    fn json_serialization() {
        println!(
            "{}",
            serde_json::to_string_pretty(&StakingPoolOperatorCommand::StartStaking).unwrap()
        );
    }
}

use crate::domain::TGas;
use crate::TERA;
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{
        de::{self, Visitor},
        Deserialize, Deserializer, Serialize, Serializer,
    },
    serde_json, RuntimeFeesConfig,
};
use std::ops::{Add, AddAssign, Sub, SubAssign};
use std::{
    fmt::{self, Display, Formatter},
    ops::{Deref, DerefMut},
};

pub const ZERO_GAS: Gas = Gas(0);

/// provides support to also [compute][`Gas::compute`] runtime gas costs
#[derive(
    BorshSerialize, BorshDeserialize, Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Default,
)]
pub struct Gas(pub u64);

pub type TransactionResourceCount = u8;

impl Gas {
    pub fn value(&self) -> u64 {
        self.0
    }

    /// compute the runtime cost for the transaction resource
    pub fn compute(costs: Vec<(TransactionResource, TransactionResourceCount)>) -> Gas {
        let runtime_fees = RuntimeFeesConfig::default();
        costs
            .iter()
            .fold(0_u64, |gas_cost, (cost, count)| {
                assert!(*count > 0, "transaction resource count must not be zero");
                let cost = match cost {
                    TransactionResource::ActionReceipt(SenderIsReceiver(sir)) => {
                        let fee = &runtime_fees.action_receipt_creation_config;
                        fee.send_fee(*sir) + fee.execution
                    }
                    TransactionResource::DataReceipt(SenderIsReceiver(sir), ByteLen(len)) => {
                        let fee = &runtime_fees.data_receipt_creation_config.cost_per_byte;
                        let cost_per_byte = fee.send_fee(*sir) + fee.execution;

                        let fee = &runtime_fees.data_receipt_creation_config.base_cost;
                        let base_cost = fee.send_fee(*sir) + fee.execution;

                        base_cost + (cost_per_byte * len)
                    }
                    TransactionResource::Action(action) => match action {
                        ActionType::CreateAccount => {
                            let fee = &runtime_fees.action_creation_config.create_account_cost;
                            fee.send_not_sir + fee.execution
                        }
                        ActionType::DeployContract(ByteLen(len)) => {
                            let fee = &runtime_fees
                                .action_creation_config
                                .deploy_contract_cost_per_byte;
                            let cost_per_byte = fee.send_not_sir + fee.execution;

                            let fee = &runtime_fees.action_creation_config.deploy_contract_cost;
                            let base_cost = fee.send_not_sir + fee.execution;

                            base_cost + (cost_per_byte * len)
                        }
                        ActionType::FunctionCall(SenderIsReceiver(sir), ByteLen(len)) => {
                            let fee = &runtime_fees
                                .action_creation_config
                                .function_call_cost_per_byte;
                            let cost_per_byte = fee.send_fee(*sir) + fee.execution;

                            let fee = &runtime_fees.action_creation_config.function_call_cost;
                            let base_cost = fee.send_fee(*sir) + fee.execution;

                            base_cost + (cost_per_byte * len)
                        }
                        ActionType::Transfer => {
                            let fee = &runtime_fees.action_creation_config.transfer_cost;
                            fee.send_not_sir + fee.execution
                        }
                        ActionType::Stake(SenderIsReceiver(sir)) => {
                            let fee = &runtime_fees.action_creation_config.stake_cost;
                            fee.send_fee(*sir) + fee.execution
                        }
                        ActionType::AccessKeyCreation(
                            SenderIsReceiver(sir),
                            key_type,
                            ByteLen(len),
                        ) => {
                            let fee = &runtime_fees.action_creation_config.add_key_cost;
                            match key_type {
                                AccessKeyType::FullAccess => {
                                    let fee = &fee.full_access_cost;
                                    fee.send_fee(*sir) + fee.execution
                                }
                                AccessKeyType::FunctionCall => {
                                    let per_byte_fee = &fee.function_call_cost_per_byte;
                                    let cost_per_byte =
                                        per_byte_fee.send_fee(*sir) + per_byte_fee.execution;

                                    let base_fee = &fee.function_call_cost;
                                    let base_cost = base_fee.send_fee(*sir) + base_fee.execution;

                                    base_cost + (cost_per_byte * len)
                                }
                            }
                        }
                        ActionType::DeleteKey(SenderIsReceiver(sir)) => {
                            let fee = &runtime_fees.action_creation_config.delete_key_cost;
                            fee.send_fee(*sir) + fee.execution
                        }
                        ActionType::DeleteAccount(SenderIsReceiver(sir)) => {
                            let fee = &runtime_fees.action_creation_config.delete_account_cost;
                            fee.send_fee(*sir) + fee.execution
                        }
                    },
                };
                gas_cost + (cost * *count as u64)
            })
            .into()
    }
}

impl From<TGas> for Gas {
    fn from(gas: TGas) -> Self {
        Self(gas.value() * TERA)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum TransactionResource {
    ActionReceipt(SenderIsReceiver),
    DataReceipt(SenderIsReceiver, ByteLen),
    Action(ActionType),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ActionType {
    CreateAccount,
    DeployContract(ByteLen),
    FunctionCall(SenderIsReceiver, ByteLen),
    Transfer,
    Stake(SenderIsReceiver),
    AccessKeyCreation(SenderIsReceiver, AccessKeyType, ByteLen),
    DeleteKey(SenderIsReceiver),
    DeleteAccount(SenderIsReceiver),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct SenderIsReceiver(pub bool);

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ByteLen(pub u64);

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum AccessKeyType {
    FullAccess,
    FunctionCall,
}

impl From<u64> for Gas {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl Deref for Gas {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Gas {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Add<Gas> for Gas {
    type Output = Self;

    fn add(self, rhs: Gas) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl AddAssign<Gas> for Gas {
    fn add_assign(&mut self, rhs: Gas) {
        self.0 += rhs.0
    }
}

impl Sub<Gas> for Gas {
    type Output = Self;

    fn sub(self, rhs: Gas) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl SubAssign<Gas> for Gas {
    fn sub_assign(&mut self, rhs: Gas) {
        self.0 -= rhs.0
    }
}

impl Display for Gas {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Serialize for Gas {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        let value = self.0.to_string();
        serializer.serialize_str(&value)
    }
}

impl<'de> Deserialize<'de> for Gas {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(YoctoNearVisitor)
    }
}

struct YoctoNearVisitor;

impl<'de> Visitor<'de> for YoctoNearVisitor {
    type Value = Gas;

    fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("u64 serialized as string")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let value: u64 = serde_json::from_str(v)
            .map_err(|_| de::Error::custom("JSON parsing failed for YoctoNear"))?;
        Ok(Gas(value))
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_str(&v)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::TERA;
    use near_sdk::test_utils::test_env;

    #[test]
    fn json_serialization() {
        let amount = Gas::from(100);
        let amount_as_json = serde_json::to_string(&amount).unwrap();
        println!("{}", amount_as_json);

        let amount2: Gas = serde_json::from_str(&amount_as_json).unwrap();
        assert_eq!(amount, amount2);
    }

    #[test]
    fn compute_gas() {
        test_env::setup();

        let func_call_receipt = TransactionResource::ActionReceipt(SenderIsReceiver(false));
        let func_call_action = TransactionResource::Action(ActionType::FunctionCall(
            SenderIsReceiver(false),
            ByteLen(100),
        ));
        let data_receipt = TransactionResource::DataReceipt(SenderIsReceiver(false), ByteLen(100));
        let gas = Gas::compute(vec![
            (func_call_receipt, 1),
            (func_call_action, 1),
            (data_receipt, 2),
        ]);
        println!("gas = {}", gas);
        println!("gas = {} rem {}", *gas / TERA, *gas % TERA);
    }
}

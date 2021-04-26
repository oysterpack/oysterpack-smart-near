use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env, is_promise_success,
    json_types::ValidAccountId,
    near_bindgen,
    serde::{Deserialize, Serialize},
    AccountId, PanicOnDefault, Promise,
};
use oysterpack_smart_near::asserts::ERR_INVALID;
use oysterpack_smart_near::domain::{
    ActionType, Gas, SenderIsReceiver, TGas, TransactionResource, YoctoNear,
};
use oysterpack_smart_near::{
    domain::{BasisPoints, PublicKey},
    json_function_callback, to_valid_account_id, ErrCode, Level, LogEvent, TERA, YOCTO,
};

near_sdk::setup_alloc!();

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract;

pub const ERR_INSUFFICIENT_ATTACHED_DEPOSIT: ErrCode = ErrCode("INSUFFICIENT_ATTACHED_DEPOSIT");
pub const ERR_STAKE_POOL_DEPLOY_FAILURE: ErrCode = ErrCode("STAKE_POOL_DEPLOY_FAILURE");

pub const LOG_EVENT_STAKE_POOL_DEPLOY_SUCCESS: LogEvent =
    LogEvent(Level::INFO, "STAKE_POOL_DEPLOY_SUCCESS");

/// conservatively overestimated
const STAKE_DEPLOY_GAS: Gas = Gas(100 * TERA);

#[near_bindgen]
impl Contract {
    #[init]
    pub fn init() -> Self {
        Self
    }

    /// Used to deploy an instance of the STAKE pool contract
    ///
    /// ## Args
    /// - stake_symbol - will be used to create the child account ID, which will also be used as the STAKE FT symbol
    /// - stake_public_key - public key that binds the STAKE pool to the validator node
    /// - owner - STAKE pool owner
    /// - staking_fee - default is 0 BPS
    /// - earnings_fee - default is 100 BPS
    #[payable]
    pub fn deploy(
        stake_symbol: String,
        stake_public_key: PublicKey,
        owner: Option<ValidAccountId>,
        staking_fee: Option<BasisPoints>,
        earnings_fee: Option<BasisPoints>,
    ) -> Promise {
        let stake_pool_account_id = {
            let stake_pool_account_id = format!(
                "{}.{}",
                stake_symbol.to_lowercase(),
                env::current_account_id()
            );
            to_valid_account_id(&stake_pool_account_id)
        };

        let stake_contract_wasm_bytes = {
            let stake_contract_wasm_bytes = Self::stake_contract_wasm_bytes();
            let contract_storage_costs =
                stake_contract_wasm_bytes.len() as u128 * env::storage_byte_cost();
            // an extra NEAR is used to pay for contract operational storage costs
            let min_required_deposit = contract_storage_costs + YOCTO;
            ERR_INSUFFICIENT_ATTACHED_DEPOSIT.assert(
                || env::attached_deposit() >= min_required_deposit,
                || format!("No enough deposit was attached for deploying the STAKE pool contract. Min required attached deposit is {} yoctoNEAR", min_required_deposit),
            );
            stake_contract_wasm_bytes
        };

        let deploy = Promise::new(stake_pool_account_id.as_ref().clone())
            .create_account()
            .transfer(env::attached_deposit())
            .deploy_contract(stake_contract_wasm_bytes)
            .function_call(
                b"deploy".to_vec(),
                near_sdk::serde_json::to_vec(&StakePoolDeployArgs {
                    stake_public_key,
                    owner: owner.or(Some(to_valid_account_id(&env::predecessor_account_id()))),
                    staking_fee,
                    earnings_fee,
                    stake_symbol: Some(stake_symbol),
                })
                .unwrap(),
                0,
                *STAKE_DEPLOY_GAS,
            );
        let finalize = json_function_callback(
            "on_deploy",
            Some(OnDeployArgs {
                account_id: env::predecessor_account_id(),
                deposit: env::attached_deposit().into(),
            }),
            YoctoNear::ZERO,
            Self::callback_gas(),
        );
        deploy.then(finalize)
    }

    #[private]
    pub fn on_deploy(&mut self, account_id: AccountId, deposit: YoctoNear) {
        if is_promise_success() {
            LOG_EVENT_STAKE_POOL_DEPLOY_SUCCESS.log("");
        } else {
            ERR_STAKE_POOL_DEPLOY_FAILURE.log("");
            Promise::new(account_id).transfer(*deposit);
        }
    }
}

impl Contract {
    fn stake_contract_wasm_bytes() -> Vec<u8> {
        include_bytes!("../../target/wasm32-unknown-unknown/release/oysterpack_smart_stake.wasm")
            .to_vec()
    }

    fn callback_gas() -> Gas {
        const REMAINING_COMPUTE: Gas = Gas(100 * TERA);
        let gas =
            (env::prepaid_gas() - env::used_gas() - *STAKE_DEPLOY_GAS - *REMAINING_COMPUTE).into();

        let min_callback_gas = {
            const RECEIPT: TransactionResource =
                TransactionResource::ActionReceipt(SenderIsReceiver(false));
            const TRANSFER: TransactionResource = TransactionResource::Action(ActionType::Transfer);
            const COMPUTE: TGas = TGas(10); // conservatively overestimated
            Gas::compute(vec![
                (RECEIPT, 1), // transfer
                (TRANSFER, 1),
            ]) + COMPUTE
        };
        ERR_INVALID.assert(
            || gas >= min_callback_gas,
            || {
                let min_required_gas = env::used_gas() + *min_callback_gas + *REMAINING_COMPUTE;
                format!(
                    "not enough gas was attached - min required gas is {} TGas",
                    min_required_gas / TERA + 1 // round up 1 TGas
                )
            },
        );
        gas
    }
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
struct StakePoolDeployArgs {
    stake_public_key: PublicKey,
    owner: Option<ValidAccountId>,
    staking_fee: Option<BasisPoints>,
    earnings_fee: Option<BasisPoints>,
    stake_symbol: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
struct OnDeployArgs {
    account_id: AccountId,
    deposit: YoctoNear,
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::serde_json;
    use oysterpack_smart_near_test::*;

    fn staking_public_key() -> PublicKey {
        serde_json::from_str("\"ed25519:GTi3gtSio5ZYYKTT8WVovqJEob6KqdmkTi8KqGSfwqdm\"").unwrap()
    }

    #[test]
    fn load_stake_contract_wasm_file() {
        let stake_contract_wasm_bytes = Contract::stake_contract_wasm_bytes();

        println!(
            "stake_contract_wasm_bytes.len() = {}",
            stake_contract_wasm_bytes.len()
        );
    }

    #[test]
    fn deploy() {
        let mut ctx = new_context("bob");
        ctx.prepaid_gas = 300 * TERA;
        ctx.attached_deposit = 10 * YOCTO;
        testing_env!(ctx);
        let staking_fee = Some(BasisPoints(10));
        let earnings_fee = Some(BasisPoints(50));
        Contract::deploy(
            "PEARL".to_string(),
            staking_public_key(),
            None,
            staking_fee,
            earnings_fee,
        );

        let receipts = get_receipts();
        assert_eq!(receipts.len(), 2);
        {
            let receipt = &receipts[0];
            assert_eq!(
                receipt.receiver_id,
                format!("pearl.{}", env::current_account_id())
            );
            assert_eq!(receipt.actions.len(), 4);
            {
                let action = &receipt.actions[0];
                match action {
                    Action::CreateAccount => {}
                    _ => panic!("expected CreateAccount"),
                }
            }
            {
                let action = &receipt.actions[1];
                match action {
                    Action::Transfer(action) => {
                        assert_eq!(action.deposit, env::attached_deposit());
                    }
                    _ => panic!("expected Transfer"),
                }
            }
            {
                let action = &receipt.actions[2];
                match action {
                    Action::DeployContract(action) => {
                        println!("code len() = {}", action.code.len());
                    }
                    _ => panic!("expected DeployContract"),
                }
            }
            {
                let action = &receipt.actions[3];
                match action {
                    Action::FunctionCall(action) => {
                        assert_eq!(action.method_name, "deploy");
                        let args: StakePoolDeployArgs = serde_json::from_str(&action.args).unwrap();
                        assert_eq!(args.stake_symbol.unwrap(), "PEARL");
                        assert_eq!(
                            args.owner.unwrap(),
                            to_valid_account_id(env::predecessor_account_id().as_str())
                        );
                        assert_eq!(args.stake_public_key, staking_public_key());
                        assert_eq!(args.staking_fee, staking_fee);
                        assert_eq!(args.earnings_fee, earnings_fee);

                        assert_eq!(action.gas, *STAKE_DEPLOY_GAS);
                    }
                    _ => panic!("expected FunctionCall"),
                }
            }
        }
        {
            let receipt = &receipts[1];
            assert_eq!(receipt.actions.len(), 1);
            assert_eq!(receipt.receiver_id, env::current_account_id());
            match &receipt.actions[0] {
                Action::FunctionCall(action) => {
                    assert_eq!(action.method_name, "on_deploy");
                    let args: OnDeployArgs = serde_json::from_str(&action.args).unwrap();
                    assert_eq!(args.account_id, env::predecessor_account_id());
                    assert_eq!(args.deposit, env::attached_deposit().into());
                }
                _ => panic!("expected FunctionCall"),
            }
        }
    }
}

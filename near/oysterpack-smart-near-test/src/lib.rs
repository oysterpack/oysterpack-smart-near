use near_sdk::{
    env,
    serde::{Deserialize, Serialize},
    serde_json,
    test_utils::{get_created_receipts, VMContextBuilder},
    AccountId, Balance, Gas, PromiseResult, PublicKey, VMContext,
};
use oysterpack_smart_near::YOCTO;

pub use near_sdk::{self, testing_env, MockedBlockchain};
pub use near_vm_logic;
pub use oysterpack_smart_near::to_valid_account_id;

pub const DEFAULT_CONTRACT_ACCOUNT_ID: &str = "contract.near";

pub const DEFAULT_CONTRACT_ACCOUNT_BALANCE: u128 = 10000 * YOCTO;

/// Creates a new NEAR test context.
/// - `predecessor_account_id` is also used as the `signer_account_id`
/// - `account_balance` is set to 10000 NEAR
pub fn new_context(predecessor_account_id: &str) -> VMContext {
    VMContextBuilder::new()
        .current_account_id(to_valid_account_id(&DEFAULT_CONTRACT_ACCOUNT_ID))
        .signer_account_id(to_valid_account_id(&predecessor_account_id))
        .predecessor_account_id(to_valid_account_id(&predecessor_account_id))
        .account_balance(DEFAULT_CONTRACT_ACCOUNT_BALANCE)
        .build()
}

/// Used to inject `PromiseResult`s into the NEAR runtime test environment. This enables callbacks
/// to be unit tested.
pub fn testing_env_with_promise_results(context: VMContext, promise_results: Vec<PromiseResult>) {
    assert!(
        !promise_results.is_empty(),
        "promise_results must not be empty"
    );
    let storage = env::take_blockchain_interface()
        .unwrap()
        .as_mut_mocked_blockchain()
        .unwrap()
        .take_storage();

    env::set_blockchain_interface(Box::new(MockedBlockchain::new(
        context,
        Default::default(),
        Default::default(),
        promise_results,
        storage,
        Default::default(),
        Default::default(),
    )));
}

/// injects a successful promise result into the NEAR runtime testing env
pub fn testing_env_with_promise_result_success(context: VMContext) {
    testing_env_with_promise_results(context, vec![PromiseResult::Successful(vec![0])]);
}

/// injects a failed promise result into the NEAR runtime testing env
pub fn testing_env_with_promise_result_failure(context: VMContext) {
    testing_env_with_promise_results(context, vec![PromiseResult::Failed]);
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct Receipt {
    pub receiver_id: String,
    pub receipt_indices: Vec<usize>,
    pub actions: Vec<Action>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(crate = "near_sdk::serde")]
pub enum Action {
    CreateAccount,
    DeployContract(DeployContractAction),
    FunctionCall(FunctionCallAction),
    Transfer(TransferAction),
    Stake(StakeAction),
    AddKeyWithFullAccess(AddKeyWithFullAccessAction),
    AddKeyWithFunctionCall(AddKeyWithFunctionCallAction),
    DeleteKey(DeleteKeyAction),
    DeleteAccount(DeleteAccountAction),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct DeployContractAction {
    pub code: Vec<u8>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct FunctionCallAction {
    pub method_name: String,
    pub args: String,
    pub gas: Gas,
    pub deposit: Balance,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct TransferAction {
    pub deposit: Balance,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct StakeAction {
    pub stake: Balance,
    public_key: PublicKey,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct AddKeyWithFullAccessAction {
    pub public_key: PublicKey,
    pub nonce: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct AddKeyWithFunctionCallAction {
    pub public_key: PublicKey,
    pub nonce: u64,
    pub allowance: Option<Balance>,
    pub receiver_id: AccountId,
    pub method_names: Vec<Vec<u8>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct DeleteKeyAction {
    pub public_key: PublicKey,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct DeleteAccountAction {
    pub beneficiary_id: AccountId,
}

pub fn deserialize_receipts() -> Vec<Receipt> {
    get_created_receipts()
        .iter()
        .map(|receipt| {
            let json = serde_json::to_string_pretty(receipt).unwrap();
            println!("{}", json);
            let receipt: Receipt = serde_json::from_str(&json).unwrap();
            receipt
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::test_env::bob;
    use near_sdk::{env, testing_env, MockedBlockchain};

    #[test]
    fn inject_promise_results() {
        // Arrange
        let context = new_context(&bob());
        testing_env!(context.clone());

        // Act
        testing_env_with_promise_results(
            context.clone(),
            vec![
                PromiseResult::Successful(vec![1, 2, 3]),
                PromiseResult::Failed,
            ],
        );

        // Assert
        assert_eq!(env::promise_results_count(), 2);
    }

    #[test]
    fn promise_result_success() {
        // Arrange
        let context = new_context(&bob());
        testing_env!(context.clone());

        // Act;
        testing_env_with_promise_result_success(context);

        // Assert
        assert_eq!(env::promise_results_count(), 1);
        match env::promise_result(0) {
            PromiseResult::Successful(_) => println!("promise result was success"),
            _ => panic!("expected PromiseResult::Successful"),
        }
    }

    #[test]
    fn promise_result_failure() {
        // Arrange
        let context = new_context(&bob());
        testing_env!(context.clone());

        // Act;
        testing_env_with_promise_result_failure(context);

        // Assert
        assert_eq!(env::promise_results_count(), 1);
        match env::promise_result(0) {
            PromiseResult::Failed => println!("promise result failed"),
            _ => panic!("expected PromiseResult::Failed"),
        }
    }
}

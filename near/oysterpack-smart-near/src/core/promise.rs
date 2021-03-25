use crate::domain::{Gas, YoctoNear};
use near_sdk::{borsh::BorshSerialize, env, serde::Serialize, serde_json, Promise};

pub fn borsh_function_call<Args>(
    account_id: &str,
    method: &str,
    args: Option<Args>,
    deposit: YoctoNear,
    gas: Gas,
) -> Promise
where
    Args: BorshSerialize,
{
    Promise::new(account_id.to_string()).function_call(
        method.as_bytes().to_vec(),
        args.map_or_else(|| vec![], |args| args.try_to_vec().unwrap()),
        *deposit,
        *gas,
    )
}

pub fn borsh_function_callback<Args>(
    method: &str,
    args: Option<Args>,
    deposit: YoctoNear,
    gas: Gas,
) -> Promise
where
    Args: BorshSerialize,
{
    borsh_function_call(&env::predecessor_account_id(), method, args, deposit, gas)
}

pub fn json_function_call<Args>(
    account_id: &str,
    method: &str,
    args: Option<Args>,
    deposit: YoctoNear,
    gas: Gas,
) -> Promise
where
    Args: Serialize,
{
    Promise::new(account_id.to_string()).function_call(
        method.as_bytes().to_vec(),
        args.map_or_else(|| vec![], |args| serde_json::to_vec(&args).unwrap()),
        *deposit,
        *gas,
    )
}

pub fn json_function_callback<Args>(
    method: &str,
    args: Option<Args>,
    deposit: YoctoNear,
    gas: Gas,
) -> Promise
where
    Args: Serialize,
{
    json_function_call(&env::predecessor_account_id(), method, args, deposit, gas)
}

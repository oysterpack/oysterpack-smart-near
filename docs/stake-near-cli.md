```shell
cd near/oysterpack-smart-stake
# set `CONTRACT_NAME` env var
. ./neardev/dev-account.env
echo $CONTRACT_NAME

ACCOUNT=oysterpack.testnet

# DELETE contract and reclaim funds
# near delete $CONTRACT_NAME oysterpack.testnet
```

## Account Storage Usage
```shell
near view $CONTRACT_NAME ops_storage_usage_bounds
near view $CONTRACT_NAME ops_storage_usage --args '{"account_id":"oysterpack.testnet"}'
```

## Storage Management
```shell
near view $CONTRACT_NAME storage_balance_bounds
near view $CONTRACT_NAME storage_balance_of --args '{"account_id":"oysterpack.testnet"}'

near call $CONTRACT_NAME storage_deposit --accountId alfio-zappala-oysterpack.testnet --amount 0.00393
near call $CONTRACT_NAME storage_deposit --args '{"account_id":"oysterpack-2.testnet", "registration_only":true}' --accountId oysterpack.testnet --amount 1
near call $CONTRACT_NAME storage_deposit --args '{"registration_only":true}' --accountId oysterpack-2.testnet --amount 1

near call $CONTRACT_NAME storage_withdraw --accountId oysterpack-2.testnet --amount 0.000000000000000000000001
near call $CONTRACT_NAME storage_withdraw --accountId oysterpack-2.testnet --args '{"amount":"1000"}' --amount 0.000000000000000000000001

near call $CONTRACT_NAME storage_unregister --accountId oysterpack-2.testnet --amount 0.000000000000000000000001
near call $CONTRACT_NAME storage_unregister --args '{"force":true}' --accountId oysterpack-2.testnet --amount 0.000000000000000000000001
```

## Permissions Management
```shell
near view $CONTRACT_NAME ops_permissions_is_admin --args '{"account_id":"oysterpack.testnet"}'
near view $CONTRACT_NAME ops_permissions_is_operator --args '{"account_id":"oysterpack.testnet"}'
near view $CONTRACT_NAME ops_permissions --args '{"account_id":"oysterpack.testnet"}'
near view $CONTRACT_NAME ops_permissions_granted --args '{"account_id":"oysterpack.testnet"}'

near call $CONTRACT_NAME ops_permissions_grant_admin --args '{"account_id":"oysterpack-2.testnet"}' --accountId oysterpack.testnet
near call $CONTRACT_NAME ops_permissions_grant_operator --args '{"account_id":"oysterpack-2.testnet"}' --accountId oysterpack.testnet
near call $CONTRACT_NAME ops_permissions_grant_permissions --args '{"account_id":"oysterpack-2.testnet", "permissions": [0]}' --accountId oysterpack.testnet

near call $CONTRACT_NAME ops_permissions_revoke_admin --args '{"account_id":"oysterpack-2.testnet"}' --accountId oysterpack.testnet
near call $CONTRACT_NAME ops_permissions_revoke_operator --args '{"account_id":"oysterpack-2.testnet"}' --accountId oysterpack.testnet
near call $CONTRACT_NAME ops_permissions_revoke_all --args '{"account_id":"oysterpack-2.testnet"}' --accountId oysterpack.testnet
near call $CONTRACT_NAME ops_permissions_revoke_permissions --args '{"account_id":"oysterpack-2.testnet", "permissions": [0]}' --accountId oysterpack.testnet

near view $CONTRACT_NAME ops_permissions_contract_permissions
```

## Contract Metrics
```shell
near view $CONTRACT_NAME ops_metrics
near view $CONTRACT_NAME ops_metrics_near_balances
near view $CONTRACT_NAME ops_metrics_accounts
near view $CONTRACT_NAME ops_metrics_total_registered_accounts
near view $CONTRACT_NAME ops_metrics_contract_storage_usage
near view $CONTRACT_NAME ops_metrics_storage_usage_costs
```

## Fungible Token
```shell
near view $CONTRACT_NAME ft_total_supply

near view $CONTRACT_NAME ft_balance_of --args '{"account_id":"oysterpack.testnet"}'
near view $CONTRACT_NAME ft_balance_of --args '{"account_id":"oysterpack-2.testnet"}'

near call $CONTRACT_NAME ft_transfer --args '{"receiver_id":"dev-1618770943926-8326158","amount":"1000000000000000000000000000"}' --accountId oysterpack.testnet --amount 0.000000000000000000000001

near call $CONTRACT_NAME ft_transfer_call --args '{"receiver_id":"dev-1618770943926-8326158","amount":"1000000000000000000000000000","msg":""}' --accountId oysterpack.testnet --amount 0.000000000000000000000001
```

## Fungible Token Operator
```shell
near view $CONTRACT_NAME ft_operator_transfer_callback_gas

near call $CONTRACT_NAME ft_operator_command --accountId oysterpack.testnet --args '{"command":OperatorCommand}'
pub enum OperatorCommand {
    SetIcon(Icon),
    ClearIcon,
    SetReference(Reference, Hash),
    ClearReference,
    SetTransferCallbackGas(TGas),
}
```

### Fungible Token Metadata
```shell
near view $CONTRACT_NAME ft_metadata
```

## Staking Pool
```shell
near view $CONTRACT_NAME ops_stake_status
near view $CONTRACT_NAME ops_stake_pool_balances
near view $CONTRACT_NAME ops_stake_fee
near view $CONTRACT_NAME ops_stake_public_key
near view $CONTRACT_NAME ops_stake_token_value

near view $CONTRACT_NAME ops_stake_balance --args '{"account_id":"alfio-zappala-oysterpack.testnet"}'

near call $CONTRACT_NAME ops_stake --accountId oysterpack.testnet
near call $CONTRACT_NAME ops_stake --accountId alfio-zappala-oysterpack.testnet --amount 0.1
near call $CONTRACT_NAME ops_stake --accountId oysterpack.testnet --amount 1

near call $CONTRACT_NAME ops_unstake --accountId alfio-zappala-oysterpack.testnet --args '{"amount":"1000000000000000000000000"}'

near call $CONTRACT_NAME ops_restake --accountId alfio-zappala-oysterpack.testnet

near call $CONTRACT_NAME ops_stake_withdraw --accountId alfio-zappala-oysterpack.testnet 

near call $CONTRACT_NAME ops_stake_transfer --accountId oysterpack.testnet --args '{"receiver_id":"alfio-zappala-oysterpack.testnet","amount":"1000000000000000000000000"}' --amount 0.000000000000000000000001
```

### Staking Pool Operator
```shell
near call $CONTRACT_NAME ops_stake_operator_command --args '{"command":"StartStaking"}' --accountId oysterpack.testnet

near call $CONTRACT_NAME ops_stake_operator_command --args '{"command":"StopStaking"}' --accountId oysterpack.testnet

near call $CONTRACT_NAME ops_stake_operator_command --args '{"command":{"UpdateFees":{"staking_fee":1,"earnings_fee":50}}}' --accountId $oysterpack.testnet
```

## Staking Pool Treasury
```shell
near call $CONTRACT_NAME ops_stake_treasury_deposit --accountId oysterpack.testnet --amount 10

near call $CONTRACT_NAME ops_stake_treasury_distribution --accountId oysterpack.testnet --amount 10

near call $CONTRACT_NAME ops_stake_treasury_transfer_to_owner --accountId oysterpack.testnet --args '{"amount":"1000000000000000000000000"}'
```

### STAKE Pool Factory
```shell
near call $CONTRACT_NAME deploy --accountId oysterpack.testnet --amount 6 --gas 300000000000000 --args \
'{"stake_symbol":"PEARL","stake_public_key":"ed25519:GTi3gtSio5ZYYKTT8WVovqJEob6KqdmkTi8KqGSfwqdm","earnings_fee":50,"staking_fee":1}'
```

#   1000000000000000000000000     - 1 NEAR
# 0.0003930000000000000000000

# 1000000000000                   - 1 TGas
# 
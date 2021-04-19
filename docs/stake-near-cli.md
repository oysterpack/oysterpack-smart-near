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
near view $CONTRACT_NAME ops_storage_usage --args '{"account_id":"alfio-zappala-oysterpack.testnet"}'
```

## Storage Management
```shell
near view $CONTRACT_NAME storage_balance_bounds

near view $CONTRACT_NAME storage_balance_of --args '{"account_id":"oysterpack.testnet"}'

near call $CONTRACT_NAME storage_deposit --accountId alfio-zappala-oysterpack.testnet --amount 0.00393
near call $CONTRACT_NAME storage_deposit --args '{"account_id":"oysterpack-2.testnet", "registration_only":true}' --accountId oysterpack.testnet --amount 1
near call $CONTRACT_NAME storage_deposit --args '{"registration_only":true}' --accountId oysterpack-2.testnet --amount 1

near call $CONTRACT_NAME storage_deposit --accountId oysterpack-2.testnet --amount 1

near call $CONTRACT_NAME storage_withdraw --args 

near call $CONTRACT_NAME storage_unregister --args --accountId alfio-zappala-oysterpack.testnet --amount 0.000000000000000000000001
near call $CONTRACT_NAME storage_unregister --args --accountId oysterpack-2.testnet --amount 0.000000000000000000000001
```

## Access Control
```shell
near view $CONTRACT_NAME ops_permissions_is_admin --args '{"account_id":"oysterpack.testnet"}'
near view $CONTRACT_NAME ops_permissions_is_operator --args '{"account_id":"oysterpack.testnet"}'
near view $CONTRACT_NAME ops_permissions --args '{"account_id":"oysterpack.testnet"}'
near view $CONTRACT_NAME ops_permissions_granted --args '{"account_id":"oysterpack.testnet"}'

near call $CONTRACT_NAME ops_permissions_grant_admin --args '{"account_id":"oysterpack-2.testnet"}' --accountId oysterpack.testnet
near call $CONTRACT_NAME ops_permissions_grant_operator --args '{"account_id":"oysterpack-2.testnet"}' --accountId oysterpack.testnet

near call $CONTRACT_NAME ops_permissions_revoke_all --args '{"account_id":"oysterpack-2.testnet"}' --accountId oysterpack.testnet

near call $CONTRACT_NAME ops_permissions_grant_permissions --args '{"account_id":"oysterpack-2.testnet", "permissions": [0]}' --accountId oysterpack.testnet

near view $CONTRACT_NAME ops_permissions_contract_permissions
```

## Contract Metrics
```shell
near view $CONTRACT_NAME ops_metrics_near_balances
near view $CONTRACT_NAME  ops_metrics_accounts
```

## Staking Pool
```shell
near view $CONTRACT_NAME ops_stake_status
near view $CONTRACT_NAME ops_stake_pool_balances
near view $CONTRACT_NAME ops_stake_fee
near view $CONTRACT_NAME ops_stake_public_key
near view $CONTRACT_NAME ops_stake_token_value

near view $CONTRACT_NAME ops_stake_balance --args '{"account_id":"oysterpack.testnet"}'

near call $CONTRACT_NAME ops_stake --accountId oysterpack.testnet
near call $CONTRACT_NAME ops_stake --accountId alfio-zappala-oysterpack.testnet
near call $CONTRACT_NAME ops_stake --accountId oysterpack.testnet --amount 350000

near call $CONTRACT_NAME ops_unstake --accountId oysterpack.testnet --args '{"amount":"1000000000000000000000000"}'

near call $CONTRACT_NAME ops_restake --accountId oysterpack.testnet
```

### Staking Pool Operator
```shell
near call $CONTRACT_NAME ops_stake_operator_command --args '{"command":"StartStaking"}' --accountId oysterpack.testnet

near call $CONTRACT_NAME ops_stake_operator_command --args '{"command":"StopStaking"}' --accountId oysterpack.testnet
```

#      1000000000000000000000000 - 1 NEAR
#               1964010147207714
#        79999999723157851582977
         80000000000000000000000
#         1200892249084800000000
#          480786260264800000000
#          687403551522700000000

#           1000000000000000000 - 1 TGas

932763604497300000000
1200892249084800000000

350209991292164780970499999999
    350209991710169784696199999999

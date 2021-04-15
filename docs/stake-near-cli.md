```shell
cd near/oysterpack-smart-stake
# set `CONTRACT_NAME` env var
. ./neardev/dev-account.env
echo $CONTRACT_NAME

ACCOUNT=oysterpack.testnet
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

near call $CONTRACT_NAME storage_deposit --args --accountId alfio-zappala-oysterpack.testnet --amount 0.00393

near call $CONTRACT_NAME storage_unregister --args --accountId alfio-zappala-oysterpack.testnet --amount 0.000000000000000000000001

```
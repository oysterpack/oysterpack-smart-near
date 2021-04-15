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
```

## Storage Management
```shell
near call $CONTRACT_NAME storage_deposit --args

```
- https://wallet.openshards.io/
- https://explorer.guildnet.near.org/nodes/online-nodes
- https://near-guildnet.github.io/open-shards-faucet/

## How to configure NEAR CLI to point to GuildNet
```shell
near --nodeUrl https://rpc.openshards.io/ --walletUrl https://wallet.openshards.io/ --networkId guildnet

alias near-guildnet='near --nodeUrl https://rpc.openshards.io/ --walletUrl https://wallet.openshards.io/ --networkId guildnet'
```

## OysterPack STAKE Pool - Basic APIs to Get Started
```shell
# register account
CONTRACT=pearl.stake-v1.oysterpack.guildnet
ACCOUNT=oysterpack.guildnet

# Storage Management APIs - used to register the account with the STAKE pool contract
# - see https://nomicon.io/Standards/StorageManagement.html for more info

# NOTE: all amounts specified in --args are specified in yocto units:
# 1 NEAR = 1000000000000000000000000 yoctoNEAR

# registers the account - actual cost is 0.00393 NEAR and rest will be refunded 
near-guildnet call $CONTRACT storage_deposit --accountId $ACCOUNT --args '{"registration_only":true}' --amount 1

# stakes the attached deposit + any account storage available balance
near-guildnet call $CONTRACT ops_stake --accountId $ACCOUNT --amount 1

# unstakes specified amount
near-guildnet call $CONTRACT ops_unstake --accountId $ACCOUNT --args '{"amount":"1000000000000000000000000"}'
# unstakes all
near-guildnet call $CONTRACT ops_unstake --accountId $ACCOUNT

# withdraws specified amount from unstaked available balance
near-guildnet call $CONTRACT ops_stake_withdraw --accountId $ACCOUNT --args '{"amount":"1000000000000000000000000"}'
# withdraws all unstaked available balance
near-guildnet call $CONTRACT ops_stake_withdraw --accountId $ACCOUNT

# used to check the pool status: Online/Offline
near-guildnet view $CONTRACT ops_stake_status
# returns balances that the pool manages
near-guildnet view $CONTRACT ops_stake_pool_balances
near-guildnet view $CONTRACT ops_stake_fees
near-guildnet view $CONTRACT ops_stake_public_key
# returns the current NEAR value for 1 STAKE
near-guildnet view $CONTRACT ops_stake_token_value 
# returns the current NEAR value for the specified amount of STAKE
near-guildnet view $CONTRACT ops_stake_token_value --args '{"amount":"5000000000000000000000000"}'
```
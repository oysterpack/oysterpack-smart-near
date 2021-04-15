## Running a Validator Node on testnet
- https://docs.near.org/docs/validator/staking
```shell
STAKING_POOL=dev-1618485186536-2839500
NEARCORE=~/Documents/projects/github/near/nearcore

nearup run testnet --binary-path $NEARCORE/target/release --account-id $STAKING_POOL
```

## Running a Validator Node on mainnet
- https://docs.near.org/docs/validator/deploy-on-mainnet
```shell
git clone https://github.com/near/nearcore.git
export NEAR_RELEASE_VERSION=$(curl -s https://github.com/near/nearcore/releases/latest | tr '/" ' '\n' | grep "[0-9]\.[0-9]*\.[0-9]" | head -n 1)
cd nearcore
git checkout $NEAR_RELEASE_VERSION
cargo build -p neard --release

target/release/neard init --chain-id="mainnet" --account-id=<YOUR_STAKING_POOL_ID>
target/release/neard runn
```

## Initializing the STAKE contract
```shell
cd near/oysterpack-smart-stake
# set `CONTRACT_NAME` env var
. ./neardev/dev-account.env
echo $CONTRACT_NAME
near call $CONTRACT_NAME deploy --accountId oysterpack.testnet --args '{"stake_public_key":"ed25519:GTi3gtSio5ZYYKTT8WVovqJEob6KqdmkTi8KqGSfwqdm"}'
```
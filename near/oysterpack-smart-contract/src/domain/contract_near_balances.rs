use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
};
use oysterpack_smart_near::domain::{YoctoNear, ZERO_NEAR};
use std::collections::HashMap;

#[derive(
    BorshSerialize,
    BorshDeserialize,
    Deserialize,
    Serialize,
    Clone,
    Debug,
    PartialOrd,
    PartialEq,
    Eq,
    Hash,
    Default,
)]
#[serde(crate = "near_sdk::serde")]
pub struct BalanceId(pub u8);

/// used to track NEAR balances that are outside registered accounts - examples
/// - liquidity
/// - profit sharing fund
pub type NearBalances = HashMap<BalanceId, YoctoNear>;

#[derive(
    BorshSerialize, BorshDeserialize, Deserialize, Serialize, Clone, Debug, PartialEq, Default,
)]
#[serde(crate = "near_sdk::serde")]
pub struct ContractNearBalances {
    total: YoctoNear,
    accounts: YoctoNear,
    balances: Option<NearBalances>,
    owner: YoctoNear,
}

impl ContractNearBalances {
    pub fn new(total: YoctoNear, accounts: YoctoNear, balances: Option<NearBalances>) -> Self {
        let owner = total
            - accounts
            - balances.as_ref().map_or(ZERO_NEAR, |balances| {
                balances
                    .values()
                    .map(|balance| balance.value())
                    .sum::<u128>()
                    .into()
            });
        Self {
            total,
            accounts,
            balances,
            owner,
        }
    }

    pub fn total(&self) -> YoctoNear {
        self.total
    }

    pub fn accounts(&self) -> YoctoNear {
        self.accounts
    }

    /// NEAR balances that are not owned by registered accounts and not by the contract owner, e.g.,
    /// - liquidity pools
    /// - batched funds, e.g., STAKE batches
    /// - profit sharing funds
    pub fn balances(&self) -> Option<NearBalances> {
        self.balances.as_ref().map(|balances| balances.clone())
    }

    /// returns portion of total contract NEAR balance that is owned by the contract owner, which is
    /// computed as: `total - accounts - balances`
    pub fn owner(&self) -> YoctoNear {
        self.owner
    }
}

use oysterpack_smart_near::near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env,
    serde::{Deserialize, Serialize},
};
use oysterpack_smart_near::{data::Object, domain::YoctoNear};
use std::{collections::HashMap, ops::Deref};

/// Balance ID is used to track separate NEAR balances
/// - use ULID to generate unique IDs to avoid collisions between components
#[derive(
    BorshSerialize,
    BorshDeserialize,
    Deserialize,
    Serialize,
    Clone,
    Copy,
    Debug,
    PartialOrd,
    PartialEq,
    Eq,
    Hash,
    Default,
)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub struct BalanceId(pub u128);

/// used to track NEAR balances that are outside registered accounts - examples
/// - liquidity
/// - profit sharing fund
pub type NearBalances = HashMap<BalanceId, YoctoNear>;

/// Provides a breakdown of the contract's NEAR balances
#[derive(
    BorshSerialize, BorshDeserialize, Deserialize, Serialize, Clone, Debug, PartialEq, Default,
)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub struct ContractNearBalances {
    total: YoctoNear,
    accounts: YoctoNear,
    balances: Option<NearBalances>,
    owner: YoctoNear,
    locked: YoctoNear,
}

impl ContractNearBalances {
    pub fn new(accounts: YoctoNear, balances: Option<NearBalances>) -> Self {
        let locked: YoctoNear = env::account_locked_balance().into();
        let total: YoctoNear = locked + env::account_balance();

        let total_contract_near_balances: YoctoNear =
            balances.as_ref().map_or(YoctoNear::ZERO, |balances| {
                balances
                    .values()
                    .map(|balance| balance.value())
                    .sum::<u128>()
                    .into()
            });

        // let owner = (total - locked)
        //     .saturating_sub(*accounts)
        //     .saturating_sub(*total_contract_near_balances)
        //     .into();
        let owner = total - locked - accounts - total_contract_near_balances;

        Self {
            total,
            accounts,
            balances,
            owner,
            locked,
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
    /// computed as: `total - locked - accounts - balances`
    pub fn owner(&self) -> YoctoNear {
        self.owner
    }

    /// balance locked for validator staking
    pub fn locked(&self) -> YoctoNear {
        self.locked
    }
}

const NEAR_BALANCES_KEY: u128 = 1953121181530803691069739592144632957;

type DAO = Object<u128, NearBalances>;

impl ContractNearBalances {
    pub fn load_near_balances() -> NearBalances {
        DAO::load(&NEAR_BALANCES_KEY)
            .map_or_else(NearBalances::new, |object| object.deref().clone())
    }

    pub fn near_balance(id: BalanceId) -> YoctoNear {
        DAO::load(&NEAR_BALANCES_KEY).map_or(YoctoNear::ZERO, |object| {
            object.get(&id).cloned().unwrap_or(YoctoNear::ZERO)
        })
    }

    /// Increments the balance by the specified amount and returns the updated balance
    pub fn incr_balance(id: BalanceId, amount: YoctoNear) -> YoctoNear {
        let mut balances = DAO::load(&NEAR_BALANCES_KEY)
            .unwrap_or_else(|| DAO::new(NEAR_BALANCES_KEY, NearBalances::new()));
        let mut balance = balances.get(&id).cloned().unwrap_or(YoctoNear::ZERO);
        balance += amount;
        balances.insert(id, balance);
        balances.save();
        balance
    }

    /// Decrements the balance by the specified amount and returns the updated balance
    pub fn decr_balance(id: BalanceId, amount: YoctoNear) -> YoctoNear {
        let mut balances = DAO::load(&NEAR_BALANCES_KEY)
            .unwrap_or_else(|| DAO::new(NEAR_BALANCES_KEY, NearBalances::new()));
        let mut balance = balances.get(&id).cloned().unwrap_or(YoctoNear::ZERO);
        balance -= amount;
        if balance == YoctoNear::ZERO {
            balances.remove(&id);
        } else {
            balances.insert(id, balance);
        }
        balances.save();
        balance
    }

    /// Sets the balance to the specified amount and returns the updated balance
    pub fn set_balance(id: BalanceId, amount: YoctoNear) {
        let mut balances = DAO::load(&NEAR_BALANCES_KEY)
            .unwrap_or_else(|| DAO::new(NEAR_BALANCES_KEY, NearBalances::new()));
        if amount == YoctoNear::ZERO {
            balances.remove(&id);
        } else {
            balances.insert(id, amount);
        }
        balances.save();
    }

    /// Clears the balance and removes the record from storage
    pub fn clear_balance(id: BalanceId) {
        let mut balances = DAO::load(&NEAR_BALANCES_KEY)
            .unwrap_or_else(|| DAO::new(NEAR_BALANCES_KEY, NearBalances::new()));
        balances.remove(&id);
        balances.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oysterpack_smart_near::near_sdk::test_utils::test_env;
    use oysterpack_smart_near::YOCTO;

    const LIQUIDITY_BALANCE_ID: BalanceId = BalanceId(0);
    const EARNINGS_BALANCE_ID: BalanceId = BalanceId(1);

    #[test]
    fn contract_near_balances() {
        // Arrange
        test_env::setup();

        let balances = ContractNearBalances::load_near_balances();
        assert!(balances.is_empty());

        // Act - increment balances
        ContractNearBalances::incr_balance(LIQUIDITY_BALANCE_ID, YOCTO.into());
        ContractNearBalances::incr_balance(LIQUIDITY_BALANCE_ID, YOCTO.into());
        ContractNearBalances::incr_balance(LIQUIDITY_BALANCE_ID, YOCTO.into());
        ContractNearBalances::incr_balance(EARNINGS_BALANCE_ID, (2 * YOCTO).into());

        // Assert
        let balances = ContractNearBalances::load_near_balances();
        assert_eq!(balances.len(), 2);
        assert_eq!(
            balances.get(&LIQUIDITY_BALANCE_ID).unwrap().value(),
            3 * YOCTO
        );
        assert_eq!(
            balances.get(&EARNINGS_BALANCE_ID).unwrap().value(),
            2 * YOCTO
        );

        // Act - decrement balances
        ContractNearBalances::decr_balance(LIQUIDITY_BALANCE_ID, YOCTO.into());
        ContractNearBalances::decr_balance(EARNINGS_BALANCE_ID, YOCTO.into());

        // Assert
        let balances = ContractNearBalances::load_near_balances();
        assert_eq!(balances.len(), 2);
        assert_eq!(
            balances.get(&LIQUIDITY_BALANCE_ID).unwrap().value(),
            2 * YOCTO
        );
        assert_eq!(balances.get(&EARNINGS_BALANCE_ID).unwrap().value(), YOCTO);

        // Act - decrement balances
        ContractNearBalances::set_balance(LIQUIDITY_BALANCE_ID, (10 * YOCTO).into());
        ContractNearBalances::set_balance(EARNINGS_BALANCE_ID, (20 * YOCTO).into());

        // Assert
        let balances = ContractNearBalances::load_near_balances();
        assert_eq!(balances.len(), 2);
        assert_eq!(
            balances.get(&LIQUIDITY_BALANCE_ID).unwrap().value(),
            10 * YOCTO
        );
        assert_eq!(
            balances.get(&EARNINGS_BALANCE_ID).unwrap().value(),
            20 * YOCTO
        );

        // Act - decrement balances
        ContractNearBalances::clear_balance(LIQUIDITY_BALANCE_ID);

        // Assert
        let balances = ContractNearBalances::load_near_balances();
        assert_eq!(balances.len(), 1);
        assert!(!balances.contains_key(&LIQUIDITY_BALANCE_ID));
        assert_eq!(
            balances.get(&EARNINGS_BALANCE_ID).unwrap().value(),
            20 * YOCTO
        );
    }
}

use crate::components::staking_pool::State;
use oysterpack_smart_near::asserts::ERR_INSUFFICIENT_FUNDS;
use oysterpack_smart_near::domain::{EpochHeight, YoctoNear};
use oysterpack_smart_near::near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env,
};
use std::cmp::Ordering;
use std::collections::BTreeMap;

/// unstaked NEAR is locked for 4 epochs before being able to be withdrawn
/// https://github.com/near/nearcore/blob/037954e087fd5c8a65598ede502495530c73f835/chain/epoch_manager/src/lib.rs#L815
const EPOCHS_LOCKED: usize = 4;

#[derive(BorshDeserialize, BorshSerialize, Debug, Clone, Copy, PartialEq, Default)]
pub struct UnstakedBalances {
    available: YoctoNear,
    locked: [(EpochHeight, YoctoNear); EPOCHS_LOCKED],
}

impl UnstakedBalances {
    pub fn total(&self) -> YoctoNear {
        self.available + self.locked_balance()
    }

    pub fn available(&self) -> YoctoNear {
        self.available
    }

    pub fn locked_balance(&self) -> YoctoNear {
        self.locked
            .iter()
            .fold(YoctoNear::ZERO, |total, (_, amount)| total + *amount)
    }

    pub fn locked(&self) -> Option<BTreeMap<EpochHeight, YoctoNear>> {
        let result: BTreeMap<EpochHeight, YoctoNear> =
            self.locked
                .iter()
                .fold(BTreeMap::new(), |mut result, (epoch, amount)| {
                    if *amount > YoctoNear::ZERO {
                        result.insert(*epoch, *amount);
                    }
                    result
                });
        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    pub(crate) fn unlock(&mut self) {
        let current_epoch: EpochHeight = env::epoch_height().into();

        for i in 0..EPOCHS_LOCKED {
            let (epoch, balance) = self.locked[i];
            if balance > YoctoNear::ZERO {
                if epoch <= current_epoch {
                    self.available += balance;
                    self.locked[i] = Default::default();
                }
            }
        }
    }

    /// If there are locked balances then try to use liquidity to unlock the funds for withdrawal.
    ///
    /// returns the amount of liquidity that was applied
    pub(crate) fn apply_liquidity(&mut self) -> YoctoNear {
        self.unlock();
        let locked_balance = self.locked_balance();
        if locked_balance == YoctoNear::ZERO {
            return YoctoNear::ZERO;
        }

        let liquidity = State::liquidity();
        if liquidity == YoctoNear::ZERO {
            return YoctoNear::ZERO;
        }

        if liquidity >= locked_balance {
            self.available = self.total();
            for i in 0..EPOCHS_LOCKED {
                self.locked[i] = Default::default();
            }
            return locked_balance;
        }

        self.available += liquidity;
        self.debit_from_locked(liquidity);
        liquidity
    }

    /// adds the unstaked balance and locks it up for 4 epochs
    pub(crate) fn credit_unstaked(&mut self, amount: YoctoNear) {
        self.unlock();
        let available_on: EpochHeight = (env::epoch_height() + EPOCHS_LOCKED as u64).into();
        for i in 0..EPOCHS_LOCKED {
            let (epoch, balance) = self.locked[i];
            if balance > YoctoNear::ZERO {
                if epoch == available_on {
                    self.locked[i] = (epoch, balance + amount);
                    return;
                }
            }
        }

        for i in 0..EPOCHS_LOCKED {
            if self.locked[i].1 == YoctoNear::ZERO {
                self.locked[i] = (available_on, amount);
                return;
            }
        }

        unreachable!()
    }

    fn sort_locked(&mut self) {
        self.locked.sort_by(|left, right| {
            if left.1 == YoctoNear::ZERO && right.1 == YoctoNear::ZERO {
                return Ordering::Equal;
            }
            if left.1 > YoctoNear::ZERO && right.1 == YoctoNear::ZERO {
                return Ordering::Less;
            }
            if left.1 == YoctoNear::ZERO && right.1 > YoctoNear::ZERO {
                return Ordering::Greater;
            }
            left.0.cmp(&right.0)
        });
    }

    /// tries to debit the specified amount from the available balance.
    ///
    /// ## NOTES
    /// - locked balances are checked if they have become available
    /// - if there are locked balances, then liquidity will be applied
    ///
    /// ## Panics
    /// if there are insufficient funds
    pub(crate) fn debit_available_balance(&mut self, amount: YoctoNear) {
        self.apply_liquidity();
        ERR_INSUFFICIENT_FUNDS.assert(|| self.available >= amount);
        self.available -= amount;
    }

    pub(crate) fn debit_for_restaking(&mut self, amount: YoctoNear) {
        let total = self.total();
        ERR_INSUFFICIENT_FUNDS.assert(|| total >= amount);

        if total == self.available {
            self.available -= amount;
            return;
        }

        let remainder = self.debit_from_locked(amount);
        self.available -= remainder;
    }

    fn debit_from_locked(&mut self, mut amount: YoctoNear) -> YoctoNear {
        self.sort_locked();

        // take from the most recent unstaked balances
        for i in (0..EPOCHS_LOCKED).rev() {
            let (available_on, unstaked) = self.locked[i];
            if unstaked > YoctoNear::ZERO {
                if unstaked <= amount {
                    amount -= unstaked;
                    self.locked[i] = Default::default();
                } else {
                    self.locked[i] = (available_on, unstaked - amount);
                    return YoctoNear::ZERO;
                }

                if amount == YoctoNear::ZERO {
                    return YoctoNear::ZERO;
                }
            }
        }

        return amount;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oysterpack_smart_near::domain::YoctoNear;
    use oysterpack_smart_near::YOCTO;
    use oysterpack_smart_near_test::*;

    #[test]
    fn unlock() {
        let mut ctx = new_context("bob");

        ctx.epoch_height = 100;
        testing_env!(ctx.clone());

        let mut unstaked_balances = UnstakedBalances::default();
        unstaked_balances.credit_unstaked(YOCTO.into());
        unstaked_balances.credit_unstaked(YOCTO.into());

        assert_eq!(unstaked_balances.available, YoctoNear::ZERO);
        let expected_available_on: EpochHeight = (ctx.epoch_height + EPOCHS_LOCKED as u64).into();
        assert_eq!(
            *unstaked_balances
                .locked()
                .unwrap()
                .get(&expected_available_on)
                .unwrap(),
            YoctoNear(2 * YOCTO)
        );
        assert_eq!(*unstaked_balances.total(), 2 * YOCTO);

        ctx.epoch_height = 101;
        testing_env!(ctx.clone());
        unstaked_balances.credit_unstaked(YOCTO.into());
        assert_eq!(unstaked_balances.available, YoctoNear::ZERO);
        assert_eq!(
            *unstaked_balances
                .locked()
                .unwrap()
                .get(&expected_available_on)
                .unwrap(),
            YoctoNear(2 * YOCTO)
        );
        let expected_available_on: EpochHeight = (ctx.epoch_height + EPOCHS_LOCKED as u64).into();
        assert_eq!(
            *unstaked_balances
                .locked()
                .unwrap()
                .get(&expected_available_on)
                .unwrap(),
            YoctoNear(YOCTO)
        );
        assert_eq!(*unstaked_balances.total(), 3 * YOCTO);

        ctx.epoch_height = 104;
        testing_env!(ctx.clone());
        unstaked_balances.unlock();
        assert_eq!(*unstaked_balances.total(), 3 * YOCTO);
        assert_eq!(*unstaked_balances.available(), 2 * YOCTO);

        ctx.epoch_height = 105;
        testing_env!(ctx.clone());
        unstaked_balances.unlock();
        assert_eq!(*unstaked_balances.total(), 3 * YOCTO);
        assert_eq!(*unstaked_balances.available(), 3 * YOCTO);

        println!("{:?}", unstaked_balances);
    }

    #[test]
    fn debit_for_restaking() {
        let mut ctx = new_context("bob");

        ctx.epoch_height = 100;
        testing_env!(ctx.clone());
        let mut unstaked_balances = UnstakedBalances::default();
        unstaked_balances.credit_unstaked(YOCTO.into());

        ctx.epoch_height = 101;
        testing_env!(ctx.clone());
        unstaked_balances.credit_unstaked(YOCTO.into());

        ctx.epoch_height = 103;
        testing_env!(ctx.clone());
        unstaked_balances.credit_unstaked(YOCTO.into());

        ctx.epoch_height = 104;
        testing_env!(ctx.clone());
        unstaked_balances.credit_unstaked(YOCTO.into());

        ctx.epoch_height = 106;
        testing_env!(ctx.clone());
        unstaked_balances.credit_unstaked(YOCTO.into());

        unstaked_balances.sort_locked();

        assert_eq!(
            unstaked_balances,
            UnstakedBalances {
                available: (2 * YOCTO).into(),
                locked: [
                    (107.into(), YOCTO.into()),
                    (108.into(), YOCTO.into()),
                    (110.into(), YOCTO.into()),
                    Default::default()
                ]
            }
        );

        unstaked_balances.debit_for_restaking(1000.into());
        assert_eq!(
            unstaked_balances,
            UnstakedBalances {
                available: (2 * YOCTO).into(),
                locked: [
                    (107.into(), YOCTO.into()),
                    (108.into(), YOCTO.into()),
                    (110.into(), (YOCTO - 1000).into()),
                    Default::default()
                ]
            }
        );

        unstaked_balances.debit_for_restaking(YOCTO.into());
        assert_eq!(
            unstaked_balances,
            UnstakedBalances {
                available: (2 * YOCTO).into(),
                locked: [
                    (107.into(), YOCTO.into()),
                    (108.into(), (YOCTO - 1000).into()),
                    Default::default(),
                    Default::default()
                ]
            }
        );

        unstaked_balances.debit_for_restaking((2 * YOCTO).into());
        assert_eq!(
            unstaked_balances,
            UnstakedBalances {
                available: ((2 * YOCTO) - 1000).into(),
                locked: Default::default()
            }
        );

        unstaked_balances.debit_for_restaking(YOCTO.into());
        assert_eq!(
            unstaked_balances,
            UnstakedBalances {
                available: (YOCTO - 1000).into(),
                locked: Default::default()
            }
        );
    }

    #[test]
    #[should_panic(expected = "[ERR] [INSUFFICIENT_FUNDS]")]
    fn debit_for_restaking_with_insufficient_funds() {
        let mut ctx = new_context("bob");

        ctx.epoch_height = 100;
        testing_env!(ctx.clone());
        let mut unstaked_balances = UnstakedBalances::default();
        unstaked_balances.credit_unstaked(YOCTO.into());

        unstaked_balances.debit_for_restaking((2 * YOCTO).into());
    }

    #[test]
    fn debit_available_balance() {
        let mut ctx = new_context("bob");

        ctx.epoch_height = 100;
        testing_env!(ctx.clone());
        let mut unstaked_balances = UnstakedBalances::default();
        unstaked_balances.credit_unstaked(YOCTO.into());
        assert_eq!(unstaked_balances.total(), YOCTO.into());

        ctx.epoch_height = 104;
        testing_env!(ctx.clone());
        unstaked_balances.debit_available_balance(YOCTO.into());
        assert_eq!(unstaked_balances.total(), YoctoNear::ZERO);
    }

    #[test]
    #[should_panic(expected = "[ERR] [INSUFFICIENT_FUNDS]")]
    fn debit_available_balance_insufficient_funds() {
        let mut ctx = new_context("bob");

        ctx.epoch_height = 100;
        testing_env!(ctx.clone());
        let mut unstaked_balances = UnstakedBalances::default();
        unstaked_balances.credit_unstaked(YOCTO.into());
        assert_eq!(unstaked_balances.total(), YOCTO.into());

        unstaked_balances.debit_available_balance(YOCTO.into());
    }
}

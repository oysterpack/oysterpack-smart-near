use oysterpack_smart_near::asserts::ERR_INSUFFICIENT_FUNDS;
use oysterpack_smart_near::domain::{EpochHeight, YoctoNear, ZERO_NEAR};
use oysterpack_smart_near::near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env,
    serde::{Deserialize, Serialize},
};
use std::cmp::Ordering;
use std::collections::BTreeMap;

/// unstaked NEAR is locked for 4 epochs before being able to be withdrawn
/// https://github.com/near/nearcore/blob/037954e087fd5c8a65598ede502495530c73f835/chain/epoch_manager/src/lib.rs#L815
const EPOCHS_LOCKED: usize = 4;

#[derive(
    BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Default,
)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub struct UnstakedBalances {
    available: YoctoNear,
    locked: [Option<(EpochHeight, YoctoNear)>; EPOCHS_LOCKED],
}

impl UnstakedBalances {
    pub fn total(&self) -> YoctoNear {
        self.available
            + self.locked.iter().fold(ZERO_NEAR, |total, entry| {
                total + entry.map_or(ZERO_NEAR, |(_, amount)| amount)
            })
    }

    pub fn available(&self) -> YoctoNear {
        self.available
    }

    pub fn locked(&self) -> Option<BTreeMap<EpochHeight, YoctoNear>> {
        let result: BTreeMap<EpochHeight, YoctoNear> =
            self.locked
                .iter()
                .fold(BTreeMap::new(), |mut result, entry| {
                    if let Some((epoch, amount)) = entry {
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

    pub fn unlock(&mut self) {
        let current_epoch: EpochHeight = env::epoch_height().into();

        for i in 0..EPOCHS_LOCKED {
            if let Some((epoch, balance)) = self.locked[i] {
                if epoch <= current_epoch {
                    self.available += balance;
                    self.locked[i] = None;
                }
            }
        }
    }

    /// adds the unstaked balance and locks it up for 4 epochs
    pub fn credit_unstaked(&mut self, amount: YoctoNear) {
        self.unlock();
        let available_on: EpochHeight = (env::epoch_height() + EPOCHS_LOCKED as u64).into();
        for i in 0..EPOCHS_LOCKED {
            if let Some((epoch, balance)) = self.locked[i] {
                if epoch == available_on {
                    self.locked[i] = Some((epoch, balance + amount));
                    return;
                }
            }
        }

        for i in 0..EPOCHS_LOCKED {
            if self.locked[i].is_none() {
                self.locked[i] = Some((available_on, amount));
                return;
            }
        }

        unreachable!()
    }

    fn sort_locked(&mut self) {
        self.locked.sort_by(|left, right| {
            if left.is_none() && right.is_none() {
                return Ordering::Equal;
            }
            if left.is_some() && right.is_none() {
                return Ordering::Less;
            }
            if left.is_none() && right.is_some() {
                return Ordering::Greater;
            }
            left.unwrap().0.cmp(&right.unwrap().0)
        });
    }

    /// tries to debit the specified amount from the available balance
    ///
    /// ## Panics
    /// if there are insufficient funds
    pub fn debit_available_balance(&mut self, amount: YoctoNear) {
        self.unlock();
        ERR_INSUFFICIENT_FUNDS.assert(|| self.available >= amount);
        self.available -= amount;
    }

    pub fn debit_for_restaking(&mut self, mut amount: YoctoNear) {
        let total = self.total();
        ERR_INSUFFICIENT_FUNDS.assert(|| total >= amount);

        if total == self.available {
            self.available -= amount;
            return;
        }

        self.sort_locked();

        // restake from the most recent unstaked balances
        for i in (0..EPOCHS_LOCKED).rev() {
            if let Some((available_on, unstaked)) = self.locked[i] {
                if unstaked <= amount {
                    amount -= unstaked;
                    self.locked[i] = None;
                } else {
                    self.locked[i] = Some((available_on, unstaked - amount));
                    return;
                }

                if amount == ZERO_NEAR {
                    return;
                }
            }
        }

        self.available -= amount;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oysterpack_smart_near::domain::ZERO_NEAR;
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

        assert_eq!(unstaked_balances.available, ZERO_NEAR);
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
        assert_eq!(unstaked_balances.available, ZERO_NEAR);
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
                    Some((107.into(), YOCTO.into())),
                    Some((108.into(), YOCTO.into())),
                    Some((110.into(), YOCTO.into())),
                    None
                ]
            }
        );

        unstaked_balances.debit_for_restaking(1000.into());
        assert_eq!(
            unstaked_balances,
            UnstakedBalances {
                available: (2 * YOCTO).into(),
                locked: [
                    Some((107.into(), YOCTO.into())),
                    Some((108.into(), YOCTO.into())),
                    Some((110.into(), (YOCTO - 1000).into())),
                    None
                ]
            }
        );

        unstaked_balances.debit_for_restaking(YOCTO.into());
        assert_eq!(
            unstaked_balances,
            UnstakedBalances {
                available: (2 * YOCTO).into(),
                locked: [
                    Some((107.into(), YOCTO.into())),
                    Some((108.into(), (YOCTO - 1000).into())),
                    None,
                    None
                ]
            }
        );

        unstaked_balances.debit_for_restaking((2 * YOCTO).into());
        assert_eq!(
            unstaked_balances,
            UnstakedBalances {
                available: ((2 * YOCTO) - 1000).into(),
                locked: [None, None, None, None]
            }
        );

        unstaked_balances.debit_for_restaking(YOCTO.into());
        assert_eq!(
            unstaked_balances,
            UnstakedBalances {
                available: (YOCTO - 1000).into(),
                locked: [None, None, None, None]
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
        assert_eq!(unstaked_balances.total(), ZERO_NEAR);
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

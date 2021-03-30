use crate::UnstakedBalance;
use oysterpack_smart_near::asserts::ERR_INVALID;
use oysterpack_smart_near::domain::{EpochHeight, YoctoNear};
use oysterpack_smart_near::near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{
        de::{self, *},
        ser::*,
        Deserialize, Deserializer, Serialize, Serializer,
    },
};
use std::collections::HashSet;
use std::convert::{TryFrom, TryInto};
use std::{
    collections::HashMap,
    fmt::{self, Formatter},
};

/// unstaked balances will be sorted by withdrawal availability, i.e., by epoch height
#[derive(BorshDeserialize, BorshSerialize, Debug, Clone, Copy, PartialEq)]
pub enum UnstakedBalances {
    Zero,
    One(UnstakedBalance),
    Two((UnstakedBalance, UnstakedBalance)),
    Three((UnstakedBalance, UnstakedBalance, UnstakedBalance)),
    Four(
        (
            UnstakedBalance,
            UnstakedBalance,
            UnstakedBalance,
            UnstakedBalance,
        ),
    ),
}

impl UnstakedBalances {
    pub fn total_unstaked_balance(&self) -> YoctoNear {
        let balances: Vec<UnstakedBalance> = (*self).into();
        balances.iter().map(|balance| balance.balance).sum()
    }

    /// returns the unstaked balance amount that is available for withdrawal based on epoch height
    pub fn unstaked_available_balance(&self) -> YoctoNear {
        let balances: Vec<UnstakedBalance> = (*self).into();
        balances
            .iter()
            .filter_map(|balance| {
                if balance.is_available() {
                    Some(balance.balance)
                } else {
                    None
                }
            })
            .sum()
    }

    pub fn remove_available_balances(self) -> UnstakedBalances {
        let balances: Vec<UnstakedBalance> = self.into();
        let balances: Vec<UnstakedBalance> = balances
            .iter()
            .cloned()
            .filter(|balance| !balance.is_available())
            .collect();
        balances.as_slice().try_into().unwrap()
    }
}

impl Default for UnstakedBalances {
    fn default() -> Self {
        UnstakedBalances::Zero
    }
}

impl Serialize for UnstakedBalances {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        let balances: Vec<UnstakedBalance> = self.clone().into();
        let mut seq = serializer.serialize_seq(Some(balances.len()))?;
        for element in balances {
            seq.serialize_element(&element)?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for UnstakedBalances {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(UnstakedBalancesVisitor)
    }
}

struct UnstakedBalancesVisitor;

impl<'de> Visitor<'de> for UnstakedBalancesVisitor {
    type Value = UnstakedBalances;

    fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("Vec<UnstakedBalance>")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut balances = vec![];
        loop {
            match seq.next_element::<UnstakedBalance>()? {
                None => break,
                Some(balance) => balances.push(balance),
            }
        }

        let balances = match balances.len() {
            0 => UnstakedBalances::Zero,
            1 => UnstakedBalances::One((&balances[0]).clone()),
            2 => UnstakedBalances::Two(((&balances[0]).clone(), (&balances[0]).clone())),
            3 => UnstakedBalances::Three((
                (&balances[0]).clone(),
                (&balances[1]).clone(),
                (&balances[2]).clone(),
            )),
            4 => UnstakedBalances::Four((
                (&balances[0]).clone(),
                (&balances[1]).clone(),
                (&balances[2]).clone(),
                (&balances[3]).clone(),
            )),
            _ => {
                return Err(de::Error::custom(format!(
                    "there should be at most 4 unstaked balances: {}",
                    balances.len()
                )))
            }
        };

        Ok(balances)
    }
}

impl From<UnstakedBalances> for HashMap<EpochHeight, YoctoNear> {
    fn from(balances: UnstakedBalances) -> Self {
        match balances {
            UnstakedBalances::Zero => HashMap::with_capacity(0),
            UnstakedBalances::One(balance) => {
                let mut map = HashMap::with_capacity(1);
                map.insert(balance.available_on, balance.balance);
                map
            }
            UnstakedBalances::Two((balance1, balance2)) => {
                let mut map = HashMap::with_capacity(2);
                map.insert(balance1.available_on, balance1.balance);
                map.insert(balance2.available_on, balance2.balance);
                map
            }
            UnstakedBalances::Three((balance1, balance2, balance3)) => {
                let mut map = HashMap::with_capacity(3);
                map.insert(balance1.available_on, balance1.balance);
                map.insert(balance2.available_on, balance2.balance);
                map.insert(balance3.available_on, balance3.balance);
                map
            }
            UnstakedBalances::Four((balance1, balance2, balance3, balance4)) => {
                let mut map = HashMap::with_capacity(4);
                map.insert(balance1.available_on, balance1.balance);
                map.insert(balance2.available_on, balance2.balance);
                map.insert(balance3.available_on, balance3.balance);
                map.insert(balance4.available_on, balance4.balance);
                map
            }
        }
    }
}

impl From<UnstakedBalances> for Vec<UnstakedBalance> {
    /// balances will be sorted by [`EpochHeight`]
    fn from(balances: UnstakedBalances) -> Self {
        let mut balances = match balances {
            UnstakedBalances::Zero => vec![],
            UnstakedBalances::One(balance) => vec![balance],
            UnstakedBalances::Two((balance1, balance2)) => vec![balance1, balance2],
            UnstakedBalances::Three((balance1, balance2, balance3)) => {
                vec![balance1, balance2, balance3]
            }
            UnstakedBalances::Four((balance1, balance2, balance3, balance4)) => {
                vec![balance1, balance2, balance3, balance4]
            }
        };
        balances.sort();
        balances
    }
}

/// balances will be sorted by [`EpochHeight`]
impl TryFrom<&[UnstakedBalance]> for UnstakedBalances {
    type Error = oysterpack_smart_near::Error<String>;

    fn try_from(value: &[UnstakedBalance]) -> Result<Self, Self::Error> {
        if value.is_empty() || value.len() > 4 {
            return Err(ERR_INVALID.error("there must be 1-4 unstaked balances".to_string()));
        }

        if value.len() == 1 {
            return Ok(UnstakedBalances::One((&value[0]).clone()));
        }

        let mut balances = value.to_vec();
        balances.sort();
        let epochs = balances.iter().fold(
            HashSet::with_capacity(value.len()),
            |mut epochs, balance| {
                epochs.insert(*balance.available_on);
                epochs
            },
        );
        if epochs.len() != value.len() {
            return Err(
                ERR_INVALID.error("UnstakedBalance::available_on must be unique".to_string())
            );
        }

        let balances = match value {
            [b1, b2] => UnstakedBalances::Two((b1.clone(), b2.clone())),
            [b1, b2, b3] => UnstakedBalances::Three((b1.clone(), b2.clone(), b3.clone())),
            [b1, b2, b3, b4] => {
                UnstakedBalances::Four((b1.clone(), b2.clone(), b3.clone(), b4.clone()))
            }
            _ => unreachable!(),
        };

        Ok(balances)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StakeAccountBalances;
    use oysterpack_smart_near::near_sdk::serde_json;
    use oysterpack_smart_near::YOCTO;

    #[test]
    fn json() {
        let mut balance = StakeAccountBalances {
            total: YOCTO.into(),
            available: 10_000.into(),
            staked: (YOCTO - (YOCTO / 2)).into(),
            unstaked: UnstakedBalances::Zero,
        };

        let json = serde_json::to_string_pretty(&balance).unwrap();
        println!("{}", json);

        balance = serde_json::from_str(&json).unwrap();

        balance.unstaked = UnstakedBalances::One(UnstakedBalance::new(1000.into(), 10.into()));

        let json = serde_json::to_string_pretty(&balance).unwrap();
        println!("{}", json);

        balance = serde_json::from_str(&json).unwrap();

        balance.unstaked = UnstakedBalances::Two((
            UnstakedBalance::new(1000.into(), 10.into()),
            UnstakedBalance::new(2000.into(), 10.into()),
        ));

        let json = serde_json::to_string_pretty(&balance).unwrap();
        println!("{}", json);

        balance = serde_json::from_str(&json).unwrap();

        balance.unstaked = UnstakedBalances::Three((
            UnstakedBalance::new(1000.into(), 10.into()),
            UnstakedBalance::new(2000.into(), 10.into()),
            UnstakedBalance::new(3000.into(), 10.into()),
        ));

        let json = serde_json::to_string_pretty(&balance).unwrap();
        println!("{}", json);

        balance = serde_json::from_str(&json).unwrap();

        balance.unstaked = UnstakedBalances::Four((
            UnstakedBalance::new(1000.into(), 10.into()),
            UnstakedBalance::new(2000.into(), 10.into()),
            UnstakedBalance::new(3000.into(), 10.into()),
            UnstakedBalance::new(4000.into(), 10.into()),
        ));

        let json = serde_json::to_string_pretty(&balance).unwrap();
        println!("{}", json);

        balance = serde_json::from_str(&json).unwrap();
        assert_eq!(
            balance.unstaked,
            UnstakedBalances::Four((
                UnstakedBalance::new(1000.into(), 10.into()),
                UnstakedBalance::new(2000.into(), 10.into()),
                UnstakedBalance::new(3000.into(), 10.into()),
                UnstakedBalance::new(4000.into(), 10.into()),
            ))
        );
    }

    #[test]
    fn to_vec() {
        let mut balances = UnstakedBalances::Four((
            UnstakedBalance::new(1000.into(), 10.into()),
            UnstakedBalance::new(5000.into(), 10.into()),
            UnstakedBalance::new(2000.into(), 10.into()),
            UnstakedBalance::new(4000.into(), 10.into()),
        ));

        let balances: Vec<UnstakedBalance> = balances.into();
        assert_eq!(
            balances,
            vec![
                UnstakedBalance::new(1000.into(), 10.into()),
                UnstakedBalance::new(2000.into(), 10.into()),
                UnstakedBalance::new(4000.into(), 10.into()),
                UnstakedBalance::new(5000.into(), 10.into()),
            ]
        );
    }
}

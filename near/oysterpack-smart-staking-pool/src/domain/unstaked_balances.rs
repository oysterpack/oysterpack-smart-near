use crate::UnstakedBalance;
use oysterpack_smart_near::domain::{EpochHeight, YoctoNear};
use oysterpack_smart_near::near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{
        de::{self, *},
        ser::*,
        Deserialize, Deserializer, Serialize, Serializer,
    },
};
use std::{
    collections::HashMap,
    fmt::{self, Formatter},
};

#[derive(BorshDeserialize, BorshSerialize, Debug, Clone, Copy, PartialEq)]
pub enum UnstakedBalances {
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
    fn from(balances: UnstakedBalances) -> Self {
        match balances {
            UnstakedBalances::One(balance) => vec![balance],
            UnstakedBalances::Two((balance1, balance2)) => vec![balance1, balance2],
            UnstakedBalances::Three((balance1, balance2, balance3)) => {
                vec![balance1, balance2, balance3]
            }
            UnstakedBalances::Four((balance1, balance2, balance3, balance4)) => {
                vec![balance1, balance2, balance3, balance4]
            }
        }
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
            unstaked: None,
        };

        let json = serde_json::to_string_pretty(&balance).unwrap();
        println!("{}", json);

        balance = serde_json::from_str(&json).unwrap();

        balance.unstaked = Some(UnstakedBalances::One(UnstakedBalance::new(
            1000.into(),
            10.into(),
        )));

        let json = serde_json::to_string_pretty(&balance).unwrap();
        println!("{}", json);

        balance = serde_json::from_str(&json).unwrap();

        balance.unstaked = Some(UnstakedBalances::Two((
            UnstakedBalance::new(1000.into(), 10.into()),
            UnstakedBalance::new(2000.into(), 10.into()),
        )));

        let json = serde_json::to_string_pretty(&balance).unwrap();
        println!("{}", json);

        balance = serde_json::from_str(&json).unwrap();

        balance.unstaked = Some(UnstakedBalances::Three((
            UnstakedBalance::new(1000.into(), 10.into()),
            UnstakedBalance::new(2000.into(), 10.into()),
            UnstakedBalance::new(3000.into(), 10.into()),
        )));

        let json = serde_json::to_string_pretty(&balance).unwrap();
        println!("{}", json);

        balance = serde_json::from_str(&json).unwrap();

        balance.unstaked = Some(UnstakedBalances::Four((
            UnstakedBalance::new(1000.into(), 10.into()),
            UnstakedBalance::new(2000.into(), 10.into()),
            UnstakedBalance::new(3000.into(), 10.into()),
            UnstakedBalance::new(4000.into(), 10.into()),
        )));

        let json = serde_json::to_string_pretty(&balance).unwrap();
        println!("{}", json);

        balance = serde_json::from_str(&json).unwrap();
        assert_eq!(
            balance.unstaked,
            Some(UnstakedBalances::Four((
                UnstakedBalance::new(1000.into(), 10.into()),
                UnstakedBalance::new(2000.into(), 10.into()),
                UnstakedBalance::new(3000.into(), 10.into()),
                UnstakedBalance::new(4000.into(), 10.into()),
            )))
        );
    }
}

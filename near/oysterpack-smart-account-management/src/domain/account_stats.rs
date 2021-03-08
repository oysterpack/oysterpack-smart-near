use crate::AccountStorageEvent;
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env,
    serde::{Deserialize, Serialize},
};
use oysterpack_smart_near::data::numbers::U128;
use oysterpack_smart_near::{data::Object, domain::YoctoNear};

const ACCOUNT_STATS_KEY: u128 = 1952364736129901845182088441739779955;

type AccountStatsObject = Object<u128, AccountStats>;

/// Account statistics
#[derive(
    BorshSerialize, BorshDeserialize, Deserialize, Serialize, Copy, Clone, Debug, PartialEq, Default,
)]
#[serde(crate = "near_sdk::serde")]
pub struct AccountStats {
    total_registered_accounts: U128,
    total_accounts_near_balance: YoctoNear,
}

impl AccountStats {
    pub fn load() -> AccountStats {
        let stats = AccountStatsObject::load(&ACCOUNT_STATS_KEY)
            .unwrap_or_else(|| AccountStatsObject::new(ACCOUNT_STATS_KEY, AccountStats::default()));
        *stats
    }

    pub fn save(&self) {
        AccountStatsObject::new(ACCOUNT_STATS_KEY, *self).save();
    }

    pub fn on_account_storage_event(event: &AccountStorageEvent) {
        // TODO
        env::log(format!("{:?}", event).as_bytes());
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use oysterpack_smart_near_test::*;

    #[test]
    fn on_account_storage_event() {
        // Arrange
        let account_id = "bob.near";
        let context = new_context(account_id);
        testing_env!(context);

        let stats = AccountStats::load();
        assert_eq!(stats.total_registered_accounts, 0.into());
        assert_eq!(stats.total_accounts_near_balance, 0.into());
        // TODO
    }
}

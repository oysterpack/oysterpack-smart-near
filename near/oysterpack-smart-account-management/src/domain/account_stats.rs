use crate::AccountStorageEvent;
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env,
};
use oysterpack_smart_near::{data::Object, domain::YoctoNear};

const ACCOUNT_STATS_KEY: u128 = 1952364736129901845182088441739779955;

type AccountStatsObject = Object<u128, AccountStats>;

#[derive(BorshSerialize, BorshDeserialize, Copy, Clone, Debug, PartialEq, Default)]
pub struct AccountStats {
    total_registered_accounts: u128,
    total_accounts_near_balance: YoctoNear,
    total_accounts_storage_available_balance: YoctoNear,
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
        env::log(format!("{:?}", event).as_bytes());
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use oysterpack_smart_near::Hash;
    use oysterpack_smart_near_test::*;

    #[test]
    fn hash_id() {
        // Arrange
        let account_id = "bob.near";
        let context = new_context(account_id);
        testing_env!(context);
    }
}

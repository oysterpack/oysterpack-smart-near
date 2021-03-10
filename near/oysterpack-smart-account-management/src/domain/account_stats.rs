use crate::AccountStorageEvent;
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env,
    serde::{Deserialize, Serialize},
};

use lazy_static::lazy_static;
use oysterpack_smart_near::{
    data::{numbers::U128, Object},
    domain::{StorageUsage, YoctoNear},
    eventbus,
};
use std::sync::Mutex;

const ACCOUNT_STATS_KEY: u128 = 1952364736129901845182088441739779955;

type AccountStatsObject = Object<u128, AccountStats>;

/// Account statistics
#[derive(
    BorshSerialize, BorshDeserialize, Deserialize, Serialize, Copy, Clone, Debug, PartialEq, Default,
)]
#[serde(crate = "near_sdk::serde")]
pub struct AccountStats {
    pub total_registered_accounts: U128,
    pub total_near_balance: YoctoNear,
    pub total_storage_usage: StorageUsage,
}

lazy_static! {
    static ref ACCOUNT_STORAGE_EVENT_HANDLER_REGISTERED: Mutex<bool> = Mutex::new(false);
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

    /// meant for unit testing purposes
    pub(crate) fn reset() {
        let mut stats = AccountStats::load();
        stats.total_storage_usage = 0.into();
        stats.total_near_balance = 0.into();
        stats.total_registered_accounts = 0.into();
        stats.save();
    }

    /// can be safely called multiple times and will only register the event handler once
    pub fn register_account_storage_event_handler() {
        let mut registered = ACCOUNT_STORAGE_EVENT_HANDLER_REGISTERED.lock().unwrap();
        if !*registered {
            eventbus::register(AccountStats::on_account_storage_event);
            *registered = true;
        }
    }

    pub fn on_account_storage_event(event: &AccountStorageEvent) {
        env::log(format!("{:?}", event).as_bytes());

        let mut stats = AccountStats::load();

        match event {
            AccountStorageEvent::Registered(storage_balance, storage_usage) => {
                stats.total_registered_accounts = stats
                    .total_registered_accounts
                    .checked_add(1)
                    .expect("total_registered_accounts overflow")
                    .into();

                stats.total_near_balance = stats
                    .total_near_balance
                    .checked_add(storage_balance.total.value())
                    .expect("total_near_balance overflow")
                    .into();

                stats.total_storage_usage = stats
                    .total_storage_usage
                    .checked_add(storage_usage.value())
                    .expect("total_storage_usage overflow")
                    .into();
            }

            AccountStorageEvent::Deposit(amount) => {
                stats.total_near_balance = stats
                    .total_near_balance
                    .checked_add(amount.value())
                    .expect("total_near_balance overflow")
                    .into();
            }
            AccountStorageEvent::Withdrawal(amount) => {
                stats.total_near_balance = stats
                    .total_near_balance
                    .checked_sub(amount.value())
                    .expect("total_near_balance overflow")
                    .into();
            }
            AccountStorageEvent::StorageUsageChanged(change) => {
                if change.is_positive() {
                    stats.total_storage_usage = stats
                        .total_storage_usage
                        .checked_add(change.value() as u64)
                        .expect("total_storage_usage overflow")
                        .into();
                } else {
                    stats.total_storage_usage = stats
                        .total_storage_usage
                        .checked_sub(change.value().abs() as u64)
                        .expect("total_storage_usage overflow")
                        .into();
                }
            }

            AccountStorageEvent::Unregistered(account_near_balance, account_storage_usage) => {
                stats.total_registered_accounts = stats
                    .total_registered_accounts
                    .checked_sub(1)
                    .expect("total_registered_accounts overflow")
                    .into();

                stats.total_near_balance = stats
                    .total_near_balance
                    .checked_sub(account_near_balance.value())
                    .expect("total_near_balance overflow")
                    .into();

                stats.total_storage_usage = stats
                    .total_storage_usage
                    .checked_sub(account_storage_usage.value())
                    .expect("total_storage_usage overflow")
                    .into();
            }
        }

        stats.save();
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::StorageBalance;
    use oysterpack_smart_near::domain::StorageUsageChange;
    use oysterpack_smart_near::*;
    use oysterpack_smart_near_test::*;

    #[test]
    fn on_account_storage_event() {
        // Arrange
        let account_id = "bob.near";
        let context = new_context(account_id);
        testing_env!(context);

        AccountStats::register_account_storage_event_handler();

        let stats = AccountStats::load();
        assert_eq!(stats.total_registered_accounts, 0.into());
        assert_eq!(stats.total_near_balance, 0.into());
        assert_eq!(stats.total_storage_usage, 0.into());

        // Act - account registered
        let storage_balance = StorageBalance {
            total: YOCTO.into(),
            available: 0.into(),
        };
        eventbus::post(&AccountStorageEvent::Registered(
            storage_balance,
            1000.into(),
        ));

        // Assert
        let stats = AccountStats::load();
        assert_eq!(stats.total_registered_accounts, 1.into());
        assert_eq!(stats.total_near_balance, YOCTO.into());
        assert_eq!(stats.total_storage_usage, 1000.into());

        // Act - deposit
        eventbus::post(&AccountStorageEvent::Deposit(YOCTO.into()));

        // Assert
        let stats = AccountStats::load();
        assert_eq!(stats.total_registered_accounts, 1.into());
        assert_eq!(stats.total_near_balance, (2 * YOCTO).into());
        assert_eq!(stats.total_storage_usage, 1000.into());

        // Act - withdraw
        eventbus::post(&AccountStorageEvent::Withdrawal(YOCTO.into()));

        // Assert
        let stats = AccountStats::load();
        assert_eq!(stats.total_registered_accounts, 1.into());
        assert_eq!(stats.total_near_balance, YOCTO.into());
        assert_eq!(stats.total_storage_usage, 1000.into());

        // Act - storage usage increase
        eventbus::post(&AccountStorageEvent::StorageUsageChanged(1000.into()));

        // Assert
        let stats = AccountStats::load();
        assert_eq!(stats.total_registered_accounts, 1.into());
        assert_eq!(stats.total_near_balance, YOCTO.into());
        assert_eq!(stats.total_storage_usage, 2000.into());

        // Act - storage usage decrease
        eventbus::post(&AccountStorageEvent::StorageUsageChanged(
            StorageUsageChange(-1000),
        ));

        // Assert
        let stats = AccountStats::load();
        assert_eq!(stats.total_registered_accounts, 1.into());
        assert_eq!(stats.total_near_balance, YOCTO.into());
        assert_eq!(stats.total_storage_usage, 1000.into());

        // Act - account unregistered
        eventbus::post(&AccountStorageEvent::Unregistered(
            YOCTO.into(),
            StorageUsage(1000),
        ));

        // Assert
        let stats = AccountStats::load();
        assert_eq!(stats.total_registered_accounts, 0.into());
        assert_eq!(stats.total_near_balance, 0.into());
        assert_eq!(stats.total_storage_usage, 0.into());
    }
}

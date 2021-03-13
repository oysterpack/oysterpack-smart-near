use crate::AccountStorageEvent;
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
};

use crate::AccountNearDataObject;
use lazy_static::lazy_static;
use oysterpack_smart_near::{
    data::{numbers::U128, Object},
    domain::{StorageUsage, YoctoNear},
    eventbus,
};
use std::sync::Mutex;

const ACCOUNT_STATS_KEY: u128 = 1952364736129901845182088441739779955;

type DAO = Object<u128, AccountMetrics>;

/// Account metrics
#[derive(
    BorshSerialize, BorshDeserialize, Deserialize, Serialize, Copy, Clone, Debug, PartialEq, Default,
)]
#[serde(crate = "near_sdk::serde")]
pub struct AccountMetrics {
    pub total_registered_accounts: U128,
    pub total_near_balance: YoctoNear,
    pub total_storage_usage: StorageUsage,
}

lazy_static! {
    static ref ACCOUNT_STORAGE_EVENT_HANDLER_REGISTERED: Mutex<bool> = Mutex::new(false);
}

impl AccountMetrics {
    pub fn load() -> AccountMetrics {
        let stats = DAO::load(&ACCOUNT_STATS_KEY)
            .unwrap_or_else(|| DAO::new(ACCOUNT_STATS_KEY, AccountMetrics::default()));
        *stats
    }

    pub fn save(&self) {
        DAO::new(ACCOUNT_STATS_KEY, *self).save();
    }

    #[cfg(test)]
    pub(crate) fn reset() {
        let mut stats = AccountMetrics::load();
        stats.total_storage_usage = 0.into();
        stats.total_near_balance = 0.into();
        stats.total_registered_accounts = 0.into();
        stats.save();
    }

    /// can be safely called multiple times and will only register the event handler once
    pub(crate) fn register_account_storage_event_handler() {
        let mut registered = ACCOUNT_STORAGE_EVENT_HANDLER_REGISTERED.lock().unwrap();
        if !*registered {
            eventbus::register(AccountMetrics::on_account_storage_event);
            *registered = true;
        }
    }

    fn on_account_storage_event(event: &AccountStorageEvent) {
        event.log();

        let mut stats = AccountMetrics::load();

        match *event {
            AccountStorageEvent::Registered(storage_balance) => {
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
            AccountStorageEvent::StorageUsageChanged(account_id_hash, change) => {
                if change.value() != 0 {
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

                    if let Some(mut account) = AccountNearDataObject::load(account_id_hash) {
                        if change.is_positive() {
                            account.incr_storage_usage((change.value() as u64).into())
                        } else {
                            account.dec_storage_usage((change.value().abs() as u64).into())
                        }
                        account.save();
                    }
                }
            }

            AccountStorageEvent::Unregistered(account_near_balance) => {
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
            }
        }

        stats.save();
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::StorageBalance;
    use oysterpack_smart_near::domain::{StorageUsageChange, ZERO_NEAR};
    use oysterpack_smart_near::*;
    use oysterpack_smart_near_test::*;

    #[test]
    fn on_account_storage_event() {
        // Arrange
        let account_id = "bob.near";
        let context = new_context(account_id);
        testing_env!(context);

        AccountMetrics::register_account_storage_event_handler();

        let account = AccountNearDataObject::new(account_id, ZERO_NEAR);
        account.save();

        let stats = AccountMetrics::load();
        assert_eq!(stats.total_registered_accounts, 0.into());
        assert_eq!(stats.total_near_balance, 0.into());
        assert_eq!(stats.total_storage_usage, 0.into());

        // Act - account registered
        let storage_balance = StorageBalance {
            total: YOCTO.into(),
            available: 0.into(),
        };
        eventbus::post(&AccountStorageEvent::Registered(storage_balance));

        // Assert
        let stats = AccountMetrics::load();
        assert_eq!(stats.total_registered_accounts, 1.into());
        assert_eq!(stats.total_near_balance, YOCTO.into());
        assert_eq!(stats.total_storage_usage, 0.into());

        // Act - deposit
        eventbus::post(&AccountStorageEvent::Deposit(YOCTO.into()));

        // Assert
        let stats = AccountMetrics::load();
        assert_eq!(stats.total_registered_accounts, 1.into());
        assert_eq!(stats.total_near_balance, (2 * YOCTO).into());
        assert_eq!(stats.total_storage_usage, 0.into());

        // Act - withdraw
        eventbus::post(&AccountStorageEvent::Withdrawal(YOCTO.into()));

        // Assert
        let stats = AccountMetrics::load();
        assert_eq!(stats.total_registered_accounts, 1.into());
        assert_eq!(stats.total_near_balance, YOCTO.into());
        assert_eq!(stats.total_storage_usage, 0.into());
        let initial_account_storage_usage = account.storage_usage();

        // Act - storage usage increase
        eventbus::post(&AccountStorageEvent::StorageUsageChanged(
            account.key().account_id_hash(),
            1000_u64.into(),
        ));

        // Assert
        let stats = AccountMetrics::load();
        assert_eq!(stats.total_registered_accounts, 1.into());
        assert_eq!(stats.total_near_balance, YOCTO.into());
        assert_eq!(stats.total_storage_usage, 1000.into());

        let account = AccountNearDataObject::load(account_id).unwrap();
        assert_eq!(
            account.storage_usage(),
            initial_account_storage_usage + 1000.into()
        );

        // Act - storage usage decrease
        eventbus::post(&AccountStorageEvent::StorageUsageChanged(
            account.key().account_id_hash(),
            StorageUsageChange(-1000),
        ));

        // Assert
        let stats = AccountMetrics::load();
        assert_eq!(stats.total_registered_accounts, 1.into());
        assert_eq!(stats.total_near_balance, YOCTO.into());
        assert_eq!(stats.total_storage_usage, 0.into());

        let account = AccountNearDataObject::load(account_id).unwrap();
        assert_eq!(account.storage_usage(), initial_account_storage_usage);

        // Act - account unregistered
        eventbus::post(&AccountStorageEvent::Unregistered(YOCTO.into()));

        // Assert
        let stats = AccountMetrics::load();
        assert_eq!(stats.total_registered_accounts, 0.into());
        assert_eq!(stats.total_near_balance, 0.into());
        assert_eq!(stats.total_storage_usage, 0.into());
    }
}

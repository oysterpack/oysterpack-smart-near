use crate::{AccountIdHash, StorageBalance};
use lazy_static::lazy_static;
use oysterpack_smart_near::domain::StorageUsageChange;
use oysterpack_smart_near::{domain::YoctoNear, eventbus::*, Level, LogEvent};
use std::fmt::{self, Display, Formatter};
use std::sync::Mutex;

/// Account storage related events
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum AccountStorageEvent {
    /// an account was registered
    Registered(StorageBalance),
    // an account made a deposit
    Deposit(YoctoNear),
    /// an account made a withdrawal from its storage available balance
    Withdrawal(YoctoNear),
    /// account storage usage changed
    StorageUsageChanged(AccountIdHash, StorageUsageChange),
    /// an account was unregistered
    /// - its NEAR balance was refunded
    Unregistered(YoctoNear),
}

impl Display for AccountStorageEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            AccountStorageEvent::StorageUsageChanged(_, change) => write!(f, "{:?}", change),
            _ => write!(f, "{:?}", self),
        }
    }
}

/// log event for [`AccountStorageEvent`]
pub const LOG_EVENT_ACCOUNT_STORAGE_CHANGED: LogEvent =
    LogEvent(Level::INFO, "ACCOUNT_STORAGE_CHANGED");

impl AccountStorageEvent {
    pub fn log(&self) {
        LOG_EVENT_ACCOUNT_STORAGE_CHANGED.log(self.to_string());
    }
}

// TODO: create macro to generate boilerplate code for event: #[event]
lazy_static! {
    static ref ACCOUNT_STORAGE_EVENTS: Mutex<EventHandlers<AccountStorageEvent>> =
        Mutex::new(EventHandlers::new());
}

impl Event for AccountStorageEvent {
    fn handlers<F>(f: F)
    where
        F: FnOnce(&EventHandlers<Self>),
    {
        f(&*ACCOUNT_STORAGE_EVENTS.lock().unwrap())
    }

    fn handlers_mut<F>(f: F)
    where
        F: FnOnce(&mut EventHandlers<Self>),
    {
        f(&mut *ACCOUNT_STORAGE_EVENTS.lock().unwrap())
    }
}

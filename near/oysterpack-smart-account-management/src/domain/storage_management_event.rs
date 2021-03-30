use lazy_static::lazy_static;
use oysterpack_smart_near::eventbus::*;
use oysterpack_smart_near::near_sdk::AccountId;
use std::fmt::{self, Display, Formatter};
use std::sync::Mutex;

/// Account storage related events
#[derive(Debug, PartialEq, Clone)]
pub enum StorageManagementEvent {
    /// Invoked before funds are withdrawn. This provides a hook to update balances before the withdrawal.
    ///
    /// ## Use Case
    /// Account may have funds that are locked and managed externally by other components. For example,
    /// when unstaked NEAR balances become unlocked, then they should be made available for withdraawal.
    PreWithdraw(AccountId),
    /// Invoked before the account is unregistered. It provides a hook for other components to run
    /// component specific business logic to unregister the account.
    ///
    /// The hook is responsible for:
    /// - if `force=false`, panic if the account cannot be deleted because of contract specific
    ///   business logic, e.g., for FT, the account cannot unregister if it has a token balance
    /// - delete any account data outside of the [`crate::AccountNearDataObject`] and [`crate::AccountDataObject`] objects
    /// - apply any component specific business logic
    ///
    /// After all hooks are run, the [AccountManagementComponent][1] will be responsible for
    /// - sending account NEAR balance refund
    /// - publishing events
    /// - deleting [`crate::AccountNearDataObject`] and [`crate::AccountDataObject`] objects from contract storage
    ///
    /// ## NOTES
    ///- the predecessor account is being unregistered
    /// - hooks should use [`crate::ERR_CODE_UNREGISTER_FAILURE`] for failures
    ///
    /// [1]: crate::components::account_management::AccountManagementComponent
    PreUnregister { account_id: AccountId, force: bool },
}

impl Display for StorageManagementEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            _ => write!(f, "{:?}", self),
        }
    }
}

lazy_static! {
    static ref EVENT_HANDLERS: Mutex<EventHandlers<StorageManagementEvent>> =
        Mutex::new(EventHandlers::new());
}

impl Event for StorageManagementEvent {
    fn handlers<F>(f: F)
    where
        F: FnOnce(&EventHandlers<Self>),
    {
        match EVENT_HANDLERS.lock() {
            Ok(guard) => f(&*guard),
            Err(poisoned) => f(&*poisoned.into_inner()),
        };
    }

    fn handlers_mut<F>(f: F)
    where
        F: FnOnce(&mut EventHandlers<Self>),
    {
        match EVENT_HANDLERS.lock() {
            Ok(mut guard) => f(&mut *guard),
            Err(poisoned) => f(&mut *poisoned.into_inner()),
        };
    }
}

impl StorageManagementEvent {
    pub fn clear_event_handlers() {
        match EVENT_HANDLERS.lock() {
            Ok(mut guard) => guard.clear(),
            Err(poisoned) => poisoned.into_inner().clear(),
        };
    }
}

use near_sdk::json_types::ValidAccountId;
use oysterpack_smart_near::domain::StorageUsage;

use crate::{AccountNearDataObject, AccountStorageUsage, StorageUsageBounds};
use oysterpack_smart_near::service::{Deploy, Service};

#[derive(Default)]
pub struct AccountStorageUsageComponent;

impl AccountStorageUsage for AccountStorageUsageComponent {
    fn storage_usage_bounds(&self) -> StorageUsageBounds {
        *Self::load_state().unwrap()
    }

    fn storage_usage(&self, account_id: ValidAccountId) -> Option<StorageUsage> {
        AccountNearDataObject::load(account_id.as_ref().as_str())
            .map(|account| account.storage_usage())
    }
}

impl Service for AccountStorageUsageComponent {
    type State = StorageUsageBounds;

    fn state_key() -> u128 {
        1952475351321611295376996018476025471
    }
}

impl Deploy for AccountStorageUsageComponent {
    type Config = Self::State;

    fn deploy(config: Option<Self::Config>) {
        let state = config.expect("initial state must be provided");
        let state = Self::new_state(state);
        state.save();
    }
}

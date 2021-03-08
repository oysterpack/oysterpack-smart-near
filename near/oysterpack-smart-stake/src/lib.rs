use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env,
    json_types::U128,
    near_bindgen, wee_alloc, PanicOnDefault,
};
use oysterpack_smart_account_management::{
    Account, AccountManagementService, AccountStats, AccountStorageEvent, StorageBalance,
    StorageBalanceBounds, StorageUsageBounds,
};
use oysterpack_smart_near::{data::Object, domain::YoctoNear, eventbus, service::*};

use oysterpack_smart_account_management::components::account_service::*;
use shaku::*;

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
#[borsh_init(init)]
pub struct Contract {
    #[borsh_skip]
    key_value_store: KeyValueStoreService,
    #[borsh_skip]
    account_service_module: AccountServiceModule,
}

#[near_bindgen]
impl Contract {
    #[init]
    pub fn init_state() -> Self {
        assert!(!env::state_exists(), "contract is already initialized");
        AccountService::<()>::deploy(Some(StorageUsageBounds {
            min: 1000.into(),
            max: None,
        }));
        Self {
            key_value_store: KeyValueStoreService,
            account_service_module: Default::default(),
        }
    }

    pub fn simulate_account_storage_event(&self) {
        eventbus::post(&AccountStorageEvent::Registered(
            StorageBalance {
                total: 100.into(),
                available: 0.into(),
            },
            1000.into(),
        ));
    }

    pub fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        let service: &dyn AccountManagementService<()> = self.account_service_module.resolve_ref();
        service.storage_usage_bounds().into()
    }
}

impl Contract {
    /// get run each time the contract is loaded from storage and instantiated
    fn init(&mut self) {
        eventbus::register(AccountStats::on_account_storage_event);
        unsafe {
            FOO = 1;
        }
    }
}

static mut FOO: u128 = 0;

type Data = Object<u128, YoctoNear>;

#[near_bindgen]
impl KeyValueStore for Contract {
    fn get(&self, key: U128) -> Option<YoctoNear> {
        self.key_value_store.get(key)
    }

    fn set(&mut self, key: U128, value: YoctoNear) {
        self.key_value_store.set(key, value)
    }
}

trait KeyValueStore {
    fn get(&self, key: U128) -> Option<YoctoNear>;

    fn set(&mut self, key: U128, value: YoctoNear);
}

trait HasKeyValueStore {
    fn get_key_value_store(&self) -> &dyn KeyValueStore;

    fn get_mut_key_value_store(&mut self) -> &mut dyn KeyValueStore;
}

impl<T> KeyValueStore for T
where
    T: HasKeyValueStore,
{
    fn get(&self, key: U128) -> Option<YoctoNear> {
        self.get_key_value_store().get(key)
    }

    fn set(&mut self, key: U128, value: YoctoNear) {
        self.get_mut_key_value_store().set(key, value);
    }
}

#[derive(Default)]
struct KeyValueStoreService;

impl KeyValueStore for KeyValueStoreService {
    fn get(&self, key: U128) -> Option<YoctoNear> {
        let foo = unsafe { FOO };
        Data::load(&key.0).map(|object| YoctoNear::from(object.value() + foo))
    }

    fn set(&mut self, key: U128, value: YoctoNear) {
        Data::new(key.0, value).save();
    }
}

//////////
module! {
    pub AccountServiceModule {
        components = [AccountServiceComponent],
        providers = []
    }
}

pub type AccountServiceComponent = AccountService<()>;

impl Default for AccountServiceModule {
    fn default() -> Self {
        AccountServiceModule::builder()
            .with_component_parameters::<AccountServiceComponent>(AccountServiceParameters {
                unregister: unregister_always,
                _phantom: Default::default(),
            })
            .build()
    }
}

fn unregister_always(_account: Account<()>, _force: bool) -> bool {
    true
}

/////

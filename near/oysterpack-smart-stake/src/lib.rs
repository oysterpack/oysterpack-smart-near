use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env,
    json_types::U128,
    near_bindgen, wee_alloc, PanicOnDefault,
};
use oysterpack_smart_account_management::{AccountStats, AccountStorageEvent, StorageBalance};
use oysterpack_smart_near::{data::Object, domain::YoctoNear, EVENT_BUS};

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
#[borsh_init(init)]
pub struct Contract {
    #[borsh_skip]
    key_value_store: KeyValueStoreService,
}

#[near_bindgen]
impl Contract {
    #[init]
    pub fn init_state() -> Self {
        assert!(!env::state_exists(), "contract is already initialized");
        Self {
            key_value_store: KeyValueStoreService,
        }
    }

    pub fn simulate_account_storage_event(&self) {
        EVENT_BUS.post(&AccountStorageEvent::Registered(
            StorageBalance {
                total: 100.into(),
                available: 0.into(),
            },
            1000.into(),
        ));
    }
}

impl Contract {
    /// get run each time the contract is loaded from storage and instantiated
    fn init(&mut self) {
        EVENT_BUS.register(AccountStats::on_account_storage_event);
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

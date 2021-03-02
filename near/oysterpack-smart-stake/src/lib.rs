use near_sdk::json_types::U128;
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env, near_bindgen, wee_alloc, PanicOnDefault,
};
use oysterpack_smart_near::data::Object;
use oysterpack_smart_near::model::YoctoNear;

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    #[borsh_skip]
    key_value_store: KeyValueStoreService,
}

#[near_bindgen]
impl Contract {
    #[init]
    pub fn init() -> Self {
        assert!(!env::state_exists(), "contract is already initialized");
        Self {
            key_value_store: KeyValueStoreService,
        }
    }
}

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
        Data::load(&key.0)
            .unwrap()
            .map(|object| (*object.value()).into())
    }

    fn set(&mut self, key: U128, value: YoctoNear) {
        Data::new(key.0, value).save();
    }
}

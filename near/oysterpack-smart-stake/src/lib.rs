use near_sdk::json_types::U128;
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env, near_bindgen, wee_alloc, PanicOnDefault,
};
use oysterpack_smart_near::data::Object;

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {}

type Data = Object<u128, u128>;

#[near_bindgen]
impl Contract {
    #[init]
    pub fn init() -> Self {
        assert!(!env::state_exists(), "contract is already initialized");
        Self {}
    }

    /// TODO: REMOVE - was POC for Object
    pub fn get(key: U128) -> Option<U128> {
        Data::load(&key.0)
            .unwrap()
            .map(|object| (*object.value()).into())
    }

    /// TODO: REMOVE - was POC for Object
    pub fn set(&mut self, key: U128, value: U128) {
        Data::new(key.0, value.0).save()
    }
}

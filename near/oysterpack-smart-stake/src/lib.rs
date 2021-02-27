use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    near_bindgen, wee_alloc, PanicOnDefault,
};

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {}

#[near_bindgen]
impl Contract {}

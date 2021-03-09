```rust
//////////////////////////////////////////////////
module! {
    pub AccountServiceModule {
        components = [AccountServiceComponent],
        providers = []
    }
}
//////////////////////////////////////////////////
module! {
    pub StakeModule {
        components = [
            StakingServiceComponent, 
            StakeOperatorComponent
        ],
        providers = []
    }
}
//////////////////////////////////////////////////

#[near_bindgen]
#[borsh_init(init)]
// defines which interfaces to bind to and expose on the contract
// interface => module
#[oysterpack_smart_near_bindgen { 
    AccountManagementService { 
        module = account_service_module, 
        payable = [storage_deposit, storage_withdraw, storage_unregister]
    }, 
    StakingService {  module = stake_module }
}]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
struct Contract {
    #[borsh_skip]
    account_service_module: AccountServiceModule,
    #[borsh_skip]
    stake_module: AccountServiceModule,
    #[borsh_skip]
    foo_module: FooModule   // used internally by the contract
}

impl Contract {
    fn init(&mut self) {
        // configure modules
    }
}

#[near_bindgen]
impl Contract {
    pub fn foo(&self) -> Foo {
        let service: &dyn FooService = self.foo_module.resolve_ref();
        service.foo()
    }
}

```
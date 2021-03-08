//! This package provides a standard for building reusable smart contract stateful components
//!
//! ## Service Design
//! - Service declares it state type and defines a u128 based storage key
//!   - recommendation is to use ULID to generate the storage key
//! - Each service is responsible for managing its own state. This means when the service state changes
//!   it is the service's responsibility to save it to storage.
//! - [`Deploy`] - defines a pattern to standardize service deployment
//! - All components are lazily loaded. The pattern is to leverage lazy_static, i.e., define each service
//!   as a lazy_static, which will only be initialized on demand.

use crate::data::Object;
use near_sdk::borsh::{BorshDeserialize, BorshSerialize};
use std::fmt::Debug;

/// Defines abstraction for a stateful service.
/// - state must support Borsh serialization
pub trait Service {
    type State: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq;

    /// Used to to store the service state to blockchain storage
    /// - it is recommended to generate a ULID for the key to avoid collisions
    fn state_key() -> u128;

    /// loads the service state from storage using the key defined by [`state_key`]()
    fn load_state() -> Option<ServiceState<Self::State>> {
        ServiceState::<Self::State>::load(&Self::state_key())
    }

    /// creates new in-memory state, i.e., the state is not persisted to storage
    fn new_state(state: Self::State) -> ServiceState<Self::State> {
        ServiceState::<Self::State>::new(Self::state_key(), state)
    }
}

/// service state type
pub type ServiceState<T> = Object<u128, T>;

/// Provides standard interface pattern for contracts to use at deployment time to run service related
/// deployment code.
/// - for example, service may require config to initialize its persistent state
///
/// ## NOTES
/// - components may not need to implement ['Deploy']
pub trait Deploy: Service {
    type Config;

    /// invoked when the contract is first deployed
    /// - main use case is to initialize any service state
    fn deploy(config: Option<Self::Config>);
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazy_static::lazy_static;
    use oysterpack_smart_near_test::*;
    use std::ops::DerefMut;
    use std::sync::Mutex;

    lazy_static! {
        static ref FOO: Mutex<Foo> = Mutex::new(Foo::new());
    }

    type FooState = ServiceState<u128>;
    struct Foo {
        state: FooState,
    }

    impl Service for Foo {
        type State = u128;

        fn state_key() -> u128 {
            1952470210719526730429153601986271427
        }
    }

    impl Foo {
        fn new() -> Self {
            let state = Foo::load_state().unwrap_or(Foo::new_state(0));
            Self { state }
        }
    }

    impl Deploy for Foo {
        type Config = u128;

        fn deploy(config: Option<Self::Config>) {
            if let Some(config) = config {
                let state = Self::new_state(config);
                state.save();
            }
        }
    }

    #[test]
    fn service() {
        // Arrange
        let ctx = new_context("bob");
        testing_env!(ctx);

        Foo::deploy(None);

        // Act
        {
            let foo = FOO.lock().unwrap();
            assert_eq!(*foo.state, 0);
        }

        {
            let mut foo = FOO.lock().unwrap();
            *foo.state.deref_mut() = 1_u128;
            foo.state.save();
        }

        {
            let foo = FOO.lock().unwrap();
            assert_eq!(*foo.state, 1);
        }
    }
}

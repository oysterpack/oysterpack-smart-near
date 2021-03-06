//! This package provides a standard for building reusable smart contract stateful services
//!
//! ## Service Design
//! - Service declares it state type and defines a u128 based storage key
//!   - recommendation is to use ULID to generate the storage key
//! - Each service is responsible for managing its own state. This means when the service state changes
//!   it is the service's responsibility to save it to storage.
//! - Service lifecycle hooks:
//!   - [`Deploy`]
//!   - [`Init`]
//! - All services are lazily loaded. The pattern is to leverage lazy_static, i.e., define each service
//!   as a lazy_static, which will only be initialized on demand.
//!

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

/// provides standard interface for contracts to use at deployment time to run service related deployment
/// code
pub trait Deploy: Service {
    /// invoked when the contract is first deployed
    /// - main use case is to initialize any service state
    fn deploy<F>(initial_state_provider: Option<F>) -> Self
    where
        F: FnOnce() -> Self::State;
}

/// provide standard interface that is meant to be used to lazily initialize the service
/// - one of its basic functions is to load its state from storage
pub trait Init: Service {
    /// invoked when the contract is first used when the contract is invoked.
    ///
    /// The pattern is to define all your services via lazy_static, which will initialize the service
    /// the first time it is referenced.
    ///
    /// For example, the service may load some state from storage into memory. Then when the service
    /// is destroyed, it will store the state back to storage.
    ///
    fn init() -> Self;
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazy_static::lazy_static;
    use oysterpack_smart_near_test::*;
    use std::ops::DerefMut;
    use std::sync::Mutex;

    lazy_static! {
        static ref FOO: Mutex<Foo> = Mutex::new(Foo::init());
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

    impl Deploy for Foo {
        fn deploy<F>(initial_state_provider: Option<F>) -> Self
        where
            F: FnOnce() -> Self::State,
        {
            let state = Foo::new_state(0);
            state.save();
            Self { state }
        }
    }

    impl Init for Foo {
        fn init() -> Self {
            let state = Foo::load_state().expect("service is not deployed");
            Self { state }
        }
    }

    #[test]
    fn service() {
        // Arrange
        let ctx = new_context("bob");
        testing_env!(ctx);

        Foo::deploy();

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

//! This package provides a standard for building reusable smart contract stateful components
//!
//! ## Component Design
//! - Component declares it state type and defines a u128 based storage key
//!   - recommendation is to use ULID to generate the storage key
//! - Each component is responsible for managing its own state. This means when the component state
//!   changes it is the component's responsibility to save it to storage.
//! - [`crate::component::Deploy`] - defines a pattern to standardize component deployment

use crate::data::Object;
use near_sdk::borsh::{BorshDeserialize, BorshSerialize};
use std::fmt::Debug;

/// Defines abstraction for a stateful component
pub trait Component {
    type State: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq;

    /// Used to to store the service state to blockchain storage
    /// - it is recommended to generate a ULID for the key to avoid collisions
    fn state_key() -> u128;

    /// loads the service state from storage using the key defined by [`state_key`]()
    fn load_state() -> Option<ComponentState<Self::State>> {
        ComponentState::<Self::State>::load(&Self::state_key())
    }

    /// creates new in-memory state, i.e., the state is not persisted to storage
    fn new_state(state: Self::State) -> ComponentState<Self::State> {
        ComponentState::<Self::State>::new(Self::state_key(), state)
    }
}

/// service state type
pub type ComponentState<T> = Object<u128, T>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::Deploy;
    use lazy_static::lazy_static;
    use oysterpack_smart_near_test::*;
    use std::ops::DerefMut;
    use std::sync::Mutex;

    lazy_static! {
        static ref FOO: Mutex<Foo> = Mutex::new(Foo::new());
    }

    type FooState = ComponentState<u128>;
    struct Foo {
        state: FooState,
    }

    impl Component for Foo {
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

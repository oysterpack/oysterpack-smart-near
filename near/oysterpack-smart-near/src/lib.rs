//! Provides support for building OysterPack SMART NEAR smart contracts.

pub use crate::core::*;

pub mod component;
mod core;
pub mod data;
pub mod domain;

pub use near_sdk;

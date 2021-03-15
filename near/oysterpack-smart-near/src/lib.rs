//! Provides support for building OysterPack SMART NEAR smart contracts.

pub use crate::core::*;

pub mod component;
mod contract_context;
mod core;
pub mod data;
pub mod domain;

pub use contract_context::*;

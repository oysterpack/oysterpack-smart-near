//! Provides account management support for NEAR smart contracts

pub use domain::*;
pub use interface::*;

mod domain;
mod interface;
pub mod services;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}

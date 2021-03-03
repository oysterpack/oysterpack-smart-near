//! Provides account management support for NEAR smart contracts

mod account_manager;
mod domain;
mod interface;

pub use domain::*;
pub use interface::*;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}

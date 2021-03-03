mod account_manager;
pub mod domain;
mod interface;

pub use interface::*;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}

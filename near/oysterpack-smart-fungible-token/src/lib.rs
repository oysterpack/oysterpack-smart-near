pub mod components;
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

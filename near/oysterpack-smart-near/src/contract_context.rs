/// The pattern is to create the smart contract context in the contract initialization phase
/// leveraging the `#[borsh_init(init)]` lifecycle hook on the contract.
///
/// ## Notes
/// - the `Default` trait is required in order to create the contract instance
pub trait SmartContractContext: Default {
    /// Config that is used to build the context
    /// - if the context requires no config, then set it `()`
    type Config;

    /// constructor - will be called each time the contract is loaded into the VM, i.e., triggered
    /// by a contract function being invoked
    fn build(config: Self::Config) -> Self;

    /// meant to be run once as part of contract deployment
    fn deploy(_context: &mut Self) {}
}

/// Provides standard interface pattern for contracts to use at deployment time to run component
/// related deployment code.
///
/// For example, components may require config to initialize its persistent state.
pub trait Deploy {
    type Config;

    /// invoked when the contract is first deployed
    /// - main use case is to initialize any service state
    fn deploy(config: Self::Config);
}

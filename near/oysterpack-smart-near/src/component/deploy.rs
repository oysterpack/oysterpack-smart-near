/// Provides standard interface pattern for contracts to use at deployment time to run service related
/// deployment code.
/// - for example, service may require config to initialize its persistent state
///
/// ## NOTES
/// - components may not need to implement ['Deploy']
pub trait Deploy {
    type Config;

    /// invoked when the contract is first deployed
    /// - main use case is to initialize any service state
    fn deploy(config: Option<Self::Config>);
}

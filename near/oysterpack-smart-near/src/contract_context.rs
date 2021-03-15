/// Provides the context needed to execute contract functions
pub trait SmartContractContext {
    /// Config that is used to build the context
    /// - if the context requires no config, then set it `()`
    type Config;

    /// constructor
    fn build(config: Self::Config) -> Self;
}

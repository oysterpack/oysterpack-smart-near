use oysterpack_smart_near::domain::YoctoNear;

pub trait AccountManagement {
    /// returns the total number of accounts registered with the contract
    fn total_registered_accounts(&self) -> u128;

    /// returns the total NEAR balance that is owned by registered accounts
    fn total_accounts_near_balance(&self) -> YoctoNear;

    /// returns the total NEAR storage available balance that is owned by registered accounts
    fn total_accounts_storage_available_balance(&self) -> YoctoNear;
}

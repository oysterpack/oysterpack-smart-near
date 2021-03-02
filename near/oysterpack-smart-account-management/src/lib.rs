use near_sdk::json_types::ValidAccountId;
use oysterpack_smart_near::model::YoctoNear;

/// The primary use case for account registration is to address storage staking costs. This is an
/// issue that needs to be addressed by any multi-user contract that allocates storage for the user
/// on the blockchain. Storage staking costs are the most expensive costs to consider for the
/// contract on NEAR. If storage costs are not managed properly, then they can break the bank for
/// the contract.
///
/// On NEAR, the contract is responsible to pay for its long term persistent storage. Thus, multi-user
/// contracts should be designed to pass on storage costs to its user accounts. Contracts should
/// require accounts to pay for their own account storage when the account registers with the contract.
pub trait AccountRegistry {
    /// Used for accounts to register with a contract. This function supports 2 modes of account registration:
    /// 1. self registration (account_id is not specified): the function caller (predecessor account)
    ///    will be registered
    /// 2. third party registration (account_id is valid NEAR account ID): the function caller is
    ///    registering the specified account_id and paying the account registration fee on the
    ///    account's behalf
    ///
    /// Account registration fee is required and must be attached to the function all.
    /// - The main purpose for the registration fee is to pay for account storage allocation, however
    ///   additional fees may apply that are specific to the contract's business use case. The account
    ///   registration fee can be looked up on the contract via [`AccountRegistry::ar_registration_fee`].
    ///   Account registration fee overpayment will be refunded back to the predecessor account that
    ///   invoked the function.
    /// - If the account_id is specified, then the predecessor account will be responsible for paying
    ///   the registration fee on behalf of the account being registered.
    ///
    /// ## Returns
    /// - true if the account was successfully registered
    /// - false if the account was already registered - the attached deposit will be refunded
    ///
    /// ## Pancis
    /// - if the attached deposit cannot cover the registration fee
    /// - if `account_id` is not a valid NEAR account ID
    fn ar_register(&mut self, account_id: Option<ValidAccountId>) -> bool;

    /// Used to check if the account is registered with the contract.
    ///
    /// ## Returns
    /// - true if the specified account_id is registered with the contract.
    /// - false if the specified account_id is not registered with the contract.
    ///
    /// ## Panics
    /// If account_id is not a valid NEAR account ID
    fn ar_is_registered(account_id: ValidAccountId) -> bool;

    /// Used to lookup the account registration fee that is required for registering an account with
    /// the contract.
    fn ar_registration_fee(&self) -> YoctoNear;
}

pub struct AccountManager {}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}

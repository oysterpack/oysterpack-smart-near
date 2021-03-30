use crate::{StorageBalance, StorageBalanceBounds};
use oysterpack_smart_near::domain::YoctoNear;
use oysterpack_smart_near::near_sdk::json_types::ValidAccountId;
use oysterpack_smart_near::ErrCode;

/// # **Contract Interface**: [Account Storage API][3]
///
/// [Storage staking][1] is an issue that needs to be addressed by any multi-user contract that allocates storage for the
/// user on the blockchain. Storage staking costs are the most expensive costs to consider for the contract on NEAR. If storage
/// costs are not managed properly, then they can [break the bank][2] for the contract.
///
/// On NEAR, the contract is responsible to pay for its long term persistent storage. Thus, multi-user contracts should be
/// designed to pass on storage costs to its user accounts. The account storage API provides the following:
/// 1. It enables accounts to lookup the minimum required account storage balance for the initial deposit in order to be able
///    to use the contract.
/// 2. It enables accounts to deposit NEAR funds into the contract to pay for storage for either itself or on behalf of another
///    account. The initial deposit for the account must be at least the minimum amount required by the contract.
/// 3. Account storage total and available balance can be looked up. The amount required to pay for the account's storage usage
///    will be locked up in the contract. Any storage balance above storage staking costs is available for withdrawal.
///
/// [1]: https://docs.near.org/docs/concepts/storage#how-much-does-it-cost
/// [2]: https://docs.near.org/docs/concepts/storage#the-million-cheap-data-additions-attack
/// [3]: https://nomicon.io/Standards/StorageManagement.html
pub trait StorageManagement {
    /// Used by accounts to deposit funds to pay for account storage staking fees.
    ///
    /// This function supports 2 deposit modes:
    /// 1. **self deposit** (`account_id` is not specified): predecessor account is used as the account
    /// 2. **third party deposit** (`account_id` is valid NEAR account ID):  the function caller is
    ///    depositing NEAR funds for the specified `account_id`
    ///    
    /// - If this is the initial deposit for the account, then the deposit must be enough to cover the
    ///   minimum required balance.
    /// - If the attached deposit is more than the required minimum balance, then the funds are credited
    ///   to the account storage available balance.
    /// - If `registration_only=true`, contract MUST refund above the minimum balance if the account
    ///   wasn't registered and refund full deposit if already registered.
    ///  - Any attached deposit in excess of `storage_balance_bounds.max` must be refunded to predecessor account.
    ///
    /// ## Example Use Cases
    ///  1. In order for the account to hold tokens, the account must first have enough NEAR funds
    ///     deposited into the token contract to pay for the account's storage staking fees. The account
    ///     can deposit NEAR funds for itself into the token contract, or another contract might have
    ///     deposited NEAR funds into the token contract on the account's behalf to pay for the account's
    ///     storage staking fees.
    ///  2. Account's may use the blockchain to store data that grows over time. The account can use
    ///     this API to deposit additional funds to pay for additional account storage usage growth.
    ///
    /// ## Arguments
    /// - `account_id` - optional NEAR account ID. If not specified, then predecessor account ID will be used.
    ///
    /// ## Returns
    /// The account's updated storage balance.
    ///
    /// ## Panics
    /// - If the attached deposit is less than the minimum required account storage fee on the initial deposit.
    /// - If `account_id` is not a valid NEAR account ID
    ///
    /// `#[payable]`
    fn storage_deposit(
        &mut self,
        account_id: Option<ValidAccountId>,
        registration_only: Option<bool>,
    ) -> StorageBalance;

    /// Used to withdraw NEAR from the predecessor account's storage available balance.
    /// If amount is not specified, then all of the account's storage available balance will be withdrawn.
    ///
    /// - The attached yoctoNEAR will be refunded with the withdrawal transfer.
    /// - The account is required to attach exactly 1 yoctoNEAR to the function call to prevent
    ///   restricted function-call access-key call.
    /// - If the withdrawal amount is zero, then the 1 yoctoNEAR attached deposit is not refunded
    ///   because it would cost more to send the refund
    ///
    /// ## Arguments
    /// - `amount` - the amount to withdraw from the account's storage available balance expressed in yoctoNEAR
    ///amount: Option<YoctoNear>
    /// ## Returns
    /// The account's updated storage balance.
    ///
    /// ## Panics
    /// - If the attached deposit does not equal 1 yoctoNEAR
    /// - If the account is not registered
    /// - If the specified withdrawal amount is greater than the account's available storage balance
    ///
    /// `#[payable]`
    fn storage_withdraw(&mut self, amount: Option<YoctoNear>) -> StorageBalance;

    /// Unregisters the predecessor account and returns the storage NEAR deposit.
    ///
    /// If `force=true` the function SHOULD ignore existing account data, such as non-zero balances
    /// on an FT contract (that is, it should burn such balances), and close the account.
    /// Otherwise, it MUST panic if caller has existing account data, such as a positive registered
    /// balance (eg token holdings) or if the contract doesn't support forced unregistration.
    ///
    /// **NOTE**: function requires exactly 1 yoctoNEAR attached balance to prevent restricted function-call
    /// access-key call (UX wallet security)
    ///
    /// ## Returns
    /// - true if the account was successfully unregistered
    /// - false indicates that the account is not registered with the contract
    ///
    /// ## Panics
    /// - if exactly 1 yoctoNEAR is not attached
    ///
    /// `#[payable]`
    fn storage_unregister(&mut self, force: Option<bool>) -> bool;

    /// Returns minimum and maximum allowed balance amounts to interact with this contract.
    fn storage_balance_bounds(&self) -> StorageBalanceBounds;

    /// Used to lookup the account storage balance for the specified account.
    ///
    /// Returns None if the account is not registered with the contract
    fn storage_balance_of(&self, account_id: ValidAccountId) -> Option<StorageBalance>;
}

pub const ERR_CODE_UNREGISTER_FAILURE: ErrCode = ErrCode("UNREGISTER_FAILURE");

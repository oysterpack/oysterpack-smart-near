use crate::{AccountIdHash, StorageUsageBounds};
use lazy_static::lazy_static;
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env,
    json_types::ValidAccountId,
    serde::{Deserialize, Serialize},
};
use oysterpack_smart_near::domain::StorageUsageChange;
use oysterpack_smart_near::{domain::YoctoNear, eventbus::*, Level, LogEvent};
use std::fmt::{self, Display, Formatter};
use std::sync::Mutex;

/// # Account Storage API
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
/// ### Out of Scope
/// - Managing funds for purposes other than account storage is outside the scope of this API.
/// - The contract should account for changes in price for storage on the NEAR blockchain over time. However, how it does so is outside the scope of this contract.
///
/// [1]: https://docs.near.org/docs/concepts/storage#how-much-does-it-cost
/// [2]: https://docs.near.org/docs/concepts/storage#the-million-cheap-data-additions-attack
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
    ///
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

/// Tracks account storage balance
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy)]
#[serde(crate = "near_sdk::serde")]
pub struct StorageBalance {
    /// total NEAR funds that is purposed to pay for account storage
    pub total: YoctoNear,
    /// the amount of NEAR funds that are available for withdrawal from the account's storage balance
    pub available: YoctoNear,
}

/// Defines storage balance bounds for the contract
#[derive(
    BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, PartialEq, Clone, Copy,
)]
#[serde(crate = "near_sdk::serde")]
pub struct StorageBalanceBounds {
    /// the minimum balance that must be maintained for storage by the account on the contract
    /// - it is the amount of tokens required to start using this contract at all, e.g., to register with the contract
    pub min: YoctoNear,
    /// the maximum storage balance that is permitted
    ///
    /// A contract may implement `max` equal to `min` if it only charges for initial registration,
    /// and does not adjust per-user storage over time. A contract which implements `max` must
    /// refund deposits that would increase a user's storage balance beyond this amount.
    pub max: Option<YoctoNear>,
}

impl From<StorageUsageBounds> for StorageBalanceBounds {
    fn from(bounds: StorageUsageBounds) -> Self {
        let storage_byte_cost = env::storage_byte_cost();
        Self {
            min: (storage_byte_cost * bounds.min.value() as u128).into(),
            max: bounds
                .max
                .map(|max| (storage_byte_cost * max.value() as u128).into()),
        }
    }
}

/// Account storage related events
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum AccountStorageEvent {
    /// an account was registered
    Registered(StorageBalance),
    // an account made a deposit
    Deposit(YoctoNear),
    /// an account made a withdrawal from its storage available balance
    Withdrawal(YoctoNear),
    /// account storage usage changed
    StorageUsageChanged(AccountIdHash, StorageUsageChange),
    /// an account was unregistered
    /// - its NEAR balance was refunded
    Unregistered(YoctoNear),
}

impl Display for AccountStorageEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            AccountStorageEvent::StorageUsageChanged(_, change) => write!(f, "{:?}", change),
            _ => write!(f, "{:?}", self),
        }
    }
}

const ACCOUNT_STORAGE_EVENT: LogEvent = LogEvent(Level::INFO, "AccountStorageEvent");

impl AccountStorageEvent {
    pub fn log(&self) {
        ACCOUNT_STORAGE_EVENT.log(self.to_string());
    }
}

// TODO: create macro to generate boilerplate code for event: #[event]
lazy_static! {
    static ref ACCOUNT_STORAGE_EVENTS: Mutex<EventHandlers<AccountStorageEvent>> =
        Mutex::new(EventHandlers::new());
}

impl Event for AccountStorageEvent {
    fn handlers<F>(f: F)
    where
        F: FnOnce(&EventHandlers<Self>),
    {
        f(&*ACCOUNT_STORAGE_EVENTS.lock().unwrap())
    }

    fn handlers_mut<F>(f: F)
    where
        F: FnOnce(&mut EventHandlers<Self>),
    {
        f(&mut *ACCOUNT_STORAGE_EVENTS.lock().unwrap())
    }
}

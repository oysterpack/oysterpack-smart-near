use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    json_types::ValidAccountId,
    serde::{Deserialize, Serialize},
    AccountId,
};
use oysterpack_smart_near::domain::YoctoNear;
use oysterpack_smart_near::{ErrCode, ErrorConst, Level, LogEvent};

/// # **Contract Interface**: Contract Ownership API
/// Every contract has an owner
pub trait ContractOwnership {
    /// returns the contract owner account ID
    fn owner() -> AccountId;

    /// Returns how much of the contract's NEAR balance is owned by the contract owner and what amount
    /// is available for withdrawal.
    ///
    /// The owner must retain enough NEAR deposit on the contract to cover storage costs that are
    /// the contract's responsibility to pay for, i.e., excluding account storage.
    fn owner_balance() -> ContractOwnerNearBalance;

    /// Returns the prospective owner that the transfer is waiting on for finalization.
    ///
    /// Returns None if there is no ownership transfer in progress.
    fn prospective_owner() -> Option<AccountId>;

    /// Initiates the workflow to transfer contract ownership.
    ///
    /// The ownership transfer is not finalized until the new owner finalizes the transfer.
    /// This avoids transferring transferring contract ownership to a non-existent account ID.
    ///
    /// ## NOTES
    /// - any open contract sale is cancelled
    /// - any active bid is cancelled
    ///
    /// ## Log Event
    /// [`LOG_EVENT_CONTRACT_TRANSFER_INITIATED`]
    ///
    /// ## Panics
    /// - `ERR_OWNER_ACCESS_REQUIRED` - if the predecessor account is not the owner account
    /// - `ERR_YOCTONEAR_DEPOSIT_REQUIRED` - if 1 yoctoNEAR is not attached
    /// - if the new owner account ID is not valid
    ///
    /// `#[payable]` - requires exactly 1 yoctoNEAR to be attached
    fn transfer_ownership(&mut self, new_owner: ValidAccountId);

    /// Enables the transfer to be cancelled before it is finalized.
    ///
    /// The transfer can only be cancelled by both the current owner and the prospective owner.
    ///
    /// ## Log Event
    /// [`LOG_EVENT_CONTRACT_TRANSFER_CANCELLED`]
    ///
    /// ## Panics
    /// - `ERR_CURRENT_OR_PROSPECTIVE_OWNER_ACCESS_REQUIRED` - if the predecessor account is not the current or prospective owner account
    /// - `ERR_YOCTONEAR_DEPOSIT_REQUIRED` - if 1 yoctoNEAR is not attached
    ///
    /// `#[payable]` - requires exactly 1 yoctoNEAR to be attached
    fn cancel_ownership_transfer(&mut self);

    /// Used to finalize the contract transfer to the new prospective owner
    ///
    /// ## Notes
    /// This effectively transfers any owner balance to the new owner. The owner can withdraw
    /// from its available balance before the transfer is finalized.
    ///
    /// ## Log Event
    /// [`LOG_EVENT_CONTRACT_TRANSFER_FINALIZED`]
    ///
    /// ## Panics
    /// - `ERR_PROSPECTIVE_OWNER_ACCESS_REQUIRED` - if the predecessor ID is not the new prospective owner
    /// - `ERR_CONTRACT_OWNER_TRANSFER_NOT_INITIATED` - if there is no contract ownership transfer in progress
    /// - `ERR_YOCTONEAR_DEPOSIT_REQUIRED` - if 1 yoctoNEAR is not attached
    ///
    /// `#[payable]` - requires exactly 1 yoctoNEAR to be attached
    fn finalize_ownership_transfer(&mut self);

    /// Used by the contract owner to withdraw from the contract owner's available balance.
    ///
    /// If `amount` is None, then all available balance is withdrawn.
    ///
    /// Returns the updated contract owner NEAR balance.
    ///
    /// ## Panics
    /// - `ERR_OWNER_ACCESS_REQUIRED` - if the predecessor account is not the owner account
    /// - `ERR_YOCTONEAR_DEPOSIT_REQUIRED` - if 1 yoctoNEAR is not attached
    /// - `ERR_OWNER_BALANCE_OVERDRAW` - if there are insufficient funds to fulfill the request
    /// - `ERR_CODE_BAD_REQUEST` - if specified amount is zero
    ///
    /// `#[payable]` - requires exactly 1 yoctoNEAR to be attached
    fn withdraw_owner_balance(&mut self, amount: Option<YoctoNear>) -> ContractOwnerNearBalance;
}

/// log event for [`ContractOwnership::transfer_ownership`]
pub const LOG_EVENT_CONTRACT_TRANSFER_INITIATED: LogEvent =
    LogEvent(Level::INFO, "CONTRACT_TRANSFER_INITIATED");

/// log event for [`ContractOwnership::cancel_ownership_transfer`]
pub const LOG_EVENT_CONTRACT_TRANSFER_CANCELLED: LogEvent =
    LogEvent(Level::INFO, "CONTRACT_TRANSFER_CANCELLED");

/// log event for [`ContractOwnership::finalize_ownership_transfer`]
pub const LOG_EVENT_CONTRACT_TRANSFER_FINALIZED: LogEvent =
    LogEvent(Level::INFO, "CONTRACT_TRANSFER_FINALIZED");

/// Contract owner total and available balance
#[derive(
    BorshSerialize, BorshDeserialize, Deserialize, Serialize, Clone, Debug, PartialEq, Default,
)]
#[serde(crate = "near_sdk::serde")]
pub struct ContractOwnerNearBalance {
    pub total: YoctoNear,
    pub available: YoctoNear,
}

/// Indicates access was denied because owner access was required
pub const ERR_OWNER_ACCESS_REQUIRED: ErrorConst = ErrorConst(
    ErrCode("OWNER_ACCESS_REQUIRED"),
    "action requires owner access",
);

/// Indicates access was denied because prospective owner access was required
pub const ERR_PROSPECTIVE_OWNER_ACCESS_REQUIRED: ErrorConst = ErrorConst(
    ErrCode("PROSPECTIVE_OWNER_ACCESS_REQUIRED"),
    "action requires prospective owner access",
);

pub const ERR_CONTRACT_OWNER_TRANSFER_NOT_INITIATED: ErrorConst = ErrorConst(
    ErrCode("CONTRACT_OWNER_TRANSFER_NOT_INITIATED"),
    "contract ownership transfer has not been initiated",
);

/// Indicates access was denied because current or prospective owner access was required
pub const ERR_CURRENT_OR_PROSPECTIVE_OWNER_ACCESS_REQUIRED: ErrorConst = ErrorConst(
    ErrCode("CURRENT_OR_PROSPECTIVE_OWNER_ACCESS_REQUIRED"),
    "action requires current or prospective owner access",
);

pub const ERR_OWNER_BALANCE_OVERDRAW: ErrorConst = ErrorConst(
    ErrCode("OWNER_BALANCE_OVERDRAW"),
    "owner balance is insufficient to fulfill withdrawal",
);

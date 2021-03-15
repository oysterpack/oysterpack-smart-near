use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    json_types::ValidAccountId,
    serde::{Deserialize, Serialize},
    AccountId,
};
use oysterpack_smart_near::domain::YoctoNear;
use oysterpack_smart_near::{ErrCode, ErrorConst, Level, LogEvent};

/// Every contract has an owner
pub trait ContractOwnership {
    /// checks if the account ID is the current contract owner
    /// - account ID is not specified, then the predecessor ID is used
    fn owner(&self) -> AccountId;

    /// Initiates the workflow to transfer contract ownership.
    ///
    /// The ownership transfer is not finalized until the new owner finalizes the transfer.
    /// This avoids transferring transferring contract ownership to a non-existent account ID.
    ///
    /// ## Panics
    /// - if the predecessor account is not the owner account
    /// - if 1 yoctoNEAR is not attached
    /// - if the new owner account ID is not valid
    /// - if contract is for sale
    ///
    /// `#[payable]` - requires exactly 1 yoctoNEAR to be attached
    fn transfer_ownership(&mut self, new_owner: ValidAccountId);

    /// Enables the transfer to be cancelled before it is finalized.
    ///
    /// The transfer can only be cancelled by both the current owner and the prospective owner.
    ///
    /// ## Panics
    /// - if the predecessor account is not the current or prospective owner account
    /// - if 1 yoctoNEAR is not attached
    ///
    /// `#[payable]` - requires exactly 1 yoctoNEAR to be attached
    fn cancel_ownership_transfer(&mut self);

    /// Returns true if the specified account ID is the prospective owner that the transfer is waiting
    /// on for finalization.
    ///
    /// Returns false if there is no ownership transfer in progress.
    fn is_prospective_owner(&self, account_id: ValidAccountId) -> bool;

    /// Used to finalize the contract transfer to the new prospective owner
    ///
    /// When the transfer is finalized, any current owner balance is transferred to the previous
    /// owner account.
    ///
    /// ## Panics
    /// - if the predecessor ID is not the new prospective owner
    /// - if there is no contract ownership transfer in progress
    /// - if 1 yoctoNEAR is not attached
    ///
    /// `#[payable]` - requires exactly 1 yoctoNEAR to be attached
    fn finalize_transfer_ownership(&mut self);

    /// Used by the contract owner to withdraw from the contract owner's available balance.
    ///
    /// If `amount` is None, then all available balance is withdrawn.
    ///
    /// Returns the updated contract owner NEAR balance.
    ///
    /// ## Panics
    /// - if the predecessor account is not the owner account
    /// - if 1 yoctoNEAR is not attached
    /// - if there are insufficient funds to fulfill the request
    ///
    /// `#[payable]` - requires exactly 1 yoctoNEAR to be attached
    fn withdraw_owner_balance(&mut self, amount: Option<YoctoNear>) -> ContractOwnerNearBalance;

    fn owner_balance(&self) -> ContractOwnerNearBalance;
}

/// log event for [`ContractOwnership::transfer_ownership`]
pub const LOG_EVENT_CONTRACT_TRANSFER_INITIATED: LogEvent =
    LogEvent(Level::INFO, "CONTRACT_TRANSFER_INITIATED");

/// log event for [`ContractOwnership::cancel_transfer_ownership`]
pub const LOG_EVENT_CONTRACT_TRANSFER_CANCELLED: LogEvent =
    LogEvent(Level::INFO, "CONTRACT_TRANSFER_CANCELLED");

/// log event for [`ContractOwnership::finalize_transfer_ownership`]
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

/// Indicates access was denied because current or prospective owner access was required
pub const ERR_CURRENT_OR_PROSPECTIVE_OWNER_ACCESS_REQUIRED: ErrorConst = ErrorConst(
    ErrCode("CURRENT_OR_PROSPECTIVE_OWNER_ACCESS_REQUIRED"),
    "action requires current or prospective owner access",
);

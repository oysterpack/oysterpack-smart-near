use crate::{ContractBid, ContractOwner, ContractOwnerObject};
use near_sdk::{env, json_types::ValidAccountId};
use oysterpack_smart_near::domain::{Expiration, YoctoNear};
use oysterpack_smart_near::{ErrCode, ErrorConst, Level, LogEvent};

/// Enables the contract to be transferred to a new owner via a sale.
///
/// ## NOTES
/// - NEAR funds are never transferred externally as part of the transaction. NEAR funds are transferred
///   internally between transacting accounts that are registered on the contract.
/// - Accounts must initiate NEAR fund withdrawals themselves via the NEAR standard `StorageManagement`
///   interface (NEP-145). Thus, in order to transact the accounts must be registered with the contract.
pub trait ContractSale {
    /// Returns None if the contract is not listed for sale
    fn contract_sale_price(&self) -> Option<YoctoNear> {
        ContractOwnerObject::load().contract_sale_price()
    }

    /// Returns None if there is no current bid on the contract
    fn contract_bid(&self) -> Option<ContractBid> {
        ContractOwnerObject::load().bid().map(|bid| bid.1)
    }

    /// Checks if the specified account ID has the current highest bid
    /// - if account ID is not specified, then the predecessor ID is used
    /// - if there is no current bid, then None is returned
    fn is_highest_bidder(&self, account_id: Option<ValidAccountId>) -> Option<bool> {
        ContractOwnerObject::load()
            .bid()
            .map(|bid| bid.0)
            .map(|bidder| match account_id {
                None => bidder == env::predecessor_account_id().into(),
                Some(account_id) => bidder == account_id.into(),
            })
    }

    /// Puts up the contract for sale for the specified sale price.
    ///
    /// - If the contract is already for sale, then the sale price is updated to the new price.
    /// - If there already is a higher bid price, then the contract is sold for the bid price.
    ///
    /// ## Panics
    /// - if the predecessor account is not the owner account
    /// - if 1 yoctoNEAR is not attached
    /// - if `price` is zero
    /// - if contract transfer is in progress
    ///
    /// `#[payable]` - requires exactly 1 yoctoNEAR to be attached
    fn sell_contract(&mut self, price: YoctoNear);

    /// Takes the contract off the market for selling.
    ///
    /// If the contract is not currently up for sale, then there is no effect.
    ///
    /// ## Panics
    /// - if the predecessor account is not the owner account
    /// - if 1 yoctoNEAR is not attached
    ///
    /// `#[payable]` - requires exactly 1 yoctoNEAR to be attached
    fn cancel_contract_sell_order(&mut self);

    /// Places an order to buy the contract for the specified bid. The bid is the sum of the attached
    /// amount plus the specified amount from the bidder's contract account.
    ///
    /// - If there is no current sale price set, then this places a bid on the contract.
    /// - If the bid is greater than or equal to the sale price, then the contract is sold at the
    ///   bid price.
    /// - The buyer may set an optional expiration on the bid
    /// - If there was a previous lower bid in effect, then that buy order will be automatically
    ///   cancelled and the funds will be transferred back to the buyer's registered contract account.
    ///
    /// ## Panics
    /// - if the predecessor account is the owner account
    /// - if no deposit is attached - at lease 1 yoctoNEAR must be attached
    /// - if the submitted bid price is not higher than the current bid price
    ///
    /// `#[payable]`
    fn buy_contract(
        &mut self,
        expiration: Option<Expiration>,
        from_contract_balance: Option<YoctoNear>,
    );

    /// Cancels the buy order and withdraws the bid amount.
    ///
    /// ## Panics
    /// - if the predecessor account is not the current buyer
    /// - if 1 yoctoNEAR is not attached
    ///
    /// `#[payable]` - requires exactly 1 yoctoNEAR to be attached
    fn cancel_contract_buy_order(&mut self);
}

/// log event for [`ContractSale::sell_contract`]
/// - event gets logged each time the sale price is changed
pub const LOG_EVENT_CONTRACT_FOR_SALE: LogEvent = LogEvent(Level::INFO, "CONTRACT_FOR_SALE");

/// log event for [`ContractSale::cancel_contract_sell_order`]
pub const LOG_EVENT_CONTRACT_SALE_CANCELLED: LogEvent =
    LogEvent(Level::INFO, "CONTRACT_SALE_CANCELLED");

/// log event for [`ContractSale::buy_contract`]
pub const LOG_EVENT_CONTRACT_BID_PLACED: LogEvent = LogEvent(Level::INFO, "CONTRACT_BID_PLACED");

/// log event for [`ContractSale::cancel_contract_buy_order`]
pub const LOG_EVENT_CONTRACT_BID_CANCELLED: LogEvent =
    LogEvent(Level::INFO, "CONTRACT_BID_CANCELLED");

/// log event for expired bids which are automatically cancelled
pub const LOG_EVENT_CONTRACT_BID_EXPIRED: LogEvent = LogEvent(Level::INFO, "CONTRACT_BID_EXPIRED");

/// log event for contract sale transactions
pub const LOG_EVENT_CONTRACT_SOLD: LogEvent = LogEvent(Level::INFO, "CONTRACT_SOLD");

/// Indicates access was denied because owner access was required
pub const ERR_CONTRACT_SALE_PRICE_MUST_NOT_BE_ZERO: ErrorConst = ErrorConst(
    ErrCode("CONTRACT_SALE_PRICE_MUST_NOT_BE_ZERO"),
    "contract sale price must not be zero",
);

/// Indicates access was denied because owner access was required
pub const ERR_CONTRACT_BID_BALANCE_MISMATCH: ErrorConst = ErrorConst(
    ErrCode("CONTRACT_BID_BALANCE_MISMATCH"),
    "contract bid price did not match the NEAR balance",
);

/// Indicates access was denied because owner access was required
pub const ERR_CONTRACT_SALE_NOT_ALLOWED: ErrCode = ErrCode("CONTRACT_SALE_NOT_ALLOWED");

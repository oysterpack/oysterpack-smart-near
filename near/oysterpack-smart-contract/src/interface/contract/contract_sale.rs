use crate::ContractBid;
use near_sdk::{
    serde::{Deserialize, Serialize},
    AccountId,
};
use oysterpack_smart_near::domain::{ExpirationSetting, YoctoNear};
use oysterpack_smart_near::{ErrCode, ErrorConst, Level, LogEvent};

/// # **Contract Interface**: Contract Sale API
/// Enables the contract to be transferred to a new owner via a sale.
///
/// When the sale transaction is executed, the sale amount will be released to the current owner and
/// all of the owner's balance will be transferred out of the contract to the owner's NEAR account.
///
/// TODO: enable buyers to pay with STAKE
pub trait ContractSale {
    /// Returns None if the contract is not listed for sale
    fn ops_contract_sale_price() -> Option<YoctoNear>;

    /// Returns None if there is no current bid on the contract
    fn ops_contract_bid() -> Option<ContractBuyerBid>;

    /// Puts up the contract for sale for the specified sale price.
    ///
    /// - If the contract is already for sale, then the sale price is updated to the new price.
    /// - If there already is a higher bid price, then the contract is sold for the bid price.
    /// - If the current bid is expired, then the bid is cancelled
    ///
    /// ## Log Events
    /// - [`LOG_EVENT_CONTRACT_FOR_SALE`]
    /// - [`LOG_EVENT_CONTRACT_BID_CANCELLED`] - if current bid has expired
    /// = [`LOG_EVENT_CONTRACT_SOLD`] - if the current bid is >= the sale price
    ///
    /// ## Panics
    /// - if the predecessor account is not the owner account
    /// - if 1 yoctoNEAR is not attached
    /// - if `price` is zero
    /// - if contract transfer is in progress
    ///
    /// `#[payable]` - requires exactly 1 yoctoNEAR to be attached
    fn ops_contract_sell(&mut self, price: YoctoNear);

    /// Takes the contract off the market for selling.
    ///
    /// If the contract is not currently up for sale, then there is no effect.
    ///
    /// ## Panics
    /// - if the predecessor account is not the owner account
    /// - if 1 yoctoNEAR is not attached
    ///
    /// `#[payable]` - requires exactly 1 yoctoNEAR to be attached
    fn ops_contract_cancel_sale(&mut self);

    /// Places an order to buy the contract for the specified bid.
    ///
    /// - If there is no current sale price set, then this places a bid on the contract.
    /// - If the bid is greater than or equal to the sale price, then the contract is sold at the
    ///   bid price.
    /// - The buyer may set an optional expiration on the bid
    /// - If there was a previous lower bid in effect, then that buy order will be automatically
    ///   cancelled and the funds will be transferred back to the buyer's registered contract account.
    ///
    /// ## Log Events
    /// - [`LOG_EVENT_CONTRACT_BID_PLACED`]
    ///
    /// ## Panics
    /// - if no deposit is attached - at lease 1 yoctoNEAR must be attached
    /// - if the submitted bid price is not higher than the current bid price
    ///
    /// `#[payable]`
    fn ops_contract_buy(&mut self, expiration: Option<ExpirationSetting>);

    /// Enables the buyer to raise the contract bid and update the expiration.
    ///
    /// ## Panics
    /// - if there is no current bid
    /// - if predecessor ID is not the current buyer
    /// - if no deposit is attached - at lease 1 yoctoNEAR must be attached
    ///
    /// `#[payable]`
    fn ops_contract_raise_bid(&mut self, expiration: Option<ExpirationSetting>) -> ContractBid;

    /// Enables the buyer to lower the contract bid by the specified amount and update the expiration.
    ///
    /// The amount will be refunded back to the buyer + the 1 yoctoNEAR attached deposit
    ///
    /// ## Panics
    /// - if there is no current bid
    /// - if predecessor ID is not the current buyer
    /// - if the current bid is <= amount
    /// - if 1 yoctoNEAR deposit is not attached
    ///
    /// `#[payable]` - requires exactly 1 yoctoNEAR to be attached
    fn ops_contract_lower_bid(
        &mut self,
        amount: YoctoNear,
        expiration: Option<ExpirationSetting>,
    ) -> ContractBid;

    /// Enables the buyer to update the expiration.
    ///
    /// ## Panics
    /// - if there is no current bid
    /// - if predecessor ID is not the current buyer
    /// - if 1 yoctoNEAR deposit is not attached
    ///
    /// `#[payable]` - requires exactly 1 yoctoNEAR to be attached
    fn ops_contract_update_bid_expiration(&mut self, expiration: ExpirationSetting);

    /// Enables the buyer to clear the expiration.
    ///
    /// ## Panics
    /// - if there is no current bid
    /// - if predecessor ID is not the current buyer
    /// - if 1 yoctoNEAR deposit is not attached
    ///
    /// `#[payable]` - requires exactly 1 yoctoNEAR to be attached
    fn ops_contract_clear_bid_expiration(&mut self);

    /// Cancels the buy order and withdraws the bid amount.
    ///
    /// ## Panics
    /// - if the predecessor account is not the current buyer
    /// - if 1 yoctoNEAR is not attached
    ///
    /// `#[payable]` - requires exactly 1 yoctoNEAR to be attached
    fn ops_contract_cancel_bid(&mut self);
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(crate = "near_sdk::serde")]
pub struct ContractBuyerBid {
    pub buyer: AccountId,
    pub bid: ContractBid,
}

/// event gets logged each time the sale price is changed
pub const LOG_EVENT_CONTRACT_FOR_SALE: LogEvent = LogEvent(Level::INFO, "CONTRACT_FOR_SALE");

pub const LOG_EVENT_CONTRACT_SALE_CANCELLED: LogEvent =
    LogEvent(Level::INFO, "CONTRACT_SALE_CANCELLED");

pub const LOG_EVENT_CONTRACT_BID_PLACED: LogEvent = LogEvent(Level::INFO, "CONTRACT_BID_PLACED");

pub const LOG_EVENT_CONTRACT_BID_RAISED: LogEvent = LogEvent(Level::INFO, "CONTRACT_BID_RAISED");

pub const LOG_EVENT_CONTRACT_BID_LOWERED: LogEvent = LogEvent(Level::INFO, "CONTRACT_BID_LOWERED");

pub const LOG_EVENT_CONTRACT_BID_EXPIRATION_CHANGE: LogEvent =
    LogEvent(Level::INFO, "CONTRACT_BID_EXPIRATION_CHANGE");

pub const LOG_EVENT_CONTRACT_BID_CANCELLED: LogEvent =
    LogEvent(Level::INFO, "CONTRACT_BID_CANCELLED");

pub const LOG_EVENT_CONTRACT_SOLD: LogEvent = LogEvent(Level::INFO, "CONTRACT_SOLD");

/// Indicates access was denied because owner access was required
pub const ERR_CONTRACT_SALE_PRICE_MUST_NOT_BE_ZERO: ErrorConst = ErrorConst(
    ErrCode("CONTRACT_SALE_PRICE_MUST_NOT_BE_ZERO"),
    "contract sale price must not be zero",
);

/// Indicates the bid was too low, i.e., a higher bid has already been placed
pub const ERR_CONTRACT_BID_TOO_LOW: ErrorConst = ErrorConst(
    ErrCode("CONTRACT_BID_NOT_ATTACHED"),
    "contract bid is too low - for your bid to be accepted, you must submit a bid that is higher than the current bid",
);

/// Indicates access was denied because owner access was required
pub const ERR_CONTRACT_SALE_NOT_ALLOWED: ErrCode = ErrCode("CONTRACT_SALE_NOT_ALLOWED");

/// The owner cannot submit a bid to buy the contract
pub const ERR_OWNER_CANNOT_BUY_CONTRACT: ErrorConst = ErrorConst(
    ErrCode("OWNER_CANNOT_BUY_CONTRACT"),
    "owner cannot submit a bid to buy the contract",
);

pub const ERR_NO_ACTIVE_BID: ErrorConst =
    ErrorConst(ErrCode("NO_ACTIVE_BID"), "there is no current active bid");

pub const ERR_ACCESS_DENIED_MUST_BE_BUYER: ErrorConst = ErrorConst(
    ErrCode("ACCESS_DENIED_MUST_BE_BUYER"),
    "action is restricted to current buyer",
);

pub const ERR_BID_IS_EXPIRED: ErrorConst = ErrorConst(ErrCode("BID_IS_EXPIRED"), "bid is expired");

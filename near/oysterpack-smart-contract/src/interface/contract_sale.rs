use oysterpack_smart_near::domain::{Expiration, YoctoNear};
use oysterpack_smart_near::{Level, LogEvent};

/// Enables the contract to be transferred to a new owner via a sale.
pub trait ContractSale {
    /// Returns None if the contract is not listed for sale
    fn contract_sale_price(&self) -> Option<YoctoNear>;

    /// Puts up the contract for sale for the specified sale price.
    ///
    /// - If the contract is already for sale, then the sale price is updated to the new price.
    /// - If there already is a higher bid price, then the contract is sold for the bid price.
    ///
    /// ## Panics
    /// - if the predecessor account is not the owner account
    /// - if 1 yoctoNEAR is not attached
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

    /// Places an order to buy the contract for the specified attached amount.
    ///
    /// - If there is no current sale price set, then this places a bid on the contract.
    /// - If the bid is greater than or equal to the sale price, then the contract is sold at the
    ///   bid price.
    /// - The buyer may set an optional expiration on the bid
    /// - If there was a previous lower bid in effect, then that buy order will be automatically
    ///   cancelles and the funds will be transferred back to the buyer
    ///
    /// ## Panics
    /// - if the predecessor account is the owner account
    /// - if no deposit is attached
    /// - if the submitted bid price is not higher than the current bid price
    ///
    /// `#[payable]`
    fn buy_contract(&mut self, expiration: Option<Expiration>);

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
pub const LOG_EVENT_CONTRACT_FOR_SALE: LogEvent = LogEvent(Level::INFO, "CONTRACT_FOR_SALE");

/// log event for [`ContractSale::cancel_contract_sell_order`]
pub const LOG_EVENT_CONTRACT_SALE_CANCELLED: LogEvent =
    LogEvent(Level::INFO, "CONTRACT_SALE_CANCELLED");

/// log event for [`ContractSale::buy_contract`]
pub const LOG_EVENT_CONTRACT_BID_PLACED: LogEvent = LogEvent(Level::INFO, "CONTRACT_BID_PLACED");

/// log event for [`ContractSale::cancel_contract_buy_order`]
pub const LOG_EVENT_CONTRACT_BID_CANCELLED: LogEvent =
    LogEvent(Level::INFO, "CONTRACT_BID_CANCELLED");

/// log event for contract sale transactions
pub const LOG_EVENT_CONTRACT_SOLD: LogEvent = LogEvent(Level::INFO, "CONTRACT_SOLD");

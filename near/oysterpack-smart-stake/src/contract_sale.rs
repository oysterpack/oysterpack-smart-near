use crate::*;
use oysterpack_smart_contract::components::contract_sale::ContractSaleComponent;
use oysterpack_smart_contract::{ContractBid, ContractBuyerBid, ContractSale};
use oysterpack_smart_near::domain::{ExpirationSetting, YoctoNear};

#[near_bindgen]
impl ContractSale for Contract {
    fn ops_contract_sale_price() -> Option<YoctoNear> {
        ContractSaleComponent::ops_contract_sale_price()
    }

    fn ops_contract_bid() -> Option<ContractBuyerBid> {
        ContractSaleComponent::ops_contract_bid()
    }

    #[payable]
    fn ops_contract_sell(&mut self, price: YoctoNear) {
        ContractSaleComponent.ops_contract_sell(price);
    }

    #[payable]
    fn ops_contract_cancel_sale(&mut self) {
        ContractSaleComponent.ops_contract_cancel_sale();
    }

    #[payable]
    fn ops_contract_buy(&mut self, expiration: Option<ExpirationSetting>) {
        ContractSaleComponent.ops_contract_buy(expiration)
    }

    #[payable]
    fn ops_contract_raise_bid(&mut self, expiration: Option<ExpirationSetting>) -> ContractBid {
        ContractSaleComponent.ops_contract_raise_bid(expiration)
    }

    #[payable]
    fn ops_contract_lower_bid(
        &mut self,
        amount: YoctoNear,
        expiration: Option<ExpirationSetting>,
    ) -> ContractBid {
        ContractSaleComponent.ops_contract_lower_bid(amount, expiration)
    }

    #[payable]
    fn ops_contract_update_bid_expiration(&mut self, expiration: ExpirationSetting) {
        ContractSaleComponent.ops_contract_update_bid_expiration(expiration)
    }

    #[payable]
    fn ops_contract_clear_bid_expiration(&mut self) {
        ContractSaleComponent.ops_contract_clear_bid_expiration()
    }

    #[payable]
    fn ops_contract_cancel_bid(&mut self) {
        ContractSaleComponent.ops_contract_cancel_bid()
    }
}

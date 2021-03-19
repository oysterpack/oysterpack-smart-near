use crate::*;
use oysterpack_smart_contract::components::contract_sale::ContractSaleComponent;
use oysterpack_smart_contract::{ContractBid, ContractBuyerBid, ContractSale};
use oysterpack_smart_near::domain::{ExpirationSetting, YoctoNear};

#[near_bindgen]
impl ContractSale for Contract {
    fn contract_sale_price() -> Option<YoctoNear> {
        ContractSaleComponent::contract_sale_price()
    }

    fn contract_bid() -> Option<ContractBuyerBid> {
        ContractSaleComponent::contract_bid()
    }

    fn sell_contract(&mut self, price: YoctoNear) {
        ContractSaleComponent.sell_contract(price);
    }

    fn cancel_contract_sale(&mut self) {
        ContractSaleComponent.cancel_contract_sale();
    }

    fn buy_contract(&mut self, expiration: Option<ExpirationSetting>) {
        ContractSaleComponent.buy_contract(expiration)
    }

    fn raise_contract_bid(&mut self, expiration: Option<ExpirationSetting>) -> ContractBid {
        ContractSaleComponent.raise_contract_bid(expiration)
    }

    fn lower_contract_bid(
        &mut self,
        amount: YoctoNear,
        expiration: Option<ExpirationSetting>,
    ) -> ContractBid {
        ContractSaleComponent.lower_contract_bid(amount, expiration)
    }

    fn update_contract_bid_expiration(&mut self, expiration: ExpirationSetting) {
        ContractSaleComponent.update_contract_bid_expiration(expiration)
    }

    fn clear_contract_bid_expiration(&mut self) {
        ContractSaleComponent.clear_contract_bid_expiration()
    }

    fn cancel_contract_bid(&mut self) {
        ContractSaleComponent.cancel_contract_bid()
    }
}

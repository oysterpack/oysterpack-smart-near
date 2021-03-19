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

    #[payable]
    fn sell_contract(&mut self, price: YoctoNear) {
        ContractSaleComponent.sell_contract(price);
    }

    #[payable]
    fn cancel_contract_sale(&mut self) {
        ContractSaleComponent.cancel_contract_sale();
    }

    #[payable]
    fn buy_contract(&mut self, expiration: Option<ExpirationSetting>) {
        ContractSaleComponent.buy_contract(expiration)
    }

    #[payable]
    fn raise_contract_bid(&mut self, expiration: Option<ExpirationSetting>) -> ContractBid {
        ContractSaleComponent.raise_contract_bid(expiration)
    }

    #[payable]
    fn lower_contract_bid(
        &mut self,
        amount: YoctoNear,
        expiration: Option<ExpirationSetting>,
    ) -> ContractBid {
        ContractSaleComponent.lower_contract_bid(amount, expiration)
    }

    #[payable]
    fn update_contract_bid_expiration(&mut self, expiration: ExpirationSetting) {
        ContractSaleComponent.update_contract_bid_expiration(expiration)
    }

    #[payable]
    fn clear_contract_bid_expiration(&mut self) {
        ContractSaleComponent.clear_contract_bid_expiration()
    }

    #[payable]
    fn cancel_contract_bid(&mut self) {
        ContractSaleComponent.cancel_contract_bid()
    }
}

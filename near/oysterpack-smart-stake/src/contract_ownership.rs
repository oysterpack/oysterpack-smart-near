use crate::*;
use near_sdk::{near_bindgen, AccountId};
use oysterpack_smart_contract::{ContractOwnerNearBalance, ContractOwnership};
use oysterpack_smart_near::domain::YoctoNear;

#[near_bindgen]
impl ContractOwnership for Contract {
    fn owner() -> AccountId {
        ContractOwnershipComponent::owner()
    }

    fn owner_balance() -> ContractOwnerNearBalance {
        ContractOwnershipComponent::owner_balance()
    }

    fn prospective_owner() -> Option<AccountId> {
        ContractOwnershipComponent::prospective_owner()
    }

    #[payable]
    fn transfer_ownership(&mut self, new_owner: ValidAccountId) {
        ContractOwnershipComponent.transfer_ownership(new_owner)
    }

    #[payable]
    fn cancel_ownership_transfer(&mut self) {
        ContractOwnershipComponent.cancel_ownership_transfer()
    }

    #[payable]
    fn finalize_ownership_transfer(&mut self) {
        ContractOwnershipComponent.finalize_ownership_transfer()
    }

    #[payable]
    fn withdraw_owner_balance(&mut self, amount: Option<YoctoNear>) -> ContractOwnerNearBalance {
        ContractOwnershipComponent.withdraw_owner_balance(amount)
    }
}

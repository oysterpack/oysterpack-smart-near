use crate::*;
use near_sdk::{near_bindgen, AccountId};
use oysterpack_smart_contract::{ContractOwnerNearBalance, ContractOwnership};
use oysterpack_smart_near::domain::YoctoNear;

#[near_bindgen]
impl ContractOwnership for Contract {
    fn ops_owner(&self) -> AccountId {
        ContractOwnershipComponent.ops_owner()
    }

    fn ops_owner_balance(&self) -> ContractOwnerNearBalance {
        ContractOwnershipComponent.ops_owner_balance()
    }

    fn ops_owner_prospective(&self) -> Option<AccountId> {
        ContractOwnershipComponent.ops_owner_prospective()
    }

    #[payable]
    fn ops_owner_transfer(&mut self, new_owner: ValidAccountId) {
        ContractOwnershipComponent.ops_owner_transfer(new_owner)
    }

    #[payable]
    fn ops_owner_cancel_transfer(&mut self) {
        ContractOwnershipComponent.ops_owner_cancel_transfer()
    }

    #[payable]
    fn ops_owner_finalize_transfer(&mut self) {
        ContractOwnershipComponent.ops_owner_finalize_transfer()
    }

    #[payable]
    fn ops_owner_withdraw_balance(
        &mut self,
        amount: Option<YoctoNear>,
    ) -> ContractOwnerNearBalance {
        ContractOwnershipComponent.ops_owner_withdraw_balance(amount)
    }
}

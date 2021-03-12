use crate::{ContractOwnerNearBalance, ContractOwnership};
use oysterpack_smart_near::domain::YoctoNear;

pub struct MultiUserContractComponent;

impl ContractOwnership for MultiUserContractComponent {
    fn withdraw_owner_balance(&mut self, _amount: Option<YoctoNear>) -> ContractOwnerNearBalance {
        unimplemented!()
    }

    fn owner_balance(&self) -> ContractOwnerNearBalance {
        unimplemented!()
    }
}

use crate::{
    ContractOwnerNearBalance, ContractOwnerObject, ContractOwnership,
    ContractOwnershipAccountIdsObject,
};
use near_sdk::json_types::ValidAccountId;
use oysterpack_smart_near::asserts::assert_yocto_near_attached;
use oysterpack_smart_near::component::Deploy;
use oysterpack_smart_near::domain::YoctoNear;
use oysterpack_smart_near_test::near_vm_logic::types::AccountId;

pub struct ContractOwnershipComponent;

impl Deploy for ContractOwnershipComponent {
    type Config = ValidAccountId;

    fn deploy(config: Option<Self::Config>) {
        let owner = config.unwrap();
        ContractOwnerObject::initialize_contract(owner);
    }
}

impl ContractOwnership for ContractOwnershipComponent {
    fn owner(&self) -> AccountId {
        let account_ids = ContractOwnershipAccountIdsObject::load();
        account_ids.owner.clone()
    }

    fn transfer_ownership(&mut self, new_owner: ValidAccountId) {
        assert_yocto_near_attached();
        ContractOwnerObject::assert_owner_access();
        ContractOwnerObject::set_owner(new_owner);
    }

    fn cancel_transfer_ownership(&mut self) {
        assert_yocto_near_attached();

        let mut owner = ContractOwnerObject::assert_owner_access();
        if owner.prospective_owner_account_id_hash.take().is_some() {
            owner.save();
        }
    }

    fn is_prospective_owner(&self, account_id: ValidAccountId) -> bool {
        ContractOwnerObject::load()
            .prospective_owner_account_id_hash()
            .map_or(false, |account_id_hash| {
                account_id_hash == account_id.into()
            })
    }

    fn finalize_transfer_ownership(&mut self) {
        unimplemented!()
    }

    fn withdraw_owner_balance(&mut self, _amount: Option<YoctoNear>) -> ContractOwnerNearBalance {
        unimplemented!()
    }

    fn owner_balance(&self) -> ContractOwnerNearBalance {
        unimplemented!()
    }
}

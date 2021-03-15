use crate::{
    ContractOwnerNearBalance, ContractOwnerObject, ContractOwnership,
    ContractOwnershipAccountIdsObject, LOG_EVENT_CONTRACT_TRANSFER_CANCELLED,
    LOG_EVENT_CONTRACT_TRANSFER_INITIATED,
};
use near_sdk::json_types::ValidAccountId;
use oysterpack_smart_near::asserts::assert_yocto_near_attached;
use oysterpack_smart_near::component::Deploy;
use oysterpack_smart_near::domain::{AccountIdHash, YoctoNear};
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

        let mut owner = ContractOwnerObject::assert_owner_access();
        let new_owner_account_id_hash: AccountIdHash = new_owner.as_ref().as_str().into();
        let current_prospective_owner_account_id_hash =
            owner.prospective_owner_account_id_hash.as_ref().cloned();

        let mut update_prospective_owner = || {
            owner.prospective_owner_account_id_hash = Some(new_owner_account_id_hash);
            LOG_EVENT_CONTRACT_TRANSFER_INITIATED.log(new_owner.as_ref());
            owner.save();
        };

        match current_prospective_owner_account_id_hash {
            None => update_prospective_owner(),
            Some(prospective_owner_account_id_hash) => {
                if prospective_owner_account_id_hash != new_owner_account_id_hash {
                    update_prospective_owner()
                }
            }
        }
    }

    fn cancel_ownership_transfer(&mut self) {
        assert_yocto_near_attached();

        let mut owner = ContractOwnerObject::assert_owner_access();
        if owner.prospective_owner_account_id_hash.take().is_some() {
            owner.save();
            LOG_EVENT_CONTRACT_TRANSFER_CANCELLED.log("");
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

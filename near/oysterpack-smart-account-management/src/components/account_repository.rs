use crate::*;
use oysterpack_smart_near::domain::YoctoNear;
use oysterpack_smart_near::near_sdk::borsh::{BorshDeserialize, BorshSerialize};
use std::fmt::Debug;
use std::marker::PhantomData;

#[derive(Clone, Copy, Default)]
pub struct AccountRepositoryComponent<T>(PhantomData<T>);

impl<T> AccountRepository<T> for AccountRepositoryComponent<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    fn create_account(
        &mut self,
        account_id: &str,
        near_balance: YoctoNear,
        data: Option<T>,
    ) -> Account<T> {
        ERR_ACCOUNT_ALREADY_REGISTERED.assert(|| !AccountNearDataObject::exists(account_id));

        let near_data = AccountNearDataObject::new(account_id, near_balance);
        near_data.save();

        match data {
            Some(data) => {
                let mut data = AccountDataObject::<T>::new(account_id, data);
                data.save();
                (near_data, Some(data))
            }
            None => (near_data, None),
        }
    }

    fn load_account(&self, account_id: &str) -> Option<Account<T>> {
        self.load_account_near_data(account_id)
            .map(|near_data| (near_data, self.load_account_data(account_id)))
    }

    fn load_account_data(&self, account_id: &str) -> Option<AccountDataObject<T>> {
        AccountDataObject::<T>::load(account_id)
    }

    fn load_account_near_data(&self, account_id: &str) -> Option<AccountNearDataObject> {
        AccountNearDataObject::load(account_id)
    }

    fn registered_account(&self, account_id: &str) -> Account<T> {
        let account = self.load_account(account_id);
        ERR_ACCOUNT_NOT_REGISTERED.assert(|| account.is_some());
        account.unwrap()
    }

    fn registered_account_near_data(&self, account_id: &str) -> AccountNearDataObject {
        let account = self.load_account_near_data(account_id);
        ERR_ACCOUNT_NOT_REGISTERED.assert(|| account.is_some());
        account.unwrap()
    }

    fn registered_account_data(&self, account_id: &str) -> AccountDataObject<T> {
        match self.load_account_data(account_id) {
            None => {
                ERR_ACCOUNT_ALREADY_REGISTERED.assert(|| AccountNearDataObject::exists(account_id));
                AccountDataObject::new(account_id, Default::default())
            }
            Some(account_data) => account_data,
        }
    }

    fn account_exists(&self, account_id: &str) -> bool {
        AccountNearDataObject::exists(account_id)
    }

    fn delete_account(&mut self, account_id: &str) {
        if let Some((near_data, data)) = self.load_account(account_id) {
            near_data.delete();
            if let Some(data) = data {
                data.delete();
            }
        }
    }
}

#[cfg(test)]
mod tests_account_repository {
    use super::*;
    use oysterpack_smart_near::near_sdk;
    use oysterpack_smart_near::YOCTO;
    use oysterpack_smart_near_test::*;
    use std::ops::Deref;

    type Accounts = AccountRepositoryComponent<String>;

    #[test]
    fn crud() {
        let account = "alfio";
        let ctx = new_context(account);
        testing_env!(ctx);

        let mut accounts = Accounts::default();
        let service: &mut dyn AccountRepository<String> = &mut accounts;
        assert!(service.load_account(account).is_none());
        assert!(service.load_account_near_data(account).is_none());
        assert!(service.load_account_near_data(account).is_none());

        let (account_near_data, account_data) = service.create_account(account, YOCTO.into(), None);
        assert!(account_data.is_none());
        assert_eq!(account_near_data.near_balance(), YOCTO.into());

        let mut account_data = AccountDataObject::<String>::new(account, "data".to_string());
        account_data.save();

        let (account_near_data, account_data) = service.load_account(account).unwrap();
        assert_eq!(account_data.as_ref().unwrap().deref().as_str(), "data");
        assert_eq!(account_near_data.near_balance(), YOCTO.into());

        let (account_near_data2, account_data2) = service.registered_account(account);
        assert_eq!(account_near_data, account_near_data2);
        assert_eq!(
            account_data.as_ref().unwrap(),
            account_data2.as_ref().unwrap()
        );

        assert_eq!(
            account_near_data2,
            service.registered_account_near_data(account)
        );
        assert_eq!(
            account_data2.unwrap(),
            service.registered_account_data(account)
        );

        assert!(service.account_exists(account));
        service.delete_account(account);
        assert!(!service.account_exists(account));

        service.delete_account(account);
        assert!(!service.account_exists(account));
    }

    #[test]
    #[should_panic(expected = "[ERR] [ACCOUNT_ALREADY_REGISTERED]")]
    fn create_account_already_exists() {
        let account = "alfio";
        let ctx = new_context(account);
        testing_env!(ctx);

        let mut accounts = Accounts::default();
        let service: &mut dyn AccountRepository<String> = &mut accounts;

        service.create_account(account, YOCTO.into(), None);
        service.create_account(account, YOCTO.into(), None);
    }
}

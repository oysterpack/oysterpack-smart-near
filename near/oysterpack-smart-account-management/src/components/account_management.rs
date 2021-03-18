//! [`AccountManagementComponent`]
//! - constructor: [`AccountManagementComponent::new`]
//!   - Box<dyn [`UnregisterAccount`]>
//! - deployment: [`AccountManagementComponent::deploy`]
//!   - config: [`StorageUsageBounds`]
//!

use crate::{
    Account, AccountDataObject, AccountMetrics, AccountRepository, AccountStorageEvent,
    AccountStorageUsage, GetAccountMetrics, HasAccountStorageUsage, StorageBalance,
    StorageBalanceBounds, StorageManagement, StorageUsageBounds, ERR_ACCOUNT_ALREADY_REGISTERED,
    ERR_ACCOUNT_NOT_REGISTERED,
};
use near_sdk::{
    borsh::{BorshDeserialize, BorshSerialize},
    env,
    json_types::ValidAccountId,
    Promise,
};
use oysterpack_smart_near::{
    asserts::{assert_min_near_attached, assert_yocto_near_attached},
    domain::YoctoNear,
    eventbus, ErrCode, ErrorConst,
};
use std::{fmt::Debug, ops::Deref};
use teloc::*;

use crate::components::account_storage_usage::AccountStorageUsageComponent;
use crate::AccountNearDataObject;
use oysterpack_smart_near::component::Deploy;
use oysterpack_smart_near::domain::{StorageUsage, ZERO_NEAR};
use std::marker::PhantomData;

pub const ERR_INSUFFICIENT_STORAGE_BALANCE: ErrorConst = ErrorConst(
    ErrCode("INSUFFICIENT_STORAGE_BALANCE"),
    "account's available storage balance is insufficient to satisfy request",
);

/// Core account management component implements the following interfaces:
/// 1. [`AccountRepository`]
/// 2. [`StorageManagement`] - NEP-145
/// 3. [`GetAccountMetrics`]
/// 4. [`AccountStorageUsage`]
pub struct AccountManagementComponent<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    /// must be provided by the contract
    unregister: Box<dyn UnregisterAccount>,

    account_storage_usage: AccountStorageUsageComponent,

    _phantom_data: PhantomData<T>,
}

/// Contract is required to provide implementation that applies contract specific business logic.
/// - see [`StorageManagement::storage_unregister`]
pub trait UnregisterAccount {
    /// [`AccountManagementComponent`] will be responsible for
    /// - sending account NEAR balance refund
    /// - publishing events
    /// - deleting [`AccountNearDataObject`] and [`crate::AccountDataObject`] objects from contract storage
    ///
    /// The [`UnregisterAccount`] delegate is responsible for:
    /// - if `force=false`, panic if the account cannot be deleted because of contract specific
    ///   business logic, e.g., for FT, the account cannot unregister if it has a token balance
    /// - delete any account data outside of the [`AccountNearDataObject`] and [`crate::AccountDataObject`] objects
    /// - apply any contract specific business logic
    fn unregister_account(&mut self, force: bool);
}

/// Default implementation that performs no contract specific operation, i.e., no-operation
///
/// USE CASE: contract stores all account data within [`crate::AccountDataObject`] object
#[derive(Dependency)]
pub struct UnregisterAccountNOOP;

impl UnregisterAccount for UnregisterAccountNOOP {
    fn unregister_account(&mut self, _force: bool) {
        // no action required
    }
}

impl From<Box<UnregisterAccountNOOP>> for Box<dyn UnregisterAccount> {
    fn from(x: Box<UnregisterAccountNOOP>) -> Self {
        x
    }
}

/// constructor
#[inject]
impl<T> AccountManagementComponent<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    pub fn new(unregister: Box<dyn UnregisterAccount>) -> Self {
        AccountMetrics::register_account_storage_event_handler();
        Self {
            unregister,
            account_storage_usage: Default::default(),
            _phantom_data: Default::default(),
        }
    }
}

impl<T> AccountManagementComponent<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    /// helper method used to measure the amount of storage needed to store the specified data.
    pub fn measure_storage_usage(account_data: T) -> StorageUsage {
        let mut account_manager: Self = Self::new(Box::new(UnregisterAccountNOOP));

        // seeds the storage required to store metrics
        {
            let account_id = "1953717115592535419708657925195464285";
            account_manager.create_account(account_id, 0.into(), Some(account_data.clone()));
            account_manager.delete_account(account_id);
        }

        let account_id = "1953718041838591893489340663938715635";
        let initial_storage_usage = env::storage_usage();
        account_manager.create_account(account_id, 0.into(), Some(account_data));
        let storage_usage = env::storage_usage() - initial_storage_usage;

        // clean up storage
        account_manager.delete_account(account_id);
        // ensure all data is cleaned up
        assert_eq!(initial_storage_usage, env::storage_usage());

        storage_usage.into()
    }
}

impl<T> Deploy for AccountManagementComponent<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    type Config = StorageUsageBounds;

    fn deploy(config: Option<Self::Config>) {
        AccountStorageUsageComponent::deploy(config)
    }
}

impl<T> AccountRepository<T> for AccountManagementComponent<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    /// Creates a new account.
    ///
    /// - tracks storage usage - emits [`AccountStorageEvent::StorageUsageChanged`]
    ///
    /// # Panics
    /// if the account already is registered
    fn create_account(
        &mut self,
        account_id: &str,
        near_balance: YoctoNear,
        data: Option<T>,
    ) -> Account<T> {
        ERR_ACCOUNT_ALREADY_REGISTERED.assert(|| !AccountNearDataObject::exists(account_id));

        let near_data = AccountNearDataObject::new(account_id, near_balance);

        // measure the storage usage
        let initial_storage_usage = env::storage_usage();
        near_data.save();
        let account_storage_usage = env::storage_usage() - initial_storage_usage;
        // update the account storage usage
        eventbus::post(&AccountStorageEvent::StorageUsageChanged(
            near_data.key().account_id_hash(),
            account_storage_usage.into(),
        ));

        match data {
            Some(data) => {
                let mut data = AccountDataObject::<T>::new(account_id, data);
                data.save();
                (near_data, Some(data))
            }
            None => (near_data, None),
        }
    }

    /// tries to load the account from storage
    fn load_account(&self, account_id: &str) -> Option<Account<T>> {
        self.load_account_near_data(account_id)
            .map(|near_data| (near_data, self.load_account_data(account_id)))
    }

    /// tries to load the account data from storage
    fn load_account_data(&self, account_id: &str) -> Option<AccountDataObject<T>> {
        AccountDataObject::<T>::load(account_id)
    }

    /// tries to load the account NEAR data from storage
    fn load_account_near_data(&self, account_id: &str) -> Option<AccountNearDataObject> {
        AccountNearDataObject::load(account_id)
    }

    /// ## Panics
    /// if the account is not registered
    fn registered_account(&self, account_id: &str) -> Account<T> {
        let account = self.load_account(account_id);
        ERR_ACCOUNT_NOT_REGISTERED.assert(|| account.is_some());
        account.unwrap()
    }

    /// ## Panics
    /// if the account is not registered
    fn registered_account_near_data(&self, account_id: &str) -> AccountNearDataObject {
        let account = self.load_account_near_data(account_id);
        ERR_ACCOUNT_NOT_REGISTERED.assert(|| account.is_some());
        account.unwrap()
    }

    /// Assumes that the account will always have data if registered.
    ///
    /// ## Panics
    /// if the account is not registered
    fn registered_account_data(&self, account_id: &str) -> AccountDataObject<T> {
        let account = self.load_account_data(account_id);
        ERR_ACCOUNT_NOT_REGISTERED.assert(|| account.is_some());
        account.unwrap()
    }

    fn account_exists(&self, account_id: &str) -> bool {
        AccountNearDataObject::exists(account_id)
    }

    /// Deletes [AccountNearDataObject] and [AccountDataObject] for the specified  account ID
    /// - tracks storage usage - emits [`AccountStorageEvent::StorageUsageChanged`]
    fn delete_account(&mut self, account_id: &str) {
        if let Some((near_data, data)) = self.load_account(account_id) {
            near_data.delete();
            if let Some(data) = data {
                data.delete();
            }
        }
    }
}

impl<T> GetAccountMetrics for AccountManagementComponent<T> where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default
{
}

/// exposes [`AccountStorageUsage`] interface on the component
impl<T> HasAccountStorageUsage for AccountManagementComponent<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    fn account_storage_usage(&self) -> &dyn AccountStorageUsage {
        &self.account_storage_usage
    }
}

impl<T> StorageManagement for AccountManagementComponent<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default + 'static,
{
    fn storage_deposit(
        &mut self,
        account_id: Option<ValidAccountId>,
        registration_only: Option<bool>,
    ) -> StorageBalance {
        // if the account ID is not specified, then deposit is for the predecessor account ID
        let account_id = account_id.map_or_else(env::predecessor_account_id, |account_id| {
            account_id.as_ref().clone()
        });

        let storage_balance_bounds = self.storage_balance_bounds();

        let registration_only = registration_only.unwrap_or(false);
        if registration_only {
            assert_min_near_attached(storage_balance_bounds.min);
        }
        let deposit: YoctoNear = env::attached_deposit().into();

        let account: AccountNearDataObject = match self.load_account_near_data(&account_id) {
            Some(mut account) => {
                if registration_only {
                    // refund the full deposit
                    send_refund(deposit.value());
                } else {
                    if let Some(max) = storage_balance_bounds.max {
                        self.deposit_with_max_bound(
                            &mut account,
                            Deposit(deposit),
                            MaxStorageBalance(max),
                        )
                    } else {
                        self.deposit(&mut account, deposit)
                    }
                }
                account
            }
            None => self.register_account(
                &account_id,
                deposit,
                registration_only,
                storage_balance_bounds,
            ),
        };

        account.storage_balance(storage_balance_bounds.min)
    }

    fn storage_withdraw(&mut self, amount: Option<YoctoNear>) -> StorageBalance {
        assert_yocto_near_attached();

        let mut account = self.registered_account_near_data(env::predecessor_account_id().as_str());
        let storage_balance_bounds = self.storage_balance_bounds();
        let account_available_balance = account
            .storage_balance(storage_balance_bounds.min)
            .available;
        match amount {
            Some(amount) => {
                if amount > ZERO_NEAR {
                    ERR_INSUFFICIENT_STORAGE_BALANCE.assert(|| account_available_balance >= amount);
                    send_refund(amount + 1);
                    account.dec_near_balance(amount);
                    account.save();
                }
            }
            None => {
                if account_available_balance > ZERO_NEAR {
                    send_refund(account_available_balance + 1);
                    account.dec_near_balance(account_available_balance);
                    account.save();
                }
            }
        }

        account.storage_balance(storage_balance_bounds.min)
    }

    fn storage_unregister(&mut self, force: Option<bool>) -> bool {
        assert_yocto_near_attached();
        let account_id = env::predecessor_account_id();
        self.load_account_near_data(&account_id)
            .map_or(false, |account| {
                let account_near_balance = account.near_balance();
                self.unregister.unregister_account(force.unwrap_or(false));
                self.delete_account(&account_id);
                eventbus::post(&AccountStorageEvent::Unregistered(account_near_balance));
                send_refund(account_near_balance + 1);
                true
            })
    }

    fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        self.account_storage_usage.storage_usage_bounds().into()
    }

    fn storage_balance_of(&self, account_id: ValidAccountId) -> Option<StorageBalance> {
        self.load_account_near_data(account_id.as_ref())
            .map(|account| StorageBalance {
                total: account.near_balance(),
                available: (account.near_balance().value()
                    - self.storage_balance_bounds().min.value())
                .into(),
            })
    }
}

/// helper functions
impl<T> AccountManagementComponent<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    fn register_account(
        &mut self,
        account_id: &str,
        deposit: YoctoNear,
        registration_only: bool,
        storage_balance_bounds: StorageBalanceBounds,
    ) -> AccountNearDataObject {
        let deposit = self.initial_deposit(deposit, registration_only, storage_balance_bounds);
        let (account, _data) = self.create_account(account_id, deposit, None);
        eventbus::post(&AccountStorageEvent::Registered(
            account.storage_balance(storage_balance_bounds.min),
        ));
        account
    }

    /// refunds deposit amount that is above the max allowed storage balance
    fn deposit_with_max_bound(
        &self,
        account: &mut AccountNearDataObject,
        deposit: Deposit,
        max: MaxStorageBalance,
    ) {
        if account.near_balance() < *max {
            let max_allowed_deposit = max.value() - account.near_balance().value();
            let deposit = if deposit.value() > max_allowed_deposit {
                // refund amount over the upper bound
                send_refund(deposit.value() - max_allowed_deposit);
                max_allowed_deposit.into()
            } else {
                *deposit
            };

            self.deposit(account, deposit);
        } else {
            // account storage balance is already at max limit - thus refund the full deposit amount
            send_refund(deposit.value());
        }
    }

    fn deposit(&self, account: &mut AccountNearDataObject, deposit: YoctoNear) {
        account.incr_near_balance(deposit);
        account.save();
    }

    fn initial_deposit(
        &self,
        deposit: YoctoNear,
        registration_only: bool,
        storage_balance_bounds: StorageBalanceBounds,
    ) -> YoctoNear {
        assert_min_near_attached(storage_balance_bounds.min);
        if registration_only {
            // only take the min required and refund the rest
            let refund_amount = deposit.value() - storage_balance_bounds.min.value();
            if refund_amount > 0 {
                send_refund(refund_amount);
            }
            storage_balance_bounds.min
        } else {
            // refund deposit that is over the max allowed
            storage_balance_bounds.max.map_or(deposit, |max| {
                if deposit.value() > max.value() {
                    let refund_amount = deposit.value() - max.value();
                    send_refund(refund_amount);
                    max
                } else {
                    deposit
                }
            })
        }
    }
}

/// refund is always sent back to the predecessor account ID
fn send_refund<Amount: Into<YoctoNear>>(amount: Amount) {
    Promise::new(env::predecessor_account_id()).transfer(amount.into().value());
}

struct Deposit(YoctoNear);

impl Deref for Deposit {
    type Target = YoctoNear;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

struct MaxStorageBalance(YoctoNear);

impl Deref for MaxStorageBalance {
    type Target = YoctoNear;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(test)]
mod tests_service {
    use super::*;
    use crate::StorageUsageBounds;
    use oysterpack_smart_near_test::*;

    fn deploy_account_service() {
        AccountStorageUsageComponent::deploy(Some(StorageUsageBounds {
            min: 1000.into(),
            max: None,
        }));
    }

    #[test]
    fn deploy_and_use_module() {
        // Arrange
        let account_id = "bob";
        let ctx = new_context(account_id);
        testing_env!(ctx);

        // Act
        deploy_account_service();

        let service: AccountManagementComponent<()> =
            AccountManagementComponent::new(Box::new(UnregisterAccountNOOP));
        let storage_balance_bounds = service.storage_balance_bounds();
        assert_eq!(
            storage_balance_bounds.min,
            (env::storage_byte_cost() * 1000).into()
        );
        assert!(storage_balance_bounds.max.is_none());

        let _storage_usage_bounds = service.storage_balance_of(to_valid_account_id(account_id));
    }
}

#[cfg(test)]
mod tests_teloc {
    use super::*;
    use crate::StorageUsageBounds;
    use oysterpack_smart_near_test::*;

    pub type AccountManager = AccountManagementComponent<()>;

    fn deploy_account_service() {
        AccountManager::deploy(Some(StorageUsageBounds {
            min: 1000.into(),
            max: None,
        }));
    }

    #[test]
    fn deploy_and_use_module() {
        // Arrange
        let account_id = "bob";
        let ctx = new_context(account_id);
        testing_env!(ctx);

        // Act
        deploy_account_service();

        let container = ServiceProvider::new()
            .add_transient_c::<Box<dyn UnregisterAccount>, Box<UnregisterAccountNOOP>>()
            .add_transient::<AccountManager>();

        let service: AccountManager = container.resolve();
        let storage_balance_bounds = service.storage_balance_bounds();
        assert_eq!(
            storage_balance_bounds.min,
            (env::storage_byte_cost() * 1000).into()
        );
        assert!(storage_balance_bounds.max.is_none());

        let _storage_usage_bounds = service.storage_balance_of(to_valid_account_id(account_id));
    }
}

#[cfg(test)]
mod tests_storage_management {
    use super::*;
    use crate::{AccountMetrics, StorageUsageBounds};
    use oysterpack_smart_near::domain::StorageUsage;
    use oysterpack_smart_near_test::*;

    const STORAGE_USAGE_BOUNDS: StorageUsageBounds = StorageUsageBounds {
        min: StorageUsage(1000),
        max: None,
    };

    fn storage_balance_min() -> YoctoNear {
        (STORAGE_USAGE_BOUNDS.min.value() as u128 * env::STORAGE_PRICE_PER_BYTE).into()
    }

    const PREDECESSOR_ACCOUNT_ID: &str = "alice";

    fn run_test<F>(
        storage_usage_bounds: StorageUsageBounds,
        account_id: Option<&str>,
        registration_only: Option<bool>,
        deposit: YoctoNear,
        already_registered: bool, // if true, then the account ID will be registered before hand using storage balance min
        test: F,
    ) where
        F: FnOnce(AccountManagementComponent<()>, StorageBalance),
    {
        let mut ctx = new_context(PREDECESSOR_ACCOUNT_ID);
        testing_env!(ctx.clone());

        AccountMetrics::register_account_storage_event_handler();
        AccountMetrics::reset();

        AccountStorageUsageComponent::deploy(Some(storage_usage_bounds));

        let mut service: AccountManagementComponent<()> =
            AccountManagementComponent::new(Box::new(UnregisterAccountNOOP));
        let storage_balance_bounds = service.storage_balance_bounds();
        println!("storage_balance_bounds = {:?}", storage_balance_bounds);

        if already_registered {
            ctx.attached_deposit = storage_balance_bounds.min.value();
            testing_env!(ctx.clone());
            let storage_balance = service.storage_deposit(
                Some(to_valid_account_id(
                    account_id.unwrap_or(PREDECESSOR_ACCOUNT_ID),
                )),
                Some(true),
            );
            println!("registered account: {:?}", storage_balance);
        }

        ctx.attached_deposit = deposit.value();
        println!("deposit amount = {}", ctx.attached_deposit);
        testing_env!(ctx.clone());

        let storage_balance =
            service.storage_deposit(account_id.map(to_valid_account_id), registration_only);
        println!("storage_balance after deposit = {:?}", storage_balance);

        test(service, storage_balance);
    }

    #[cfg(test)]
    mod tests_storage_deposit {
        use super::*;

        type AccountManager = AccountManagementComponent<()>;

        #[cfg(test)]
        mod self_registration_only {
            use super::*;

            fn run_test<F>(
                deposit: YoctoNear,
                already_registered: bool, // if true, then the account ID will be registered before hand using storage balance min
                test: F,
            ) where
                F: FnOnce(AccountManagementComponent<()>, StorageBalance),
            {
                super::run_test(
                    STORAGE_USAGE_BOUNDS,
                    None,
                    Some(true),
                    deposit,
                    already_registered,
                    test,
                );
            }

            #[test]
            fn unknown_account_with_exact_storage_deposit() {
                run_test(
                    storage_balance_min(),
                    false,
                    |service, storage_balance: StorageBalance| {
                        assert_eq!(storage_balance.total, service.storage_balance_bounds().min);
                        assert_eq!(storage_balance.available, 0.into());

                        let storage_balance_2 = service
                            .storage_balance_of(to_valid_account_id(PREDECESSOR_ACCOUNT_ID))
                            .unwrap();
                        assert_eq!(storage_balance, storage_balance_2);

                        let account = service.registered_account_near_data(PREDECESSOR_ACCOUNT_ID);
                        assert_eq!(account.near_balance(), service.storage_balance_bounds().min);

                        // AccountStorageEvent:Registered event should have been published to update stats
                        let account_stats = AccountManager::account_metrics();
                        assert_eq!(account_stats.total_registered_accounts, 1.into());
                        assert_eq!(account_stats.total_near_balance, account.near_balance());
                    },
                );
            }

            #[test]
            fn unknown_account_with_over_payment() {
                run_test(
                    (storage_balance_min().value() * 3).into(),
                    false,
                    |service, storage_balance: StorageBalance| {
                        // Assert
                        assert_eq!(storage_balance.total, service.storage_balance_bounds().min);
                        assert_eq!(storage_balance.available, 0.into());

                        let storage_balance_2 = service
                            .storage_balance_of(to_valid_account_id(PREDECESSOR_ACCOUNT_ID))
                            .unwrap();
                        assert_eq!(storage_balance, storage_balance_2);

                        // Assert account was registered
                        let account = service.registered_account_near_data(PREDECESSOR_ACCOUNT_ID);
                        assert_eq!(account.near_balance(), service.storage_balance_bounds().min);

                        // AccountStorageEvent:Registered event should have been published to update stats
                        let account_stats = AccountManager::account_metrics();
                        assert_eq!(account_stats.total_registered_accounts, 1.into());
                        assert_eq!(account_stats.total_near_balance, account.near_balance());

                        // Assert overpayment was refunded
                        let receipts = deserialize_receipts();
                        assert_eq!(receipts.len(), 1);
                        let receipt = &receipts[0];
                        assert_eq!(&receipt.receiver_id, PREDECESSOR_ACCOUNT_ID);
                        let action = &receipt.actions[0];
                        match action {
                            Action::Transfer(action) => {
                                assert_eq!(
                                    action.deposit,
                                    service.storage_balance_bounds().min.value() * 2
                                );
                            }
                            _ => panic!("expected Transfer"),
                        }
                    },
                );
            }

            #[test]
            fn already_registered() {
                run_test(
                    storage_balance_min(),
                    true,
                    |service, storage_balance: StorageBalance| {
                        assert_eq!(storage_balance.total, service.storage_balance_bounds().min);
                        assert_eq!(storage_balance.available, 0.into());

                        let storage_balance_2 = service
                            .storage_balance_of(to_valid_account_id(PREDECESSOR_ACCOUNT_ID))
                            .unwrap();
                        assert_eq!(storage_balance, storage_balance_2);

                        // Assert the deposit was refunded
                        let receipts = deserialize_receipts();
                        assert_eq!(receipts.len(), 1);
                        let receipt = &receipts[0];
                        assert_eq!(&receipt.receiver_id, PREDECESSOR_ACCOUNT_ID);
                        let action = &receipt.actions[0];
                        match action {
                            Action::Transfer(action) => {
                                assert_eq!(action.deposit, storage_balance_min().value());
                            }
                            _ => panic!("expected Transfer"),
                        }
                    },
                );
            }

            #[test]
            #[should_panic(expected = "[ERR] [INSUFFICIENT_NEAR_DEPOSIT]")]
            fn zero_deposit_attached() {
                run_test(0.into(), false, |_service, _storage_balance| {});
            }

            #[test]
            #[should_panic(expected = "[ERR] [INSUFFICIENT_NEAR_DEPOSIT]")]
            fn zero_deposit_attached_already_registered() {
                run_test(0.into(), true, |_service, _storage_balance| {});
            }

            #[test]
            #[should_panic(expected = "[ERR] [INSUFFICIENT_NEAR_DEPOSIT]")]
            fn one_deposit_attached_already_registered() {
                run_test(1.into(), true, |_service, _storage_balance| {});
            }
        }

        #[cfg(test)]
        mod self_registration_only_with_max_bound_set {
            use super::*;

            const STORAGE_USAGE_BOUNDS: StorageUsageBounds = StorageUsageBounds {
                min: StorageUsage(1000),
                max: Some(StorageUsage(2000)),
            };

            fn storage_balance_min() -> YoctoNear {
                (STORAGE_USAGE_BOUNDS.min.value() as u128 * env::STORAGE_PRICE_PER_BYTE).into()
            }

            fn run_test<F>(
                deposit: YoctoNear,
                already_registered: bool, // if true, then the account ID will be registered before hand using storage balance min
                test: F,
            ) where
                F: FnOnce(AccountManagementComponent<()>, StorageBalance),
            {
                super::run_test(
                    STORAGE_USAGE_BOUNDS,
                    None,
                    Some(true),
                    deposit,
                    already_registered,
                    test,
                );
            }

            #[test]
            fn unknown_account_with_exact_storage_deposit() {
                run_test(
                    storage_balance_min(),
                    false,
                    |service, storage_balance: StorageBalance| {
                        assert_eq!(storage_balance.total, service.storage_balance_bounds().min);
                        assert_eq!(storage_balance.available, 0.into());

                        let storage_balance_2 = service
                            .storage_balance_of(to_valid_account_id(PREDECESSOR_ACCOUNT_ID))
                            .unwrap();
                        assert_eq!(storage_balance, storage_balance_2);

                        let account = service.registered_account_near_data(PREDECESSOR_ACCOUNT_ID);
                        assert_eq!(account.near_balance(), service.storage_balance_bounds().min);

                        // AccountStorageEvent:Registered event should have been published to update stats
                        let account_stats = AccountManager::account_metrics();
                        assert_eq!(account_stats.total_registered_accounts, 1.into());
                        assert_eq!(account_stats.total_near_balance, account.near_balance());
                    },
                );
            }

            #[test]
            fn unknown_account_with_over_payment() {
                run_test(
                    (storage_balance_min().value() * 3).into(),
                    false,
                    |service, storage_balance: StorageBalance| {
                        // Assert
                        assert_eq!(storage_balance.total, service.storage_balance_bounds().min);
                        assert_eq!(storage_balance.available, 0.into());

                        let storage_balance_2 = service
                            .storage_balance_of(to_valid_account_id(PREDECESSOR_ACCOUNT_ID))
                            .unwrap();
                        assert_eq!(storage_balance, storage_balance_2);

                        // Assert account was registered
                        let account = service.registered_account_near_data(PREDECESSOR_ACCOUNT_ID);
                        assert_eq!(account.near_balance(), service.storage_balance_bounds().min);

                        // AccountStorageEvent:Registered event should have been published to update stats
                        let account_stats = AccountManager::account_metrics();
                        assert_eq!(account_stats.total_registered_accounts, 1.into());
                        assert_eq!(account_stats.total_near_balance, account.near_balance());

                        // Assert overpayment was refunded
                        let receipts = deserialize_receipts();
                        assert_eq!(receipts.len(), 1);
                        let receipt = &receipts[0];
                        assert_eq!(&receipt.receiver_id, PREDECESSOR_ACCOUNT_ID);
                        let action = &receipt.actions[0];
                        match action {
                            Action::Transfer(action) => {
                                assert_eq!(
                                    action.deposit,
                                    service.storage_balance_bounds().min.value() * 2
                                );
                            }
                            _ => panic!("expected Transfer"),
                        }
                    },
                );
            }

            #[test]
            fn already_registered() {
                run_test(
                    storage_balance_min(),
                    true,
                    |service, storage_balance: StorageBalance| {
                        assert_eq!(storage_balance.total, service.storage_balance_bounds().min);
                        assert_eq!(storage_balance.available, 0.into());

                        let storage_balance_2 = service
                            .storage_balance_of(to_valid_account_id(PREDECESSOR_ACCOUNT_ID))
                            .unwrap();
                        assert_eq!(storage_balance, storage_balance_2);

                        // Assert the deposit was refunded
                        let receipts = deserialize_receipts();
                        assert_eq!(receipts.len(), 1);
                        let receipt = &receipts[0];
                        assert_eq!(&receipt.receiver_id, PREDECESSOR_ACCOUNT_ID);
                        let action = &receipt.actions[0];
                        match action {
                            Action::Transfer(action) => {
                                assert_eq!(action.deposit, storage_balance_min().value());
                            }
                            _ => panic!("expected Transfer"),
                        }
                    },
                );
            }

            #[test]
            #[should_panic(expected = "[ERR] [INSUFFICIENT_NEAR_DEPOSIT]")]
            fn zero_deposit_attached() {
                run_test(0.into(), false, |_service, _storage_balance| {});
            }

            #[test]
            #[should_panic(expected = "[ERR] [INSUFFICIENT_NEAR_DEPOSIT]")]
            fn zero_deposit_attached_already_registered() {
                run_test(0.into(), true, |_service, _storage_balance| {});
            }

            #[test]
            #[should_panic(expected = "[ERR] [INSUFFICIENT_NEAR_DEPOSIT]")]
            fn one_deposit_attached_already_registered() {
                run_test(1.into(), true, |_service, _storage_balance| {});
            }
        }

        #[cfg(test)]
        mod other_registration_only {
            use super::*;

            const ACCOUNT_ID: &str = "alfio";

            fn run_test<F>(
                deposit: YoctoNear,
                already_registered: bool, // if true, then the account ID will be registered before hand using storage balance min
                test: F,
            ) where
                F: FnOnce(AccountManagementComponent<()>, StorageBalance),
            {
                super::run_test(
                    STORAGE_USAGE_BOUNDS,
                    Some(ACCOUNT_ID),
                    Some(true),
                    deposit,
                    already_registered,
                    test,
                );
            }

            #[test]
            fn unknown_account_with_exact_storage_deposit() {
                run_test(
                    storage_balance_min(),
                    false,
                    |service, storage_balance: StorageBalance| {
                        assert_eq!(storage_balance.total, service.storage_balance_bounds().min);
                        assert_eq!(storage_balance.available, 0.into());

                        let storage_balance_2 = service
                            .storage_balance_of(to_valid_account_id(ACCOUNT_ID))
                            .unwrap();
                        assert_eq!(storage_balance, storage_balance_2);

                        let account = service.registered_account_near_data(ACCOUNT_ID);
                        assert_eq!(account.near_balance(), service.storage_balance_bounds().min);

                        // AccountStorageEvent:Registered event should have been published to update stats
                        let account_stats = AccountManager::account_metrics();
                        assert_eq!(account_stats.total_registered_accounts, 1.into());
                        assert_eq!(account_stats.total_near_balance, account.near_balance());
                    },
                );
            }

            #[test]
            fn unknown_account_with_over_payment() {
                run_test(
                    (storage_balance_min().value() * 3).into(),
                    false,
                    |service, storage_balance: StorageBalance| {
                        // Assert
                        assert_eq!(storage_balance.total, service.storage_balance_bounds().min);
                        assert_eq!(storage_balance.available, 0.into());

                        let storage_balance_2 = service
                            .storage_balance_of(to_valid_account_id(ACCOUNT_ID))
                            .unwrap();
                        assert_eq!(storage_balance, storage_balance_2);

                        // Assert account was registered
                        let account = service.registered_account_near_data(ACCOUNT_ID);
                        assert_eq!(account.near_balance(), service.storage_balance_bounds().min);

                        // AccountStorageEvent:Registered event should have been published to update stats
                        let account_stats = AccountManager::account_metrics();
                        assert_eq!(account_stats.total_registered_accounts, 1.into());
                        assert_eq!(account_stats.total_near_balance, account.near_balance());

                        // Assert overpayment was refunded
                        let receipts = deserialize_receipts();
                        assert_eq!(receipts.len(), 1);
                        let receipt = &receipts[0];
                        assert_eq!(&receipt.receiver_id, PREDECESSOR_ACCOUNT_ID);
                        let action = &receipt.actions[0];
                        match action {
                            Action::Transfer(action) => {
                                assert_eq!(
                                    action.deposit,
                                    service.storage_balance_bounds().min.value() * 2
                                );
                            }
                            _ => panic!("expected Transfer"),
                        }
                    },
                );
            }

            #[test]
            fn already_registered() {
                run_test(
                    storage_balance_min(),
                    true,
                    |service, storage_balance: StorageBalance| {
                        assert_eq!(storage_balance.total, service.storage_balance_bounds().min);
                        assert_eq!(storage_balance.available, 0.into());

                        let storage_balance_2 = service
                            .storage_balance_of(to_valid_account_id(ACCOUNT_ID))
                            .unwrap();
                        assert_eq!(storage_balance, storage_balance_2);

                        // Assert the deposit was refunded
                        let receipts = deserialize_receipts();
                        assert_eq!(receipts.len(), 1);
                        let receipt = &receipts[0];
                        assert_eq!(&receipt.receiver_id, PREDECESSOR_ACCOUNT_ID);
                        let action = &receipt.actions[0];
                        match action {
                            Action::Transfer(action) => {
                                assert_eq!(action.deposit, storage_balance_min().value());
                            }
                            _ => panic!("expected Transfer"),
                        }
                    },
                );
            }

            #[test]
            #[should_panic(expected = "[ERR] [INSUFFICIENT_NEAR_DEPOSIT]")]
            fn zero_deposit_attached() {
                run_test(0.into(), false, |_service, _storage_balance| {});
            }

            #[test]
            #[should_panic(expected = "[ERR] [INSUFFICIENT_NEAR_DEPOSIT]")]
            fn zero_deposit_attached_already_registered() {
                run_test(0.into(), true, |_service, _storage_balance| {});
            }

            #[test]
            #[should_panic(expected = "[ERR] [INSUFFICIENT_NEAR_DEPOSIT]")]
            fn one_deposit_attached_already_registered() {
                run_test(1.into(), true, |_service, _storage_balance| {});
            }
        }

        #[cfg(test)]
        mod self_deposit_with_implied_registration_only_false {
            use super::*;
            use oysterpack_smart_near::YOCTO;

            fn run_test<F>(
                deposit: YoctoNear,
                already_registered: bool, // if true, then the account ID will be registered before hand using storage balance min
                test: F,
            ) where
                F: FnOnce(AccountManagementComponent<()>, StorageBalance),
            {
                super::run_test(
                    STORAGE_USAGE_BOUNDS,
                    None,
                    None,
                    deposit,
                    already_registered,
                    test,
                );
            }

            fn run_test_with_storage_balance_bounds<F>(
                deposit: YoctoNear,
                already_registered: bool, // if true, then the account ID will be registered before hand using storage balance min
                storage_usage_bounds: StorageUsageBounds,
                test: F,
            ) where
                F: FnOnce(AccountManagementComponent<()>, StorageBalance),
            {
                super::run_test(
                    storage_usage_bounds,
                    None,
                    None,
                    deposit,
                    already_registered,
                    test,
                );
            }

            #[test]
            fn unknown_account_with_exact_storage_deposit() {
                run_test(
                    storage_balance_min(),
                    false,
                    |service, storage_balance: StorageBalance| {
                        assert_eq!(storage_balance.total, service.storage_balance_bounds().min);
                        assert_eq!(storage_balance.available, 0.into());

                        let storage_balance_2 = service
                            .storage_balance_of(to_valid_account_id(PREDECESSOR_ACCOUNT_ID))
                            .unwrap();
                        assert_eq!(storage_balance, storage_balance_2);

                        let account = service.registered_account_near_data(PREDECESSOR_ACCOUNT_ID);
                        assert_eq!(account.near_balance(), service.storage_balance_bounds().min);

                        // AccountStorageEvent:Registered event should have been published to update stats
                        let account_stats = AccountManager::account_metrics();
                        assert_eq!(account_stats.total_registered_accounts, 1.into());
                        assert_eq!(account_stats.total_near_balance, account.near_balance());
                    },
                );
            }

            #[test]
            fn unknown_account_with_over_payment() {
                let deposit_amount: YoctoNear = (storage_balance_min().value() * 3).into();
                run_test(
                    deposit_amount,
                    false,
                    |service, storage_balance: StorageBalance| {
                        // Assert
                        assert_eq!(storage_balance.total, deposit_amount);
                        assert_eq!(
                            storage_balance.available,
                            (service.storage_balance_bounds().min.value() * 2).into()
                        );

                        let storage_balance_2 = service
                            .storage_balance_of(to_valid_account_id(PREDECESSOR_ACCOUNT_ID))
                            .unwrap();
                        assert_eq!(storage_balance, storage_balance_2);

                        // Assert account was registered
                        let account = service.registered_account_near_data(PREDECESSOR_ACCOUNT_ID);
                        assert_eq!(account.near_balance(), deposit_amount);

                        // AccountStorageEvent:Registered event should have been published to update stats
                        let account_stats = AccountManager::account_metrics();
                        assert_eq!(account_stats.total_registered_accounts, 1.into());
                        assert_eq!(account_stats.total_near_balance, account.near_balance());
                    },
                );
            }

            #[test]
            fn unknown_account_with_over_payment_above_max_bound() {
                let deposit_amount: YoctoNear = (storage_balance_min().value() * 3).into();
                run_test_with_storage_balance_bounds(
                    deposit_amount,
                    false,
                    StorageUsageBounds {
                        min: STORAGE_USAGE_BOUNDS.min,
                        max: Some((STORAGE_USAGE_BOUNDS.min.value() * 2).into()),
                    },
                    |service, storage_balance: StorageBalance| {
                        let storage_balance_bounds = service.storage_balance_bounds();
                        // Assert
                        assert_eq!(storage_balance.total, storage_balance_bounds.max.unwrap());
                        assert_eq!(storage_balance.available, storage_balance_bounds.min);

                        // Assert account NEAR balance was persisted
                        let storage_balance_2 = service
                            .storage_balance_of(to_valid_account_id(PREDECESSOR_ACCOUNT_ID))
                            .unwrap();
                        assert_eq!(storage_balance, storage_balance_2);

                        // Assert account was registered
                        let account = service.registered_account_near_data(PREDECESSOR_ACCOUNT_ID);
                        assert_eq!(account.near_balance(), storage_balance_bounds.max.unwrap());

                        // AccountStorageEvent:Registered event should have been published to update stats
                        let account_stats = AccountManager::account_metrics();
                        assert_eq!(account_stats.total_registered_accounts, 1.into());
                        assert_eq!(account_stats.total_near_balance, account.near_balance());

                        let receipts = deserialize_receipts();
                        let receipt = &receipts[0];
                        assert_eq!(receipt.receiver_id, PREDECESSOR_ACCOUNT_ID);
                        match &receipt.actions[0] {
                            Action::Transfer(transfer) => {
                                assert_eq!(transfer.deposit, storage_balance_bounds.min.value());
                            }
                            _ => panic!("expected Transfer action"),
                        }
                    },
                );
            }

            #[test]
            fn deposit_with_account_already_maxed_out() {
                // Arrange
                let account = "alfio";
                let mut ctx = new_context(account);
                testing_env!(ctx.clone());

                AccountManagementComponent::<()>::deploy(Some(StorageUsageBounds {
                    min: 1000.into(),
                    max: Some(2000.into()),
                }));

                let mut service =
                    AccountManagementComponent::<()>::new(Box::new(UnregisterAccountNOOP));

                ctx.attached_deposit = YOCTO;
                testing_env!(ctx.clone());
                let storage_balance_1 = service.storage_deposit(None, None);
                testing_env!(ctx.clone());
                let storage_balance_2 = service.storage_deposit(None, None);
                assert_eq!(storage_balance_1, storage_balance_2);
                assert_eq!(
                    storage_balance_1.total,
                    service.storage_balance_bounds().max.unwrap()
                );
                let receipts = deserialize_receipts();
                assert_eq!(&receipts[0].receiver_id, account);
                match &receipts[0].actions[0] {
                    Action::Transfer(transfer) => assert_eq!(transfer.deposit, YOCTO),
                    _ => panic!("expected TransferAction"),
                }
            }

            #[test]
            fn already_registered() {
                run_test(
                    storage_balance_min(),
                    true,
                    |service, storage_balance: StorageBalance| {
                        assert_eq!(
                            storage_balance.total.value(),
                            service.storage_balance_bounds().min.value() * 2
                        );
                        assert_eq!(
                            storage_balance.available,
                            service.storage_balance_bounds().min
                        );

                        let storage_balance_2 = service
                            .storage_balance_of(to_valid_account_id(PREDECESSOR_ACCOUNT_ID))
                            .unwrap();
                        assert_eq!(storage_balance, storage_balance_2);
                    },
                );
            }

            #[test]
            #[should_panic(expected = "[ERR] [INSUFFICIENT_NEAR_DEPOSIT]")]
            fn zero_deposit_attached() {
                run_test(0.into(), false, |_service, _storage_balance| {});
            }

            #[test]
            fn zero_deposit_attached_already_registered() {
                run_test(0.into(), true, |service, storage_balance| {
                    let storage_balance_bounds = service.storage_balance_bounds();
                    assert_eq!(storage_balance.total, storage_balance_bounds.min);
                    assert_eq!(storage_balance.available, 0.into());
                });
            }

            #[test]
            fn one_deposit_attached_already_registered() {
                run_test(1.into(), true, |service, storage_balance| {
                    let storage_balance_bounds = service.storage_balance_bounds();
                    assert_eq!(
                        storage_balance.total.value(),
                        storage_balance_bounds.min.value() + 1
                    );
                    assert_eq!(storage_balance.available, 1.into());

                    // Assert account NEAR balance was persisted
                    let storage_balance_2 = service
                        .storage_balance_of(to_valid_account_id(PREDECESSOR_ACCOUNT_ID))
                        .unwrap();
                    assert_eq!(storage_balance, storage_balance_2);
                });
            }
        }

        #[cfg(test)]
        mod self_deposit_with_implied_registration_only_false_with_max_bound_set {
            use super::*;

            const STORAGE_USAGE_BOUNDS: StorageUsageBounds = StorageUsageBounds {
                min: StorageUsage(1000),
                max: Some(StorageUsage(1500)),
            };

            fn storage_balance_min() -> YoctoNear {
                (STORAGE_USAGE_BOUNDS.min.value() as u128 * env::STORAGE_PRICE_PER_BYTE).into()
            }

            fn storage_balance_max() -> YoctoNear {
                (STORAGE_USAGE_BOUNDS.max.unwrap().value() as u128 * env::STORAGE_PRICE_PER_BYTE)
                    .into()
            }

            fn run_test<F>(
                deposit: YoctoNear,
                already_registered: bool, // if true, then the account ID will be registered before hand using storage balance min
                test: F,
            ) where
                F: FnOnce(AccountManagementComponent<()>, StorageBalance),
            {
                super::run_test(
                    STORAGE_USAGE_BOUNDS,
                    None,
                    None,
                    deposit,
                    already_registered,
                    test,
                );
            }

            fn run_test_with_storage_balance_bounds<F>(
                deposit: YoctoNear,
                already_registered: bool, // if true, then the account ID will be registered before hand using storage balance min
                storage_usage_bounds: StorageUsageBounds,
                test: F,
            ) where
                F: FnOnce(AccountManagementComponent<()>, StorageBalance),
            {
                super::run_test(
                    storage_usage_bounds,
                    None,
                    None,
                    deposit,
                    already_registered,
                    test,
                );
            }

            #[test]
            fn unknown_account_with_exact_storage_deposit() {
                run_test(
                    storage_balance_min(),
                    false,
                    |service, storage_balance: StorageBalance| {
                        assert_eq!(storage_balance.total, service.storage_balance_bounds().min);
                        assert_eq!(storage_balance.available, 0.into());

                        let storage_balance_2 = service
                            .storage_balance_of(to_valid_account_id(PREDECESSOR_ACCOUNT_ID))
                            .unwrap();
                        assert_eq!(storage_balance, storage_balance_2);

                        let account = service.registered_account_near_data(PREDECESSOR_ACCOUNT_ID);
                        assert_eq!(account.near_balance(), service.storage_balance_bounds().min);

                        // AccountStorageEvent:Registered event should have been published to update stats
                        let account_stats = AccountManager::account_metrics();
                        assert_eq!(account_stats.total_registered_accounts, 1.into());
                        assert_eq!(account_stats.total_near_balance, account.near_balance());
                    },
                );
            }

            #[test]
            fn unknown_account_with_over_payment() {
                let deposit_amount: YoctoNear = (storage_balance_min().value() * 3).into();
                run_test(
                    deposit_amount,
                    false,
                    |service, storage_balance: StorageBalance| {
                        // Assert
                        assert_eq!(storage_balance.total, storage_balance_max());
                        assert_eq!(
                            storage_balance.available,
                            storage_balance_max() - storage_balance_min()
                        );

                        let storage_balance_2 = service
                            .storage_balance_of(to_valid_account_id(PREDECESSOR_ACCOUNT_ID))
                            .unwrap();
                        assert_eq!(storage_balance, storage_balance_2);

                        // Assert account was registered
                        let account = service.registered_account_near_data(PREDECESSOR_ACCOUNT_ID);
                        assert_eq!(account.near_balance(), storage_balance_max());

                        // AccountStorageEvent:Registered event should have been published to update stats
                        let account_stats = AccountManager::account_metrics();
                        assert_eq!(account_stats.total_registered_accounts, 1.into());
                        assert_eq!(account_stats.total_near_balance, account.near_balance());
                    },
                );
            }

            #[test]
            fn unknown_account_with_over_payment_above_max_bound() {
                let deposit_amount: YoctoNear = (storage_balance_min().value() * 3).into();
                run_test_with_storage_balance_bounds(
                    deposit_amount,
                    false,
                    StorageUsageBounds {
                        min: STORAGE_USAGE_BOUNDS.min,
                        max: Some((STORAGE_USAGE_BOUNDS.min.value() * 2).into()),
                    },
                    |service, storage_balance: StorageBalance| {
                        let storage_balance_bounds = service.storage_balance_bounds();
                        // Assert
                        assert_eq!(storage_balance.total, storage_balance_bounds.max.unwrap());
                        assert_eq!(storage_balance.available, storage_balance_bounds.min);

                        // Assert account NEAR balance was persisted
                        let storage_balance_2 = service
                            .storage_balance_of(to_valid_account_id(PREDECESSOR_ACCOUNT_ID))
                            .unwrap();
                        assert_eq!(storage_balance, storage_balance_2);

                        // Assert account was registered
                        let account = service.registered_account_near_data(PREDECESSOR_ACCOUNT_ID);
                        assert_eq!(account.near_balance(), storage_balance_bounds.max.unwrap());

                        // AccountStorageEvent:Registered event should have been published to update stats
                        let account_stats = AccountManager::account_metrics();
                        assert_eq!(account_stats.total_registered_accounts, 1.into());
                        assert_eq!(account_stats.total_near_balance, account.near_balance());

                        let receipts = deserialize_receipts();
                        let receipt = &receipts[0];
                        assert_eq!(receipt.receiver_id, PREDECESSOR_ACCOUNT_ID);
                        match &receipt.actions[0] {
                            Action::Transfer(transfer) => {
                                assert_eq!(transfer.deposit, storage_balance_bounds.min.value());
                            }
                            _ => panic!("expected Transfer action"),
                        }
                    },
                );
            }

            #[test]
            fn already_registered() {
                run_test(
                    storage_balance_min(),
                    true,
                    |service, storage_balance: StorageBalance| {
                        assert_eq!(storage_balance.total, storage_balance_max());
                        assert_eq!(
                            storage_balance.available,
                            storage_balance_max() - storage_balance_min()
                        );

                        let storage_balance_2 = service
                            .storage_balance_of(to_valid_account_id(PREDECESSOR_ACCOUNT_ID))
                            .unwrap();
                        assert_eq!(storage_balance, storage_balance_2);
                    },
                );
            }

            #[test]
            #[should_panic(expected = "[ERR] [INSUFFICIENT_NEAR_DEPOSIT]")]
            fn zero_deposit_attached() {
                run_test(0.into(), false, |_service, _storage_balance| {});
            }

            #[test]
            fn zero_deposit_attached_already_registered() {
                run_test(0.into(), true, |service, storage_balance| {
                    let storage_balance_bounds = service.storage_balance_bounds();
                    assert_eq!(storage_balance.total, storage_balance_bounds.min);
                    assert_eq!(storage_balance.available, 0.into());
                });
            }

            #[test]
            fn one_deposit_attached_already_registered() {
                run_test(1.into(), true, |service, storage_balance| {
                    let storage_balance_bounds = service.storage_balance_bounds();
                    assert_eq!(
                        storage_balance.total.value(),
                        storage_balance_bounds.min.value() + 1
                    );
                    assert_eq!(storage_balance.available, 1.into());

                    // Assert account NEAR balance was persisted
                    let storage_balance_2 = service
                        .storage_balance_of(to_valid_account_id(PREDECESSOR_ACCOUNT_ID))
                        .unwrap();
                    assert_eq!(storage_balance, storage_balance_2);
                });
            }
        }

        #[cfg(test)]
        mod deposit_for_account_with_implied_registration_only_false {
            use super::*;

            const ACCOUNT_ID: &str = "alfio.near";

            fn run_test<F>(
                deposit: YoctoNear,
                already_registered: bool, // if true, then the account ID will be registered before hand using storage balance min
                test: F,
            ) where
                F: FnOnce(AccountManagementComponent<()>, StorageBalance),
            {
                super::run_test(
                    STORAGE_USAGE_BOUNDS,
                    Some(ACCOUNT_ID),
                    None,
                    deposit,
                    already_registered,
                    test,
                );
            }

            fn run_test_with_storage_balance_bounds<F>(
                deposit: YoctoNear,
                already_registered: bool, // if true, then the account ID will be registered before hand using storage balance min
                storage_usage_bounds: StorageUsageBounds,
                test: F,
            ) where
                F: FnOnce(AccountManagementComponent<()>, StorageBalance),
            {
                super::run_test(
                    storage_usage_bounds,
                    Some(ACCOUNT_ID),
                    None,
                    deposit,
                    already_registered,
                    test,
                );
            }

            #[test]
            fn unknown_account_with_exact_storage_deposit() {
                run_test(
                    storage_balance_min(),
                    false,
                    |service, storage_balance: StorageBalance| {
                        assert_eq!(storage_balance.total, service.storage_balance_bounds().min);
                        assert_eq!(storage_balance.available, 0.into());

                        let storage_balance_2 = service
                            .storage_balance_of(to_valid_account_id(ACCOUNT_ID))
                            .unwrap();
                        assert_eq!(storage_balance, storage_balance_2);

                        let account = service.registered_account_near_data(ACCOUNT_ID);
                        assert_eq!(account.near_balance(), service.storage_balance_bounds().min);

                        // AccountStorageEvent:Registered event should have been published to update stats
                        let account_stats = AccountManager::account_metrics();
                        assert_eq!(account_stats.total_registered_accounts, 1.into());
                        assert_eq!(account_stats.total_near_balance, account.near_balance());
                    },
                );
            }

            #[test]
            fn unknown_account_with_over_payment() {
                let deposit_amount: YoctoNear = (storage_balance_min().value() * 3).into();
                run_test(
                    deposit_amount,
                    false,
                    |service, storage_balance: StorageBalance| {
                        // Assert
                        assert_eq!(storage_balance.total, deposit_amount);
                        assert_eq!(
                            storage_balance.available,
                            (service.storage_balance_bounds().min.value() * 2).into()
                        );

                        let storage_balance_2 = service
                            .storage_balance_of(to_valid_account_id(ACCOUNT_ID))
                            .unwrap();
                        assert_eq!(storage_balance, storage_balance_2);

                        // Assert account was registered
                        let account = service.registered_account_near_data(ACCOUNT_ID);
                        assert_eq!(account.near_balance(), deposit_amount);

                        // AccountStorageEvent:Registered event should have been published to update stats
                        let account_stats = AccountManager::account_metrics();
                        assert_eq!(account_stats.total_registered_accounts, 1.into());
                        assert_eq!(account_stats.total_near_balance, account.near_balance());
                    },
                );
            }

            #[test]
            fn unknown_account_with_over_payment_above_max_bound() {
                let deposit_amount: YoctoNear = (storage_balance_min().value() * 3).into();
                run_test_with_storage_balance_bounds(
                    deposit_amount,
                    false,
                    StorageUsageBounds {
                        min: STORAGE_USAGE_BOUNDS.min,
                        max: Some((STORAGE_USAGE_BOUNDS.min.value() * 2).into()),
                    },
                    |service, storage_balance: StorageBalance| {
                        let storage_balance_bounds = service.storage_balance_bounds();
                        // Assert
                        assert_eq!(storage_balance.total, storage_balance_bounds.max.unwrap());
                        assert_eq!(storage_balance.available, storage_balance_bounds.min);

                        // Assert account NEAR balance was persisted
                        let storage_balance_2 = service
                            .storage_balance_of(to_valid_account_id(ACCOUNT_ID))
                            .unwrap();
                        assert_eq!(storage_balance, storage_balance_2);

                        // Assert account was registered
                        let account = service.registered_account_near_data(ACCOUNT_ID);
                        assert_eq!(account.near_balance(), storage_balance_bounds.max.unwrap());

                        // AccountStorageEvent:Registered event should have been published to update stats
                        let account_stats = AccountManager::account_metrics();
                        assert_eq!(account_stats.total_registered_accounts, 1.into());
                        assert_eq!(account_stats.total_near_balance, account.near_balance());

                        let receipts = deserialize_receipts();
                        let receipt = &receipts[0];
                        assert_eq!(receipt.receiver_id, PREDECESSOR_ACCOUNT_ID);
                        match &receipt.actions[0] {
                            Action::Transfer(transfer) => {
                                assert_eq!(transfer.deposit, storage_balance_bounds.min.value());
                            }
                            _ => panic!("expected Transfer action"),
                        }
                    },
                );
            }

            #[test]
            fn already_registered() {
                run_test(
                    storage_balance_min(),
                    true,
                    |service, storage_balance: StorageBalance| {
                        assert_eq!(
                            storage_balance.total.value(),
                            service.storage_balance_bounds().min.value() * 2
                        );
                        assert_eq!(
                            storage_balance.available,
                            service.storage_balance_bounds().min
                        );

                        let storage_balance_2 = service
                            .storage_balance_of(to_valid_account_id(ACCOUNT_ID))
                            .unwrap();
                        assert_eq!(storage_balance, storage_balance_2);
                    },
                );
            }

            #[test]
            #[should_panic(expected = "[ERR] [INSUFFICIENT_NEAR_DEPOSIT]")]
            fn zero_deposit_attached() {
                run_test(0.into(), false, |_service, _storage_balance| {});
            }

            #[test]
            fn zero_deposit_attached_already_registered() {
                run_test(0.into(), true, |service, storage_balance| {
                    let storage_balance_bounds = service.storage_balance_bounds();
                    assert_eq!(storage_balance.total, storage_balance_bounds.min);
                    assert_eq!(storage_balance.available, 0.into());
                });
            }

            #[test]
            fn one_deposit_attached_already_registered() {
                run_test(1.into(), true, |service, storage_balance| {
                    let storage_balance_bounds = service.storage_balance_bounds();
                    assert_eq!(
                        storage_balance.total.value(),
                        storage_balance_bounds.min.value() + 1
                    );
                    assert_eq!(storage_balance.available, 1.into());

                    // Assert account NEAR balance was persisted
                    let storage_balance_2 = service
                        .storage_balance_of(to_valid_account_id(ACCOUNT_ID))
                        .unwrap();
                    assert_eq!(storage_balance, storage_balance_2);
                });
            }
        }

        #[cfg(test)]
        mod self_deposit_with_registration_only_false {
            use super::*;

            fn run_test<F>(
                deposit: YoctoNear,
                already_registered: bool, // if true, then the account ID will be registered before hand using storage balance min
                test: F,
            ) where
                F: FnOnce(AccountManagementComponent<()>, StorageBalance),
            {
                super::run_test(
                    STORAGE_USAGE_BOUNDS,
                    None,
                    Some(false),
                    deposit,
                    already_registered,
                    test,
                );
            }

            #[test]
            fn unknown_account_with_exact_storage_deposit() {
                run_test(
                    storage_balance_min(),
                    false,
                    |service, storage_balance: StorageBalance| {
                        assert_eq!(storage_balance.total, service.storage_balance_bounds().min);
                        assert_eq!(storage_balance.available, 0.into());

                        let account = service.registered_account_near_data(PREDECESSOR_ACCOUNT_ID);
                        assert_eq!(account.near_balance(), service.storage_balance_bounds().min);

                        // AccountStorageEvent:Registered event should have been published to update stats
                        let account_stats = AccountManager::account_metrics();
                        assert_eq!(account_stats.total_registered_accounts, 1.into());
                        assert_eq!(account_stats.total_near_balance, account.near_balance());
                    },
                );
            }

            #[test]
            fn unknown_account_with_over_payment() {
                let deposit_amount: YoctoNear = (storage_balance_min().value() * 3).into();
                run_test(
                    deposit_amount,
                    false,
                    |service, storage_balance: StorageBalance| {
                        // Assert
                        assert_eq!(storage_balance.total, deposit_amount);
                        assert_eq!(
                            storage_balance.available,
                            (service.storage_balance_bounds().min.value() * 2).into()
                        );

                        // Assert account was registered
                        let account = service.registered_account_near_data(PREDECESSOR_ACCOUNT_ID);
                        assert_eq!(account.near_balance(), deposit_amount);

                        // AccountStorageEvent:Registered event should have been published to update stats
                        let account_stats = AccountManager::account_metrics();
                        assert_eq!(account_stats.total_registered_accounts, 1.into());
                        assert_eq!(account_stats.total_near_balance, account.near_balance());
                    },
                );
            }

            #[test]
            fn already_registered() {
                run_test(
                    storage_balance_min(),
                    true,
                    |service, storage_balance: StorageBalance| {
                        assert_eq!(
                            storage_balance.total.value(),
                            service.storage_balance_bounds().min.value() * 2
                        );
                        assert_eq!(
                            storage_balance.available,
                            service.storage_balance_bounds().min
                        );

                        let account = service.registered_account_near_data(PREDECESSOR_ACCOUNT_ID);
                        // AccountStorageEvent:Registered event should have been published to update stats
                        let account_stats = AccountManager::account_metrics();
                        assert_eq!(account_stats.total_registered_accounts, 1.into());
                        assert_eq!(account_stats.total_near_balance, account.near_balance());
                    },
                );
            }

            #[test]
            #[should_panic(expected = "[ERR] [INSUFFICIENT_NEAR_DEPOSIT]")]
            fn zero_deposit_attached() {
                run_test(0.into(), false, |_service, _storage_balance| {});
            }

            #[test]
            fn zero_deposit_attached_already_registered() {
                run_test(0.into(), true, |service, storage_balance| {
                    let storage_balance_bounds = service.storage_balance_bounds();
                    assert_eq!(storage_balance.total, storage_balance_bounds.min);
                    assert_eq!(storage_balance.available, 0.into());
                });
            }

            #[test]
            fn one_deposit_attached_already_registered() {
                run_test(1.into(), true, |service, storage_balance| {
                    let storage_balance_bounds = service.storage_balance_bounds();
                    assert_eq!(
                        storage_balance.total.value(),
                        storage_balance_bounds.min.value() + 1
                    );
                    assert_eq!(storage_balance.available, 1.into());
                });
            }
        }

        #[cfg(test)]
        mod deposit_for_other_with_registration_only_false {
            use super::*;

            const ACCOUNT_ID: &str = "alfio.near";

            fn run_test<F>(
                deposit: YoctoNear,
                already_registered: bool, // if true, then the account ID will be registered before hand using storage balance min
                test: F,
            ) where
                F: FnOnce(AccountManagementComponent<()>, StorageBalance),
            {
                super::run_test(
                    STORAGE_USAGE_BOUNDS,
                    Some(ACCOUNT_ID),
                    Some(false),
                    deposit,
                    already_registered,
                    test,
                );
            }

            #[test]
            fn unknown_account_with_exact_storage_deposit() {
                run_test(
                    storage_balance_min(),
                    false,
                    |service, storage_balance: StorageBalance| {
                        assert_eq!(storage_balance.total, service.storage_balance_bounds().min);
                        assert_eq!(storage_balance.available, 0.into());

                        let account = service.registered_account_near_data(ACCOUNT_ID);
                        assert_eq!(account.near_balance(), service.storage_balance_bounds().min);

                        // AccountStorageEvent:Registered event should have been published to update stats
                        let account_stats = AccountManager::account_metrics();
                        assert_eq!(account_stats.total_registered_accounts, 1.into());
                        assert_eq!(account_stats.total_near_balance, account.near_balance());
                    },
                );
            }

            #[test]
            fn unknown_account_with_over_payment() {
                let deposit_amount: YoctoNear = (storage_balance_min().value() * 3).into();
                run_test(
                    deposit_amount,
                    false,
                    |service, storage_balance: StorageBalance| {
                        // Assert
                        assert_eq!(storage_balance.total, deposit_amount);
                        assert_eq!(
                            storage_balance.available,
                            (service.storage_balance_bounds().min.value() * 2).into()
                        );

                        // Assert account was registered
                        let account = service.registered_account_near_data(ACCOUNT_ID);
                        assert_eq!(account.near_balance(), deposit_amount);

                        // AccountStorageEvent:Registered event should have been published to update stats
                        let account_stats = AccountManager::account_metrics();
                        assert_eq!(account_stats.total_registered_accounts, 1.into());
                        assert_eq!(account_stats.total_near_balance, account.near_balance());
                    },
                );
            }

            #[test]
            fn already_registered() {
                run_test(
                    storage_balance_min(),
                    true,
                    |service, storage_balance: StorageBalance| {
                        assert_eq!(
                            storage_balance.total.value(),
                            service.storage_balance_bounds().min.value() * 2
                        );
                        assert_eq!(
                            storage_balance.available,
                            service.storage_balance_bounds().min
                        );

                        let account = service.registered_account_near_data(ACCOUNT_ID);
                        // AccountStorageEvent:Registered event should have been published to update stats
                        let account_stats = AccountManager::account_metrics();
                        assert_eq!(account_stats.total_registered_accounts, 1.into());
                        assert_eq!(account_stats.total_near_balance, account.near_balance());
                    },
                );
            }

            #[test]
            #[should_panic(expected = "[ERR] [INSUFFICIENT_NEAR_DEPOSIT]")]
            fn zero_deposit_attached() {
                run_test(0.into(), false, |_service, _storage_balance| {});
            }

            #[test]
            fn zero_deposit_attached_already_registered() {
                run_test(0.into(), true, |service, storage_balance| {
                    let storage_balance_bounds = service.storage_balance_bounds();
                    assert_eq!(storage_balance.total, storage_balance_bounds.min);
                    assert_eq!(storage_balance.available, 0.into());
                });
            }

            #[test]
            fn one_deposit_attached_already_registered() {
                run_test(1.into(), true, |service, storage_balance| {
                    let storage_balance_bounds = service.storage_balance_bounds();
                    assert_eq!(
                        storage_balance.total.value(),
                        storage_balance_bounds.min.value() + 1
                    );
                    assert_eq!(storage_balance.available, 1.into());
                });
            }
        }
    }

    #[cfg(test)]
    mod test_storage_withdraw {
        use super::*;

        pub type AccountManager = AccountManagementComponent<()>;

        fn run_test<F>(
            storage_usage_bounds: StorageUsageBounds,
            deposit: YoctoNear,
            withdraw_deposit: YoctoNear,
            withdrawal: Option<YoctoNear>,
            test: F,
        ) where
            F: FnOnce(AccountManagementComponent<()>, StorageBalance),
        {
            let mut ctx = new_context(PREDECESSOR_ACCOUNT_ID);
            testing_env!(ctx.clone());

            AccountMetrics::register_account_storage_event_handler();
            AccountMetrics::reset();

            AccountStorageUsageComponent::deploy(Some(storage_usage_bounds));

            let mut service: AccountManagementComponent<()> =
                AccountManagementComponent::new(Box::new(UnregisterAccountNOOP));

            if deposit.value() > 0 {
                ctx.attached_deposit = deposit.value();
                testing_env!(ctx.clone());
                service.storage_deposit(None, None);
            }

            ctx.attached_deposit = withdraw_deposit.value();
            testing_env!(ctx.clone());
            let storage_balance = service.storage_withdraw(withdrawal);
            test(service, storage_balance);
        }

        #[test]
        fn withdraw_amount_success() {
            run_test(
                STORAGE_USAGE_BOUNDS,
                storage_balance_min() * 2,
                1.into(),
                Some(storage_balance_min() / 2),
                |service, storage_balance| {
                    assert_eq!(
                        storage_balance.total,
                        storage_balance_min() + (storage_balance_min() / 2).value()
                    );
                    assert_eq!(storage_balance.available, storage_balance_min() / 2);

                    // Assert account NEAR balance was persisted
                    let storage_balance_2 = service
                        .storage_balance_of(to_valid_account_id(PREDECESSOR_ACCOUNT_ID))
                        .unwrap();
                    assert_eq!(storage_balance, storage_balance_2);

                    // check refund was sent
                    let receipts = deserialize_receipts();
                    let receipt = &receipts[0];
                    assert_eq!(&receipt.receiver_id, PREDECESSOR_ACCOUNT_ID);
                    let action = &receipt.actions[0];
                    match action {
                        Action::Transfer(transfer) => {
                            assert_eq!(transfer.deposit, storage_balance_min().value() / 2 + 1);
                        }
                        _ => panic!("expected TransferAction"),
                    }

                    // check account stats
                    let stats = AccountManager::account_metrics();
                    assert_eq!(stats.total_near_balance, storage_balance.total);
                },
            );
        }

        #[test]
        fn withdraw_all_available_balance_success() {
            run_test(
                STORAGE_USAGE_BOUNDS,
                storage_balance_min() * 2,
                1.into(),
                None,
                |service, storage_balance| {
                    assert_eq!(storage_balance.total, storage_balance_min());
                    assert_eq!(storage_balance.available, 0.into());

                    // Assert account NEAR balance was persisted
                    let storage_balance_2 = service
                        .storage_balance_of(to_valid_account_id(PREDECESSOR_ACCOUNT_ID))
                        .unwrap();
                    assert_eq!(storage_balance, storage_balance_2);

                    // check refund was sent
                    let receipts = deserialize_receipts();
                    let receipt = &receipts[0];
                    assert_eq!(&receipt.receiver_id, PREDECESSOR_ACCOUNT_ID);
                    let action = &receipt.actions[0];
                    match action {
                        Action::Transfer(transfer) => {
                            assert_eq!(transfer.deposit, storage_balance_min().value() + 1);
                        }
                        _ => panic!("expected TransferAction"),
                    }

                    // check account stats
                    let stats = AccountManager::account_metrics();
                    assert_eq!(stats.total_near_balance, storage_balance.total);
                },
            );
        }

        #[test]
        fn withdraw_zero() {
            run_test(
                STORAGE_USAGE_BOUNDS,
                storage_balance_min() * 2,
                1.into(),
                Some(0.into()),
                |service, storage_balance| {
                    assert_eq!(storage_balance.total, storage_balance_min() * 2);
                    assert_eq!(storage_balance.available, storage_balance_min());

                    // Assert account NEAR balance was persisted
                    let storage_balance_2 = service
                        .storage_balance_of(to_valid_account_id(PREDECESSOR_ACCOUNT_ID))
                        .unwrap();
                    assert_eq!(storage_balance, storage_balance_2);

                    // check refund was sent
                    let receipts = deserialize_receipts();
                    assert!(receipts.is_empty());

                    // check account stats
                    let stats = AccountManager::account_metrics();
                    assert_eq!(stats.total_near_balance, storage_balance.total);
                },
            );
        }

        #[test]
        #[should_panic(expected = "[ERR] [YOCTONEAR_DEPOSIT_REQUIRED]")]
        fn no_attached_deposit() {
            run_test(
                STORAGE_USAGE_BOUNDS,
                storage_balance_min() * 2,
                0.into(),
                Some(0.into()),
                |_service, _storage_balance| {},
            );
        }

        #[test]
        #[should_panic(expected = "[ERR] [YOCTONEAR_DEPOSIT_REQUIRED]")]
        fn two_yoctonear_attached() {
            run_test(
                STORAGE_USAGE_BOUNDS,
                storage_balance_min() * 2,
                2.into(),
                Some(0.into()),
                |_service, _storage_balance| {},
            );
        }

        #[test]
        #[should_panic(expected = "[ERR] [INSUFFICIENT_STORAGE_BALANCE]")]
        fn insufficient_funds() {
            run_test(
                STORAGE_USAGE_BOUNDS,
                storage_balance_min(),
                1.into(),
                Some(1.into()),
                |_service, _storage_balance| {},
            );
        }

        #[test]
        #[should_panic(expected = "[ERR] [ACCOUNT_NOT_REGISTERED]")]
        fn account_not_registered() {
            run_test(
                STORAGE_USAGE_BOUNDS,
                0.into(),
                1.into(),
                Some(0.into()),
                |_service, _storage_balance| {},
            );
        }
    }

    #[cfg(test)]
    mod test_storage_unregister_with_default_unregister_delegate {
        use super::*;

        pub type AccountManager = AccountManagementComponent<()>;

        fn run_test<F>(
            storage_usage_bounds: StorageUsageBounds,
            deposit: YoctoNear,
            unregister_deposit: YoctoNear,
            force: Option<bool>,
            test: F,
        ) where
            F: FnOnce(AccountManagementComponent<()>, bool),
        {
            let mut ctx = new_context(PREDECESSOR_ACCOUNT_ID);
            testing_env!(ctx.clone());

            AccountMetrics::register_account_storage_event_handler();
            AccountMetrics::reset();

            AccountStorageUsageComponent::deploy(Some(storage_usage_bounds));

            let mut service: AccountManagementComponent<()> =
                AccountManagementComponent::new(Box::new(UnregisterAccountNOOP));

            if deposit.value() > 0 {
                ctx.attached_deposit = deposit.value();
                testing_env!(ctx.clone());
                service.storage_deposit(None, None);
            }

            ctx.attached_deposit = unregister_deposit.value();
            testing_env!(ctx.clone());
            let result = service.storage_unregister(force);
            test(service, result);
        }

        #[test]
        fn unregister_force_none_success() {
            run_test(
                STORAGE_USAGE_BOUNDS,
                storage_balance_min() * 2,
                1.into(),
                None,
                |service, unregistered| {
                    assert!(unregistered);

                    // Assert account NEAR balance was persisted
                    let storage_balance =
                        service.storage_balance_of(to_valid_account_id(PREDECESSOR_ACCOUNT_ID));
                    assert!(storage_balance.is_none());

                    // check refund was sent
                    let receipts = deserialize_receipts();
                    let receipt = &receipts[0];
                    assert_eq!(&receipt.receiver_id, PREDECESSOR_ACCOUNT_ID);
                    let action = &receipt.actions[0];
                    match action {
                        Action::Transfer(transfer) => {
                            assert_eq!(transfer.deposit, storage_balance_min().value() * 2 + 1);
                        }
                        _ => panic!("expected TransferAction"),
                    }

                    // check account stats
                    let stats = AccountManager::account_metrics();
                    assert_eq!(stats.total_registered_accounts, 0.into());
                    assert_eq!(stats.total_near_balance, 0.into());
                    assert_eq!(stats.total_storage_usage, 0.into());

                    assert!(service
                        .storage_balance_of(to_valid_account_id(PREDECESSOR_ACCOUNT_ID))
                        .is_none());
                },
            );
        }

        #[test]
        fn unregister_force_success() {
            run_test(
                STORAGE_USAGE_BOUNDS,
                storage_balance_min() * 2,
                1.into(),
                Some(true),
                |service, unregistered| {
                    assert!(unregistered);

                    // Assert account NEAR balance was persisted
                    let storage_balance =
                        service.storage_balance_of(to_valid_account_id(PREDECESSOR_ACCOUNT_ID));
                    assert!(storage_balance.is_none());

                    // check refund was sent
                    let receipts = deserialize_receipts();
                    let receipt = &receipts[0];
                    assert_eq!(&receipt.receiver_id, PREDECESSOR_ACCOUNT_ID);
                    let action = &receipt.actions[0];
                    match action {
                        Action::Transfer(transfer) => {
                            assert_eq!(transfer.deposit, storage_balance_min().value() * 2 + 1);
                        }
                        _ => panic!("expected TransferAction"),
                    }

                    // check account stats
                    let stats = AccountManager::account_metrics();
                    assert_eq!(stats.total_registered_accounts, 0.into());
                    assert_eq!(stats.total_near_balance, 0.into());
                    assert_eq!(stats.total_storage_usage, 0.into());

                    assert!(service
                        .storage_balance_of(to_valid_account_id(PREDECESSOR_ACCOUNT_ID))
                        .is_none());
                },
            );
        }

        #[test]
        #[should_panic(expected = "[ERR] [YOCTONEAR_DEPOSIT_REQUIRED]")]
        fn no_attached_deposit() {
            run_test(
                STORAGE_USAGE_BOUNDS,
                storage_balance_min() * 2,
                0.into(),
                None,
                |_service, _storage_balance| {},
            );
        }

        #[test]
        #[should_panic(expected = "[ERR] [YOCTONEAR_DEPOSIT_REQUIRED]")]
        fn two_yoctonear_attached() {
            run_test(
                STORAGE_USAGE_BOUNDS,
                storage_balance_min() * 2,
                2.into(),
                None,
                |_service, _storage_balance| {},
            );
        }

        #[test]
        fn account_not_registered() {
            run_test(
                STORAGE_USAGE_BOUNDS,
                0.into(),
                1.into(),
                None,
                |_service, unregistered| assert!(!unregistered),
            );
        }
    }

    #[cfg(test)]
    mod test_storage_unregister {
        use super::*;
        use oysterpack_smart_near::YOCTO;

        struct UnregisterMock {
            fail: bool,
        }

        impl UnregisterAccount for UnregisterMock {
            fn unregister_account(&mut self, force: bool) {
                if !force && self.fail {
                    panic!("account cannot be unregistered because ...");
                }
            }
        }

        pub type AccountManager = AccountManagementComponent<()>;

        #[test]
        #[should_panic(expected = "account cannot be unregistered because ")]
        fn unregister_panics() {
            // Arrange
            let account = "alfio";
            let mut ctx = new_context(account);
            testing_env!(ctx.clone());

            AccountManager::deploy(Some(StorageUsageBounds {
                min: 1000.into(),
                max: None,
            }));

            let mut service = AccountManager::new(Box::new(UnregisterMock { fail: true }));
            ctx.attached_deposit = YOCTO;
            testing_env!(ctx.clone());
            service.storage_deposit(None, None);

            // Act
            ctx.attached_deposit = 1;
            testing_env!(ctx.clone());
            service.storage_unregister(None);
        }

        #[test]
        fn force_unregister_panics() {
            // Arrange
            let account = "alfio";
            let mut ctx = new_context(account);
            testing_env!(ctx.clone());

            AccountManager::deploy(Some(StorageUsageBounds {
                min: 1000.into(),
                max: None,
            }));

            let mut service = AccountManager::new(Box::new(UnregisterMock { fail: true }));
            ctx.attached_deposit = YOCTO;
            testing_env!(ctx.clone());
            service.storage_deposit(None, None);

            // Act
            ctx.attached_deposit = 1;
            testing_env!(ctx.clone());
            service.storage_unregister(Some(true));
            assert!(service
                .storage_balance_of(to_valid_account_id(account))
                .is_none());
        }
    }
}

#[cfg(test)]
mod tests_account_storage_usage {
    use super::*;
    use oysterpack_smart_near::YOCTO;
    use oysterpack_smart_near_test::*;

    type AccountManager = AccountManagementComponent<()>;

    #[test]
    fn test() {
        let account = "alfio";
        let mut ctx = new_context(account);
        testing_env!(ctx.clone());

        let storage_usage_bounds = StorageUsageBounds {
            min: AccountManager::measure_storage_usage(()),
            max: None,
        };
        println!("measured storage_usage_bounds = {:?}", storage_usage_bounds);
        AccountManager::deploy(Some(storage_usage_bounds));

        let mut service = AccountManager::new(Box::new(UnregisterAccountNOOP));
        assert_eq!(storage_usage_bounds, service.storage_usage_bounds());

        assert!(service
            .storage_balance_of(to_valid_account_id(account))
            .is_none());

        ctx.attached_deposit = YOCTO;
        testing_env!(ctx.clone());
        let storage_balance = service.storage_deposit(None, None);
        assert_eq!(
            service
                .storage_balance_of(to_valid_account_id(account))
                .unwrap(),
            storage_balance
        );
    }
}

#[cfg(test)]
mod tests_account_metrics {
    use super::*;
    use oysterpack_smart_near::YOCTO;
    use oysterpack_smart_near_test::*;

    type AccountManager = AccountManagementComponent<()>;

    #[test]
    fn test() {
        // Arrange - 0 accounts register
        let account = "alfio";
        let mut ctx = new_context(account);
        testing_env!(ctx.clone());

        let storage_usage_bounds = StorageUsageBounds {
            min: AccountManager::measure_storage_usage(()),
            max: None,
        };
        println!("measured storage_usage_bounds = {:?}", storage_usage_bounds);
        AccountManager::deploy(Some(storage_usage_bounds));

        let mut service = AccountManager::new(Box::new(UnregisterAccountNOOP));
        // Act
        let metrics = AccountManager::account_metrics();
        // Assert
        assert_eq!(metrics.total_registered_accounts.value(), 0);
        assert_eq!(metrics.total_near_balance.value(), 0);
        assert_eq!(metrics.total_storage_usage.value(), 0);

        // Arrange - register account
        ctx.attached_deposit = YOCTO;
        testing_env!(ctx.clone());
        let storage_balance = service.storage_deposit(None, None);
        let account_data = service.load_account_data(account);
        assert!(account_data.is_none());
        let mut account_data: AccountDataObject<()> = AccountDataObject::new(account, ());
        account_data.save();

        // Act
        let metrics = AccountManager::account_metrics();
        // Assert
        assert_eq!(metrics.total_registered_accounts.value(), 1);
        assert_eq!(metrics.total_near_balance, storage_balance.total);
        assert_eq!(metrics.total_storage_usage, storage_usage_bounds.min);

        // Arrange - deposit more funds
        ctx.attached_deposit = YOCTO;
        testing_env!(ctx.clone());
        let storage_balance = service.storage_deposit(None, None);
        // Act
        let metrics = AccountManager::account_metrics();
        // Assert
        assert_eq!(metrics.total_registered_accounts.value(), 1);
        assert_eq!(metrics.total_near_balance, storage_balance.total);
        assert_eq!(metrics.total_storage_usage, storage_usage_bounds.min);

        // Arrange - register another account
        ctx.attached_deposit = YOCTO;
        testing_env!(ctx.clone());
        let bob_storage_balance = service.storage_deposit(Some(to_valid_account_id("bob")), None);
        let mut account_data: AccountDataObject<()> = AccountDataObject::new("bob", ());
        account_data.save();
        // Act
        let metrics = AccountManager::account_metrics();
        // Assert
        assert_eq!(metrics.total_registered_accounts.value(), 2);
        assert_eq!(
            metrics.total_near_balance,
            storage_balance.total + bob_storage_balance.total
        );
        assert_eq!(
            metrics.total_storage_usage.value(),
            storage_usage_bounds.min.value() * 2
        );

        // Arrange - unregister account
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        service.storage_unregister(None);
        // Act
        let metrics = AccountManager::account_metrics();
        // Assert
        assert_eq!(metrics.total_registered_accounts.value(), 1);
        assert_eq!(metrics.total_near_balance, bob_storage_balance.total);
        assert_eq!(
            metrics.total_storage_usage.value(),
            storage_usage_bounds.min.value()
        );
    }
}

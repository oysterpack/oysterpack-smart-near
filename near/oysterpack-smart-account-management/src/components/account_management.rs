use crate::{
    Account, AccountReporting, AccountRepository, AccountStorageEvent, AccountStorageUsage,
    HasAccountStorageUsage, StorageBalance, StorageBalanceBounds, StorageManagement,
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
    eventbus,
};
use std::{fmt::Debug, ops::Deref};
use teloc::*;

use crate::components::account_storage_usage::AccountStorageUsageComponent;

/// Core account management component implements the following interfaces:
/// 1. [`AccountRepository`]
/// 2. [`StorageManagement`] - NEP-145
/// 3. ['AccountReporting`]
/// 4. ['AccountStorageUsage`]
#[derive(Default)]
pub struct AccountManagementComponent<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    /// must be provided by the contract
    unregister: Option<Box<dyn UnregisterAccount>>,

    account_storage_usage: AccountStorageUsageComponent<T>,
}

pub trait UnregisterAccount {
    fn unregister_account(&mut self, force: bool);
}

/// constructor
#[inject]
impl<T> AccountManagementComponent<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    pub fn new(unregister: Box<dyn UnregisterAccount>) -> Self {
        Self {
            unregister: Some(unregister),
            account_storage_usage: Default::default(),
        }
    }
}

impl<T> AccountRepository<T> for AccountManagementComponent<T> where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default
{
}

impl<T> AccountReporting for AccountManagementComponent<T> where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default
{
}

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
    /// Payable method that receives an attached deposit of Ⓝ for a given account.
    ///
    /// If `account_id` is omitted, the deposit MUST go toward predecessor account.
    /// If provided, deposit MUST go toward this account. If invalid, contract MUST panic.
    ///
    /// If `registration_only=true`:
    /// - and if the account wasn't registered, then the contract MUST refund above the minimum balance  
    /// - and if the account was registered, then the contract MUST refund full deposit if already registered.
    ///
    /// Any attached deposit in excess of `storage_balance_bounds.max` must be refunded to predecessor account.
    ///
    /// Returns the StorageBalance structure showing updated balances.
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

        let account: Account<T> = match self.load_account(&account_id) {
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
            None => {
                let deposit =
                    self.initial_deposit(deposit, registration_only, storage_balance_bounds);
                let account = self.new_account(&account_id, deposit, Default::default());
                self.register(account, storage_balance_bounds)
            }
        };

        account.storage_balance(storage_balance_bounds.min)
    }

    fn storage_withdraw(&mut self, amount: Option<YoctoNear>) -> StorageBalance {
        assert_yocto_near_attached();
        unimplemented!()
    }

    fn storage_unregister(&mut self, force: Option<bool>) -> bool {
        assert_yocto_near_attached();
        match self.load_account(env::predecessor_account_id().as_str()) {
            None => false,
            Some(account) => {
                self.unregister
                    .as_mut()
                    .unwrap()
                    .unregister_account(force.unwrap_or(false));
                send_refund(account.near_balance());
                true
            }
        }
    }

    fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        self.account_storage_usage.storage_usage_bounds().into()
    }

    fn storage_balance_of(&self, account_id: ValidAccountId) -> Option<StorageBalance> {
        self.load_account(account_id.as_ref())
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
    /// refunds deposit amount that is above the max allowed storage balance
    fn deposit_with_max_bound(
        &self,
        account: &mut Account<T>,
        deposit: Deposit,
        max: MaxStorageBalance,
    ) {
        if account.near_balance() < *max {
            let max_allowed_deposit = max.value() - account.near_balance().value();
            let deposit = if deposit.value() > max_allowed_deposit {
                // refund amount over the upper bound
                send_refund(deposit.value() - max_allowed_deposit);
                Deposit(max_allowed_deposit.into())
            } else {
                deposit
            };

            self.deposit(account, *deposit);
        } else {
            // account storage balance is already at max limit - thus refund the full deposit amount
            send_refund(deposit.value());
        }
    }

    fn deposit(&self, account: &mut Account<T>, deposit: YoctoNear) {
        eventbus::post(&AccountStorageEvent::Deposit(deposit));
        account.incr_near_balance(deposit);
        account.save();
    }

    fn register(
        &self,
        account: Account<T>,
        storage_balance_bounds: StorageBalanceBounds,
    ) -> Account<T> {
        eventbus::post(&AccountStorageEvent::Registered(
            account.storage_balance(storage_balance_bounds.min),
            account.storage_usage(),
        ));
        account.save();
        account
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
    use oysterpack_smart_near::service::*;
    use oysterpack_smart_near_test::*;

    fn deploy_account_service() {
        AccountStorageUsageComponent::<()>::deploy(Some(StorageUsageBounds {
            min: 1000.into(),
            max: None,
        }));
    }

    struct UnregisterMock;

    impl UnregisterAccount for UnregisterMock {
        fn unregister_account(&mut self, force: bool) {}
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
            AccountManagementComponent::new(Box::new(UnregisterMock));
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
    use oysterpack_smart_near::service::*;
    use oysterpack_smart_near_test::*;

    fn deploy_account_service() {
        AccountStorageUsageComponent::<()>::deploy(Some(StorageUsageBounds {
            min: 1000.into(),
            max: None,
        }));
    }

    #[derive(Dependency)]
    struct UnregisterMock;

    impl UnregisterAccount for UnregisterMock {
        fn unregister_account(&mut self, force: bool) {}
    }

    impl From<Box<UnregisterMock>> for Box<dyn UnregisterAccount> {
        fn from(x: Box<UnregisterMock>) -> Self {
            x
        }
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
            .add_transient_c::<Box<dyn UnregisterAccount>, Box<UnregisterMock>>()
            .add_transient::<AccountManagementComponent<()>>();

        let service: AccountManagementComponent<()> = container.resolve();
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
mod tests_storage_deposit {
    use super::*;
    use crate::{AccountStats, StorageUsageBounds};
    use oysterpack_smart_near::domain::StorageUsage;
    use oysterpack_smart_near::service::*;
    use oysterpack_smart_near_test::*;

    struct UnregisterMock;

    impl UnregisterAccount for UnregisterMock {
        fn unregister_account(&mut self, force: bool) {}
    }

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

        AccountStats::register_account_storage_event_handler();
        AccountStats::reset();

        AccountStorageUsageComponent::<()>::deploy(Some(storage_usage_bounds));

        ctx.attached_deposit = deposit.value();
        testing_env!(ctx.clone());

        let mut service: AccountManagementComponent<()> =
            AccountManagementComponent::new(Box::new(UnregisterMock));
        let storage_balance_bounds = service.storage_balance_bounds();

        if already_registered {
            let account = service.new_account(
                account_id.unwrap_or(PREDECESSOR_ACCOUNT_ID),
                storage_balance_bounds.min,
                (),
            );
            account.save();
            eventbus::post(&AccountStorageEvent::Registered(
                account.storage_balance(storage_balance_bounds.min),
                account.storage_usage(),
            ));
        }

        let storage_balance =
            service.storage_deposit(account_id.map(to_valid_account_id), registration_only);

        test(service, storage_balance);
    }

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

                    let account = service.registered_account(PREDECESSOR_ACCOUNT_ID);
                    assert_eq!(account.near_balance(), service.storage_balance_bounds().min);

                    // AccountStorageEvent:Registered event should have been published to update stats
                    let account_stats = service.account_stats();
                    assert_eq!(account_stats.total_registered_accounts, 1.into());
                    assert_eq!(account_stats.total_near_balance, account.near_balance());
                    assert_eq!(
                        account_stats.total_storage_usage,
                        account.serialized_byte_size().into()
                    );
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

                    // Assert account was registered
                    let account = service.registered_account(PREDECESSOR_ACCOUNT_ID);
                    assert_eq!(account.near_balance(), service.storage_balance_bounds().min);

                    // AccountStorageEvent:Registered event should have been published to update stats
                    let account_stats = service.account_stats();
                    assert_eq!(account_stats.total_registered_accounts, 1.into());
                    assert_eq!(account_stats.total_near_balance, account.near_balance());
                    assert_eq!(
                        account_stats.total_storage_usage,
                        account.serialized_byte_size().into()
                    );

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

                    let account = service.registered_account(ACCOUNT_ID);
                    assert_eq!(account.near_balance(), service.storage_balance_bounds().min);

                    // AccountStorageEvent:Registered event should have been published to update stats
                    let account_stats = service.account_stats();
                    assert_eq!(account_stats.total_registered_accounts, 1.into());
                    assert_eq!(account_stats.total_near_balance, account.near_balance());
                    assert_eq!(
                        account_stats.total_storage_usage,
                        account.serialized_byte_size().into()
                    );
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

                    // Assert account was registered
                    let account = service.registered_account(ACCOUNT_ID);
                    assert_eq!(account.near_balance(), service.storage_balance_bounds().min);

                    // AccountStorageEvent:Registered event should have been published to update stats
                    let account_stats = service.account_stats();
                    assert_eq!(account_stats.total_registered_accounts, 1.into());
                    assert_eq!(account_stats.total_near_balance, account.near_balance());
                    assert_eq!(
                        account_stats.total_storage_usage,
                        account.serialized_byte_size().into()
                    );

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

                    let account = service.registered_account(PREDECESSOR_ACCOUNT_ID);
                    assert_eq!(account.near_balance(), service.storage_balance_bounds().min);

                    // AccountStorageEvent:Registered event should have been published to update stats
                    let account_stats = service.account_stats();
                    assert_eq!(account_stats.total_registered_accounts, 1.into());
                    assert_eq!(account_stats.total_near_balance, account.near_balance());
                    assert_eq!(
                        account_stats.total_storage_usage,
                        account.serialized_byte_size().into()
                    );
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
                    let account = service.registered_account(PREDECESSOR_ACCOUNT_ID);
                    assert_eq!(account.near_balance(), deposit_amount);

                    // AccountStorageEvent:Registered event should have been published to update stats
                    let account_stats = service.account_stats();
                    assert_eq!(account_stats.total_registered_accounts, 1.into());
                    assert_eq!(account_stats.total_near_balance, account.near_balance());
                    assert_eq!(
                        account_stats.total_storage_usage,
                        account.serialized_byte_size().into()
                    );
                },
            );
        }

        #[test]
        fn unknown_account_with_over_payment_above_max_bounce() {
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

                    // Assert account was registered
                    let account = service.registered_account(PREDECESSOR_ACCOUNT_ID);
                    assert_eq!(account.near_balance(), storage_balance_bounds.max.unwrap());

                    // AccountStorageEvent:Registered event should have been published to update stats
                    let account_stats = service.account_stats();
                    assert_eq!(account_stats.total_registered_accounts, 1.into());
                    assert_eq!(account_stats.total_near_balance, account.near_balance());
                    assert_eq!(
                        account_stats.total_storage_usage,
                        account.serialized_byte_size().into()
                    );

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

                    let account = service.registered_account(PREDECESSOR_ACCOUNT_ID);
                    assert_eq!(account.near_balance(), service.storage_balance_bounds().min);

                    // AccountStorageEvent:Registered event should have been published to update stats
                    let account_stats = service.account_stats();
                    assert_eq!(account_stats.total_registered_accounts, 1.into());
                    assert_eq!(account_stats.total_near_balance, account.near_balance());
                    assert_eq!(
                        account_stats.total_storage_usage,
                        account.serialized_byte_size().into()
                    );
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
                    let account = service.registered_account(PREDECESSOR_ACCOUNT_ID);
                    assert_eq!(account.near_balance(), deposit_amount);

                    // AccountStorageEvent:Registered event should have been published to update stats
                    let account_stats = service.account_stats();
                    assert_eq!(account_stats.total_registered_accounts, 1.into());
                    assert_eq!(account_stats.total_near_balance, account.near_balance());
                    assert_eq!(
                        account_stats.total_storage_usage,
                        account.serialized_byte_size().into()
                    );
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

                    let account = service.registered_account(PREDECESSOR_ACCOUNT_ID);
                    // AccountStorageEvent:Registered event should have been published to update stats
                    let account_stats = service.account_stats();
                    assert_eq!(account_stats.total_registered_accounts, 1.into());
                    assert_eq!(account_stats.total_near_balance, account.near_balance());
                    assert_eq!(
                        account_stats.total_storage_usage,
                        account.serialized_byte_size().into()
                    );
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

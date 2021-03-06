use crate::{
    Account, AccountStorageEvent, AccountTracking, StorageBalance, StorageBalanceBounds,
    StorageManagement,
};
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env,
    json_types::ValidAccountId,
    AccountId, Promise,
};
use oysterpack_smart_near::service::{Deploy, Init, Service};
use oysterpack_smart_near::{
    asserts::{assert_min_near_attached, assert_yocto_near_attached},
    domain::YoctoNear,
    EVENT_BUS,
};
use std::{fmt::Debug, marker::PhantomData, ops::Deref, sync::Mutex};

pub struct AccountService<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    storage_balance_bounds: StorageBalanceBounds,
    _phantom: PhantomData<T>,
}

impl<T> Service for AccountService<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    type State = StorageBalanceBounds;

    fn state_key() -> u128 {
        1952475351321611295376996018476025471
    }
}

impl<T> Init for AccountService<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    fn init() -> Self {
        let state = Self::load_state().unwrap();
        Self {
            storage_balance_bounds: *state,
            _phantom: Default::default(),
        }
    }
}

impl<T> Deploy for AccountService<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    fn deploy<F>(initial_state_provider: Option<F>) -> Self
    where
        F: FnOnce() -> Self::State,
    {
        let state = initial_state_provider.expect("initial state must be provided")();
        let state = Self::new_state(state);
        state.save();
        Self {
            storage_balance_bounds: *state,
            _phantom: Default::default(),
        }
    }
}

/// account related functions to make it more ergonomic to work with by encapsulating the [`AccountData`]
/// generic type
impl<T> AccountService<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    /// Creates a new in memory account object
    pub fn new_account(&self, account_id: &str, near_balance: YoctoNear, data: T) -> Account<T> {
        Account::<T>::new(account_id, near_balance, data)
    }

    /// tries to load the account from storage
    pub fn load_account(&self, account_id: &str) -> Option<Account<T>> {
        Account::<T>::load(account_id)
    }

    /// ## Panics
    /// if the account is not registered
    pub fn registered_account(&self, account_id: &str) -> Account<T> {
        Account::<T>::registered_account(account_id)
    }

    pub fn account_exists(&self, account_id: &str) -> bool {
        Account::<T>::exists(account_id)
    }
}

impl<T> StorageManagement for AccountService<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
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
                    Promise::new(account_id).transfer(deposit.value());
                } else {
                    if let Some(max) = storage_balance_bounds.max {
                        self.deposit_with_max_bound(
                            account_id,
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
                let deposit = self.initial_deposit(&account_id, deposit, registration_only);
                let account = self.new_account(&account_id, deposit, Default::default());
                self.register(account)
            }
        };

        account.storage_balance(storage_balance_bounds.min)
    }

    fn storage_withdraw(amount: Option<YoctoNear>) -> StorageBalance {
        assert_yocto_near_attached();
        unimplemented!()
    }

    fn storage_unregister(force: Option<bool>) -> bool {
        assert_yocto_near_attached();
        unimplemented!()
    }

    fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        self.storage_balance_bounds
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

impl<T> AccountTracking for AccountService<T> where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default
{
}

/// helper functions
impl<T> AccountService<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    /// refunds deposit amount that is above the max allowed storage balance
    fn deposit_with_max_bound(
        &self,
        account_id: AccountId,
        account: &mut Account<T>,
        deposit: Deposit,
        max: MaxStorageBalance,
    ) {
        if account.near_balance() < *max {
            let max_allowed_deposit = max.value() - account.near_balance().value();
            let deposit = if deposit.value() > max_allowed_deposit {
                // refund amount over the upper bound
                Promise::new(account_id).transfer(deposit.value() - max_allowed_deposit);
                Deposit(max_allowed_deposit.into())
            } else {
                deposit
            };

            self.deposit(account, *deposit);
        } else {
            // account storage balance is already at max limit - thus refund the full deposit amount
            Promise::new(account_id).transfer(deposit.value());
        }
    }

    fn deposit(&self, account: &mut Account<T>, deposit: YoctoNear) {
        EVENT_BUS.post(&AccountStorageEvent::Deposit(deposit));
        account.incr_near_balance(deposit);
        account.save();
    }

    fn register(&self, account: Account<T>) -> Account<T> {
        EVENT_BUS.post(&AccountStorageEvent::Registered(
            account.storage_balance(self.storage_balance_bounds().min),
        ));
        account.save();
        account
    }

    fn initial_deposit(
        &self,
        account_id: &str,
        deposit: YoctoNear,
        registration_only: bool,
    ) -> YoctoNear {
        if registration_only {
            // only take the min required and refund the rest
            let refund_amount = deposit.value() - self.storage_balance_bounds().min.value();
            if refund_amount > 0 {
                Promise::new(account_id.to_string()).transfer(refund_amount);
            }
            self.storage_balance_bounds().min
        } else {
            // refund deposit that is over the max allowed
            self.storage_balance_bounds().max.map_or(deposit, |max| {
                if deposit.value() > max.value() {
                    let refund_amount = deposit.value() - max.value();
                    Promise::new(account_id.to_string()).transfer(refund_amount);
                    max
                } else {
                    deposit
                }
            })
        }
    }
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
    use lazy_static::lazy_static;
    use oysterpack_smart_near::YOCTO;
    use oysterpack_smart_near_test::*;

    lazy_static! {
        pub static ref ACCOUNT_SERVICE: Mutex<AccountService<()>> =
            Mutex::new(AccountService::<()>::init());
    }

    fn deploy_account_service() {
        AccountService::<()>::deploy(Some(|| StorageBalanceBounds {
            min: YOCTO.into(),
            max: None,
        }));
    }

    #[test]
    fn deploy_and_use() {
        // Arrange
        let account_id = "bob";
        let mut ctx = new_context(account_id);
        testing_env!(ctx);

        // Act
        deploy_account_service();
        let service = ACCOUNT_SERVICE.lock().unwrap();
        let storage_bounds = service.storage_balance_bounds();
        assert_eq!(storage_bounds.min, YOCTO.into());
        assert!(storage_bounds.max.is_none());
    }
}

#[cfg(test)]
mod tests_storage_deposit_for_self_registration_only {
    use super::*;
    use lazy_static::lazy_static;
    use oysterpack_smart_near::YOCTO;
    use oysterpack_smart_near_test::*;

    lazy_static! {
        pub static ref ACCOUNT_SERVICE: Mutex<AccountService<()>> =
            Mutex::new(AccountService::<()>::init());
    }

    fn deploy_account_service() {
        AccountService::<()>::deploy(Some(|| StorageBalanceBounds {
            min: YOCTO.into(),
            max: None,
        }));
    }

    #[test]
    fn unknown_account_with_exact_storage_deposit() {
        // Arrange
        let account_id = "bob";
        let mut ctx = new_context(account_id);
        testing_env!(ctx.clone());

        deploy_account_service();
        let mut service = ACCOUNT_SERVICE.lock().unwrap();

        // attach min storage
        ctx.attached_deposit = service.storage_balance_bounds().min.value();
        testing_env!(ctx.clone());

        // Act
        let storage_balance = service.storage_deposit(None, Some(true));

        // Assert
        assert_eq!(storage_balance.total, service.storage_balance_bounds().min);
        assert_eq!(storage_balance.available, 0.into());

        // Assert account was registered
        let account = service.registered_account(account_id);
        assert_eq!(account.near_balance(), service.storage_balance_bounds().min);
    }

    #[test]
    fn unknown_account_with_over_payment() {
        // Arrange
        let account_id = "bob";
        let mut ctx = new_context(account_id);
        testing_env!(ctx.clone());

        deploy_account_service();
        let mut service = ACCOUNT_SERVICE.lock().unwrap();

        // attach min storage
        ctx.attached_deposit = service.storage_balance_bounds().min.value() * 3;
        testing_env!(ctx.clone());

        // Act
        let storage_balance = service.storage_deposit(None, Some(true));

        // Assert
        assert_eq!(storage_balance.total, service.storage_balance_bounds().min);
        assert_eq!(storage_balance.available, 0.into());

        // Assert account was registered
        let account = service.registered_account(account_id);
        assert_eq!(account.near_balance(), service.storage_balance_bounds().min);

        // Assert overpayment was refunded
        let receipts = deserialize_receipts();
        assert_eq!(receipts.len(), 1);
        let receipt = &receipts[0];
        assert_eq!(&receipt.receiver_id, account_id);
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
    }

    #[test]
    fn already_registered() {
        // Arrange
        let account_id = "bob";
        let mut ctx = new_context(account_id);
        testing_env!(ctx.clone());

        deploy_account_service();
        let mut service = ACCOUNT_SERVICE.lock().unwrap();

        // attach min storage
        ctx.attached_deposit = service.storage_balance_bounds().min.value();
        testing_env!(ctx.clone());

        // register the user
        service.storage_deposit(None, Some(true));

        // Act - try registering again
        let storage_balance = service.storage_deposit(None, Some(true));
        assert_eq!(storage_balance.total, service.storage_balance_bounds().min);
        assert_eq!(storage_balance.available, 0.into());

        // Assert the deposit was refunded
        let receipts = deserialize_receipts();
        assert_eq!(receipts.len(), 1);
        let receipt = &receipts[0];
        assert_eq!(&receipt.receiver_id, account_id);
        let action = &receipt.actions[0];
        match action {
            Action::Transfer(action) => {
                assert_eq!(action.deposit, ctx.attached_deposit.into());
            }
            _ => panic!("expected Transfer"),
        }
    }

    /// NOTE: separate lazy_static is required for each test that panics because the panic
    /// causes the mutex to become posiened - which will cause any tests running after this that
    /// use the serice to fail.
    lazy_static! {
        pub static ref ACCOUNT_SERVICE_zero_deposit_attached: Mutex<AccountService<()>> =
            Mutex::new(AccountService::<()>::init());
    }

    #[test]
    #[should_panic(expected = "[ERR] [INSUFFICIENT_NEAR_DEPOSIT]")]
    fn zero_deposit_attached() {
        // Arrange
        let account_id = "bob";
        let mut ctx = new_context(account_id);
        testing_env!(ctx.clone());

        deploy_account_service();
        let mut service = ACCOUNT_SERVICE_zero_deposit_attached.lock().unwrap();

        // Act - register the user with no deposit attached
        service.storage_deposit(None, Some(true));
    }
}

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
use oysterpack_smart_near::{
    asserts::{assert_min_near_attached, assert_yocto_near_attached},
    domain::YoctoNear,
    EVENT_BUS,
};
use std::fmt::Debug;
use std::marker::PhantomData;
use std::ops::Deref;

pub struct AccountService<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    storage_balance_bounds: StorageBalanceBounds,
    _phantom: PhantomData<T>,
}

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

        let registration_only = registration_only.unwrap_or(false);
        if registration_only {
            assert_min_near_attached(self.storage_balance_bounds.min);
        }
        let deposit: YoctoNear = env::attached_deposit().into();

        let account: Account<T> = match self.load_account(&account_id) {
            Some(mut account) => {
                if registration_only {
                    // refund the full deposit
                    Promise::new(account_id).transfer(deposit.value());
                } else {
                    if let Some(max) = self.storage_balance_bounds.max {
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

        account.storage_balance(self.storage_balance_bounds.min)
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
                    - self.storage_balance_bounds.min.value())
                .into(),
            })
    }
}

// helper functions
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
            account.storage_balance(self.storage_balance_bounds.min),
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
            let refund_amount = deposit.value() - self.storage_balance_bounds.min.value();
            if refund_amount > 0 {
                Promise::new(account_id.to_string()).transfer(refund_amount);
            }
            self.storage_balance_bounds.min
        } else {
            // refund deposit that is over the max allowed
            self.storage_balance_bounds.max.map_or(deposit, |max| {
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

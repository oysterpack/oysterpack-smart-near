use crate::{
    AccountIdHash, AccountStorageEvent, Permissions, StorageBalance, ERR_ACCOUNT_NOT_REGISTERED,
};
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env,
};
use oysterpack_smart_near::{
    data::Object,
    domain::{StorageUsage, YoctoNear},
    eventbus, Hash,
};
use std::ops::{Deref, DerefMut};

type DAO = Object<AccountNearDataHash, AccountNearData>;

/// Persistent account NEAR related data
#[derive(Clone, Debug, PartialEq)]
pub struct AccountNearDataObject(DAO);

impl AccountNearDataObject {
    /// Creates a new in memory account object
    /// - its storage usage will be initialized to the serialized object byte size, but this won't
    ///   match the actual storage usage when the object is saved because there is overhead
    pub fn new(account_id: &str, near_balance: YoctoNear) -> Self {
        let object = DAO::new(
            account_id.into(),
            AccountNearData::new(near_balance, 0.into()),
        );
        Self(object)
    }

    /// tries to load the account from storage
    pub fn load<ID>(account_id: ID) -> Option<Self>
    where
        ID: Into<AccountNearDataHash>,
    {
        DAO::load(&account_id.into()).map(|object| Self(object))
    }

    /// ## Panics
    /// if the account is not registered
    pub fn registered_account<ID>(account_id: ID) -> Self
    where
        ID: Into<AccountNearDataHash>,
    {
        Self::load(account_id).unwrap_or_else(|| {
            ERR_ACCOUNT_NOT_REGISTERED.panic();
            unreachable!()
        })
    }

    pub fn exists<ID>(account_id: ID) -> bool
    where
        ID: Into<AccountNearDataHash>,
    {
        DAO::exists(&account_id.into())
    }

    pub fn save(&self) {
        let storage_usage_before_save = env::storage_usage();
        self.0.save();
        let storage_usage_after_save = env::storage_usage();
        if storage_usage_after_save == storage_usage_before_save {
            return;
        }
        let event = if storage_usage_after_save > storage_usage_before_save {
            let storage_usage_change = storage_usage_after_save - storage_usage_before_save;
            AccountStorageEvent::StorageUsageChanged(self.key().0, storage_usage_change.into())
        } else {
            let storage_usage_change = storage_usage_before_save - storage_usage_after_save;
            AccountStorageEvent::StorageUsageChanged(
                self.key().0,
                (storage_usage_change as i64 * -1).into(),
            )
        };
        eventbus::post(&event);
    }

    /// tracks storage usage - emits [`AccountStorageEvent::StorageUsageChanged`]
    pub fn delete(self) -> bool {
        let key = self.key().0;
        let storage_usage_before_save = env::storage_usage();
        let result = self.0.delete();
        let storage_usage_deleted = storage_usage_before_save - env::storage_usage();
        if storage_usage_deleted > 0 {
            eventbus::post(&AccountStorageEvent::StorageUsageChanged(
                key,
                (storage_usage_deleted as i64 * -1).into(),
            ))
        }
        result
    }
}

impl Deref for AccountNearDataObject {
    type Target = Object<AccountNearDataHash, AccountNearData>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for AccountNearDataObject {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Provides the basic fields that all accounts need:
/// - [`AccountNearData::near_balance`] - all accounts must pay for their own storage and thus need a NEAR balance
/// - [`AccountNearData::storage_usage`] - used to track account storage usage
/// - [`AccountNearData::permissions`] - many contracts will require access control. bitflags provides
///   the ability to support up to 64 roles / permission bits. The first 62 permission bits are available
///   for the contract. The last 2 permission bits are reserved by:
///   - [`crate::Permissions::ADMIN`]
///   - [`crate::Permissions::OPERATOR`]
#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq)]
pub struct AccountNearData {
    near_balance: YoctoNear,
    storage_usage: StorageUsage,
    permissions: Option<Permissions>,
}

impl AccountNearData {
    /// constructor
    pub fn new(near_balance: YoctoNear, storage_usage: StorageUsage) -> Self {
        Self {
            near_balance,
            storage_usage,
            permissions: None,
        }
    }

    pub fn near_balance(&self) -> YoctoNear {
        self.near_balance
    }

    pub fn storage_usage(&self) -> StorageUsage {
        self.storage_usage
    }

    pub fn permissions(&self) -> Option<Permissions> {
        self.permissions
    }

    pub fn storage_balance(&self, required_min_storage_balance: YoctoNear) -> StorageBalance {
        StorageBalance {
            total: self.near_balance,
            available: (self.near_balance.value() - required_min_storage_balance.value()).into(),
        }
    }

    /// Posts [`AccountStorageEvent::Deposit`] event
    ///
    /// ## Panics
    /// if overflow occurs
    pub fn incr_near_balance(&mut self, amount: YoctoNear) {
        *self.near_balance = self.near_balance.checked_add(amount.value()).unwrap();
        eventbus::post(&AccountStorageEvent::Deposit(amount));
    }

    /// Posts [`AccountStorageEvent::Withdrawal`] event
    ///
    /// ## Panics
    /// if overflow occurs
    pub fn dec_near_balance(&mut self, amount: YoctoNear) {
        *self.near_balance = self.near_balance.checked_sub(amount.value()).unwrap();
        eventbus::post(&AccountStorageEvent::Withdrawal(amount));
    }

    /// - if change was positive then it posts [`AccountStorageEvent::Deposit`] event
    /// - if change was negative, then it posts  [`AccountStorageEvent::Withdrawal`] event
    pub fn set_near_balance(&mut self, amount: YoctoNear) {
        if self.near_balance > amount {
            eventbus::post(&AccountStorageEvent::Withdrawal(self.near_balance - amount));
        } else if amount > self.near_balance {
            eventbus::post(&AccountStorageEvent::Deposit(amount - self.near_balance));
        }
        *self.near_balance = amount.value();
    }

    /// ## Panics
    /// if overflow occurs
    pub(crate) fn incr_storage_usage(&mut self, amount: StorageUsage) {
        *self.storage_usage = self.storage_usage.checked_add(amount.value()).unwrap();
    }

    /// ## Panics
    /// if overflow occurs
    pub(crate) fn decr_storage_usage(&mut self, amount: StorageUsage) {
        *self.storage_usage = self.storage_usage.checked_sub(amount.value()).unwrap();
    }

    pub fn set_permissions(&mut self, permissions: Permissions) {
        self.permissions = Some(permissions)
    }

    pub fn is_admin(&self) -> bool {
        self.permissions.map_or(false, |permissions| {
            permissions.contains(Permissions::ADMIN)
        })
    }

    pub fn grant_admin(&mut self) {
        let mut permissions = self.permissions.take().unwrap_or_else(Default::default);
        permissions.grant(Permissions::ADMIN);
        self.permissions = Some(permissions);
    }

    pub fn revoke_admin(&mut self) {
        if let Some(mut permissions) = self.permissions.take() {
            permissions.revoke(Permissions::ADMIN);
            self.permissions = Some(permissions);
        }
    }

    pub fn is_operator(&self) -> bool {
        self.permissions.map_or(false, |permissions| {
            permissions.contains(Permissions::OPERATOR) || permissions.contains(Permissions::ADMIN)
        })
    }

    pub fn grant_operator(&mut self) {
        let mut permissions = self.permissions.take().unwrap_or_else(Default::default);
        permissions.grant(Permissions::OPERATOR);
        self.permissions = Some(permissions);
    }

    pub fn revoke_operator(&mut self) {
        if let Some(mut permissions) = self.permissions.take() {
            permissions.revoke(Permissions::OPERATOR);
            self.permissions = Some(permissions);
        }
    }

    pub fn grant(&mut self, access: Permissions) {
        let mut permissions = self.permissions.take().unwrap_or_else(Default::default);
        permissions.grant(access);
        self.permissions = Some(permissions);
    }

    pub fn revoke(&mut self, access: Permissions) {
        if let Some(mut permissions) = self.permissions.take() {
            permissions.revoke(access);
            if permissions.has_permissions() {
                self.permissions = Some(permissions);
            } else {
                self.permissions = None;
            }
        }
    }

    pub fn revoke_all(&mut self) {
        self.permissions = None;
    }

    /// returns true if the account has all of the specified permissions
    pub fn contains_permissions(&self, permissions: Permissions) -> bool {
        self.permissions.map_or(false, |perms| {
            perms.contains(permissions) || perms.contains(Permissions::ADMIN)
        })
    }
}

type AccountNearDataKey = u128;

/// Used as key to store [`AccountNearData`] - defined on [`AccountNearDataObject`]
#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AccountNearDataHash(AccountIdHash, AccountNearDataKey);

impl AccountNearDataHash {
    const ACCOUNT_NEAR_DATA_KEY: u128 = 1953035509579102406775126588391115273;

    pub fn account_id_hash(&self) -> AccountIdHash {
        self.0
    }
}

impl From<AccountIdHash> for AccountNearDataHash {
    fn from(hash: AccountIdHash) -> Self {
        Self(hash, Self::ACCOUNT_NEAR_DATA_KEY)
    }
}

impl From<&str> for AccountNearDataHash {
    fn from(account_id: &str) -> Self {
        AccountIdHash(Hash::from(account_id)).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oysterpack_smart_near::domain::ZERO_NEAR;
    use oysterpack_smart_near::YOCTO;
    use oysterpack_smart_near_test::*;

    #[test]
    fn bitflags() {
        let mut account = AccountNearData::new(ZERO_NEAR, 0.into());

        assert!(!account.is_admin());
        account.grant_admin();
        assert!(account.is_admin());
        account.revoke_admin();
        assert!(!account.is_admin());

        assert!(!account.is_operator());
        account.grant_operator();
        assert!(account.is_operator());
        account.revoke_operator();
        assert!(!account.is_operator());
    }

    #[test]
    fn update_near_balance() {
        // Arrange
        let account_id = "bob.near";
        let context = new_context(account_id);
        testing_env!(context);

        let mut account = AccountNearDataObject::new(account_id.into(), ZERO_NEAR);

        // Act - incr near balance
        account.incr_near_balance(YOCTO.into());
        account.save();

        // Assert
        let mut account = AccountNearDataObject::load(account_id).unwrap();
        assert_eq!(account.near_balance(), YOCTO.into());

        // Act - dec near balance
        account.dec_near_balance(YOCTO.into());
        account.save();

        // Assert
        let mut account = AccountNearDataObject::load(account_id).unwrap();
        assert_eq!(account.near_balance(), ZERO_NEAR);

        // Act - set near balance
        account.set_near_balance((2 * YOCTO).into());
        account.save();

        // Assert
        let account = AccountNearDataObject::load(account_id).unwrap();
        assert_eq!(account.near_balance(), (2 * YOCTO).into());
    }

    #[test]
    fn update_storage_usage() {
        // Arrange
        let account_id = "bob.near";
        let context = new_context(account_id);
        testing_env!(context);

        let mut account = AccountNearDataObject::new(account_id.into(), ZERO_NEAR);
        account.save();
        let initial_storage_usage = account.storage_usage;

        // Act - incr near balance
        account.incr_storage_usage(1000.into());
        account.save();

        // Assert
        let mut account = AccountNearDataObject::load(account_id).unwrap();
        assert_eq!(
            account.storage_usage(),
            (initial_storage_usage.value() + 1000).into()
        );

        // Act - dec
        account.decr_storage_usage(1000.into());
        account.save();

        // Assert
        let mut account = AccountNearDataObject::load(account_id).unwrap();
        assert_eq!(account.storage_usage(), initial_storage_usage);

        // Act - incr near balance
        account.incr_storage_usage(2000.into());
        account.save();

        // Assert
        let mut account = AccountNearDataObject::load(account_id).unwrap();
        assert_eq!(account.storage_usage(), 2000.into());

        // Act - update near balance
        account.incr_storage_usage(1000_u64.into());
        account.save();

        // Assert
        let mut account = AccountNearDataObject::load(account_id).unwrap();
        assert_eq!(account.storage_usage(), 3000.into());

        // Act - update near balance
        account.decr_storage_usage(1000_u64.into());
        account.save();

        // Assert
        let mut account = AccountNearDataObject::load(account_id).unwrap();
        assert_eq!(account.storage_usage(), 2000.into());

        // Act - update near balance
        account.incr_storage_usage(0_u64.into());
        account.save();

        // Act - update near balance
        account.decr_storage_usage(0_u64.into());
        account.save();

        // Assert
        let account = AccountNearDataObject::load(account_id).unwrap();
        assert_eq!(account.storage_usage(), 2000.into());
    }
}

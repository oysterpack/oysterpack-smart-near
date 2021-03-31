//! [`FungibleTokenComponent`]
//! - constructor: [`FungibleTokenComponent::new`]
//!   - [`AccountManagementComponent`]
//! - deployment: [`FungibleTokenComponent::deploy`]
//!   - config: [`Config`]
//! - use [`FungibleTokenComponent::register_storage_management_event_handler`]  to register event
//!   handler for [`StorageManagementEvent::PreUnregister`] which integrates with [`AccountManagementComponent`]

use crate::{
    contract::operator::{FungibleTokenOperator, OperatorCommand},
    FungibleToken, FungibleTokenMetadataProvider, Memo, Metadata, ResolveTransferCall, TokenAmount,
    TokenService, TransferCallMessage, ERR_CODE_FT_RESOLVE_TRANSFER, LOG_EVENT_FT_BURN,
    LOG_EVENT_FT_MINT, LOG_EVENT_FT_TRANSFER, LOG_EVENT_FT_TRANSFER_CALL_FAILURE,
    LOG_EVENT_FT_TRANSFER_CALL_PARTIAL_REFUND, LOG_EVENT_FT_TRANSFER_CALL_RECEIVER_DEBIT,
    LOG_EVENT_FT_TRANSFER_CALL_REFUND_NOT_APPLIED, LOG_EVENT_FT_TRANSFER_CALL_SENDER_CREDIT,
};
use oysterpack_smart_account_management::{
    components::account_management::AccountManagementComponent, AccountRepository,
    AccountStorageEvent, StorageManagementEvent, ERR_ACCOUNT_NOT_REGISTERED,
    ERR_CODE_UNREGISTER_FAILURE,
};
use oysterpack_smart_near::eventbus::{self, post};
use oysterpack_smart_near::near_sdk::{
    borsh::{BorshDeserialize, BorshSerialize},
    env,
    json_types::ValidAccountId,
    serde::{Deserialize, Serialize},
    serde_json, AccountId, Promise, PromiseResult,
};
use oysterpack_smart_near::{
    asserts::{
        assert_yocto_near_attached, ERR_CODE_BAD_REQUEST, ERR_INSUFFICIENT_FUNDS, ERR_INVALID,
    },
    lazy_static::lazy_static,
    {component::Deploy, data::Object, to_valid_account_id, Hash, TERA},
};
use oysterpack_smart_near::{
    component::ManagesAccountData,
    domain::{ActionType, ByteLen, Gas, SenderIsReceiver, StorageUsage, TGas, TransactionResource},
};

use std::{fmt::Debug, ops::Deref, ops::DerefMut, sync::Mutex};

pub struct FungibleTokenComponent<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    account_manager: AccountManagementComponent<T>,
}

impl<T> FungibleTokenComponent<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    pub fn new(account_manager: AccountManagementComponent<T>) -> Self {
        Self { account_manager }
    }
}

impl<T> Deploy for FungibleTokenComponent<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    type Config = Config;

    fn deploy(config: Self::Config) {
        MetadataObject::new(METADATA_KEY, config.metadata.clone()).save();
        TokenSupply::new(TOKEN_SUPPLY, config.token_supply).save();
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub struct Config {
    pub metadata: Metadata,
    /// initial token supply
    pub token_supply: u128,
}

impl<T> FungibleToken for FungibleTokenComponent<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    fn ft_transfer(
        &mut self,
        receiver_id: ValidAccountId,
        amount: TokenAmount,
        memo: Option<Memo>,
    ) {
        assert_yocto_near_attached();
        ERR_CODE_BAD_REQUEST.assert(|| *amount > 0, || "transfer amount cannot be zero");

        let sender_id = &env::predecessor_account_id();
        ERR_CODE_BAD_REQUEST.assert(
            || sender_id != receiver_id.as_ref(),
            || "sender and receiver cannot be the same",
        );

        ERR_ACCOUNT_NOT_REGISTERED.assert_with_message(
            || self.account_manager.account_exists(sender_id),
            || "sender account is not registered",
        );
        ERR_ACCOUNT_NOT_REGISTERED.assert_with_message(
            || self.account_manager.account_exists(receiver_id.as_ref()),
            || "receiver account is not registered",
        );

        let sender_balance = self.ft_balance_of(to_valid_account_id(sender_id));
        ERR_INSUFFICIENT_FUNDS.assert(|| *sender_balance >= *amount);

        // transfer the tokens
        AccountFTBalance::set_balance(sender_id, *sender_balance - *amount);
        let receiver_balance = self.ft_balance_of(receiver_id.clone());
        AccountFTBalance::set_balance(receiver_id.as_ref(), *receiver_balance + *amount);

        if let Some(memo) = memo {
            LOG_EVENT_FT_TRANSFER.log(memo);
        }
    }

    fn ft_transfer_call(
        &mut self,
        receiver_id: ValidAccountId,
        amount: TokenAmount,
        memo: Option<Memo>,
        msg: TransferCallMessage,
    ) -> Promise {
        self.ft_transfer(receiver_id.clone(), amount, memo);

        self.create_promise_transfer_receiver_ft_on_transfer(
            &env::predecessor_account_id(),
            receiver_id.as_ref(),
            amount,
            msg,
        )
    }

    fn ft_total_supply(&self) -> TokenAmount {
        TokenSupply::load(&TOKEN_SUPPLY).map_or(0.into(), |amount| (*amount).into())
    }

    fn ft_balance_of(&self, account_id: ValidAccountId) -> TokenAmount {
        AccountFTBalance::balance_of(account_id.as_ref())
    }
}

impl<T> FungibleTokenOperator for FungibleTokenComponent<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    fn ft_operator_command(&mut self, command: OperatorCommand) {
        self.account_manager.assert_operator();
        let mut metadata = MetadataObject::load(&METADATA_KEY).unwrap();
        match command {
            OperatorCommand::SetIcon(icon) => metadata.icon = Some(icon),
            OperatorCommand::ClearIcon => metadata.icon = None,
            OperatorCommand::SetReference(reference, hash) => {
                metadata.reference = Some(reference);
                metadata.reference_hash = Some(hash);
            }
            OperatorCommand::ClearReference => {
                metadata.reference = None;
                metadata.reference_hash = None;
            }
            OperatorCommand::SetTransferCallbackGas(gas) => set_transfer_callback_gas(gas),
        }
        metadata.save();
    }

    fn ft_operator_transfer_callback_gas(&self) -> Gas {
        transfer_callback_gas()
    }
}

const TRANSFER_CALLBACK_GAS_KEY: u128 = 1954437955283579441216159415148835888;
type TransferCallbackGas = Object<u128, Gas>;

fn transfer_callback_gas() -> Gas {
    TransferCallbackGas::load(&TRANSFER_CALLBACK_GAS_KEY)
        .map_or_else(|| (5 * TERA).into(), |gas| *gas)
}

fn set_transfer_callback_gas(gas: TGas) {
    ERR_INVALID.assert(|| gas.value() > 0, || "transfer callback TGas must be > 0");
    let gas = TransferCallbackGas::new(TRANSFER_CALLBACK_GAS_KEY, gas.into());
    gas.save();
}

impl<T> TokenService for FungibleTokenComponent<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    fn ft_mint(&mut self, account_id: &str, amount: TokenAmount) {
        ERR_INVALID.assert(|| *amount > 0, || "mint amount cannot be zero");
        ERR_ACCOUNT_NOT_REGISTERED.assert(|| self.account_manager.account_exists(account_id));

        let mut ft_balance = AccountFTBalance::get(account_id);
        *ft_balance += *amount;
        ft_balance.save(account_id);

        let mut token_supply = token_supply();
        *token_supply += *amount;
        token_supply.save();

        LOG_EVENT_FT_MINT.log(format!("account: {}, amount: {}", account_id, amount));
    }

    fn ft_burn(&mut self, account_id: &str, amount: TokenAmount) {
        ERR_INVALID.assert(|| *amount > 0, || "burn amount cannot be zero");
        ERR_ACCOUNT_NOT_REGISTERED.assert(|| self.account_manager.account_exists(account_id));

        let mut ft_balance = AccountFTBalance::load(account_id).unwrap();
        ERR_INVALID.assert(
            || *ft_balance >= *amount,
            || "account has insufficient funds",
        );
        *ft_balance -= *amount;
        ft_balance.save(account_id);

        burn_tokens(*amount);
        LOG_EVENT_FT_BURN.log(format!("account: {}, amount: {}", account_id, amount));
    }

    fn ft_burn_all(&mut self, account_id: &str) {
        if let Some(mut ft_balance) = AccountFTBalance::load(account_id) {
            let amount = *ft_balance;
            *ft_balance = 0;
            ft_balance.save(account_id);

            burn_tokens(amount);
            LOG_EVENT_FT_BURN.log(format!("account: {}, amount: {}", account_id, amount));
        }
    }
}

impl<T> ManagesAccountData for FungibleTokenComponent<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    fn account_storage_min() -> StorageUsage {
        let account_id = "19544499980228477895959808916967586760";
        let initial_storage = env::storage_usage();
        AccountFTBalance::set_balance(account_id, 1);
        let account_storage_usage = env::storage_usage() - initial_storage;
        AccountFTBalance::set_balance(account_id, 0);
        account_storage_usage.into()
    }
}

impl<T> FungibleTokenComponent<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    fn create_promise_transfer_receiver_ft_on_transfer(
        &self,
        sender_id: &str,
        receiver_id: &str,
        amount: TokenAmount,
        msg: TransferCallMessage,
    ) -> Promise {
        let ft_on_transfer = b"ft_on_transfer".to_vec();
        let ft_on_transfer_args = serde_json::to_vec(&OnTransferArgs {
            sender_id: env::predecessor_account_id(),
            amount,
            msg,
        })
        .expect("");
        let ft_on_transfer_bytes: u64 = (ft_on_transfer.len() + ft_on_transfer_args.len()) as u64;

        let ft_resolve_transfer_call = b"ft_resolve_transfer_call".to_vec();
        let ft_resolve_transfer_call_args = serde_json::to_vec(&ResolveTransferArgs {
            sender_id: sender_id.to_string(),
            receiver_id: receiver_id.to_string(),
            amount,
        })
        .expect("");
        let ft_resolve_transfer_call_bytes: u64 =
            (ft_resolve_transfer_call.len() + ft_resolve_transfer_call_args.len()) as u64;

        // compute how much gas is needed to complete this call and the resolve transfer callback
        // and then give the rest of the gas to the transfer receiver call
        let ft_on_transfer_receipt_action_cost = {
            let action_receipt = TransactionResource::ActionReceipt(SenderIsReceiver(false));
            let func_call_action = TransactionResource::Action(ActionType::FunctionCall(
                SenderIsReceiver(false),
                ByteLen(ft_on_transfer_bytes),
            ));
            Gas::compute(vec![(action_receipt, 1), (func_call_action, 1)])
        };
        let ft_resolve_transfer_call_receipt_action_cost = {
            let action_receipt = TransactionResource::ActionReceipt(SenderIsReceiver(true));
            let func_call_action = TransactionResource::Action(ActionType::FunctionCall(
                SenderIsReceiver(true),
                ByteLen(ft_resolve_transfer_call_bytes),
            ));
            // byte len for transfer amount is set to 100 because even though the underlying type
            // is u128, it is marshalled as a string. Thus the number of bytes will vary depending on
            // the amount value - we'll pick 100 to be conservative.
            let data_receipt =
                TransactionResource::DataReceipt(SenderIsReceiver(false), ByteLen(100));
            Gas::compute(vec![
                (action_receipt, 1),
                (func_call_action, 1),
                (data_receipt, 1),
            ])
        };
        let ft_on_transfer_gas = env::prepaid_gas()
            - env::used_gas()
            - transfer_callback_gas().value()
            - ft_on_transfer_receipt_action_cost.value()
            - ft_resolve_transfer_call_receipt_action_cost.value()
            - TERA; // to complete this call;

        // create the function call chain
        {
            let ft_transfer_call = Promise::new(receiver_id.to_string()).function_call(
                ft_on_transfer,
                ft_on_transfer_args,
                0,
                ft_on_transfer_gas,
            );
            let ft_resolve_transfer_call = Promise::new(env::current_account_id()).function_call(
                ft_resolve_transfer_call,
                ft_resolve_transfer_call_args,
                0,
                transfer_callback_gas().value(),
            );
            ft_transfer_call.then(ft_resolve_transfer_call)
        }
    }

    /// Used to register an event handler hook to handle account unregistrations
    ///
    /// can be safely called multiple times and will only register the event handler once
    pub fn register_storage_management_event_handler() {
        let mut registered = STORAGE_MANAGEMENT_EVENT_HANDLER_REGISTERED.lock().unwrap();
        if !*registered {
            eventbus::register(Self::on_unregister_account);
            *registered = true;
        }
    }

    /// EventHandler must be registered to handle [`StorageManagementEvent::PreUnregister`] events
    ///
    /// When an account is forced unregistered, any tokens it owned will be burned, which reduces the total
    /// token supply.
    fn on_unregister_account(event: &StorageManagementEvent) {
        if let StorageManagementEvent::PreUnregister { force, .. } = event {
            let account_id = &env::predecessor_account_id();
            if let Some(ft_balance) = AccountFTBalance::load(&account_id) {
                ERR_CODE_UNREGISTER_FAILURE
                    .assert(|| *force, || "account has non-zero token balance");
                let amount = *ft_balance;
                AccountFTBalance::set_balance(account_id, 0);
                burn_tokens(amount);
                LOG_EVENT_FT_BURN.log(format!(
                    "account forced unregistered with token balance: account: account: {}, amount: {}",
                    account_id, amount
                ));
            }
        }
    }
}

lazy_static! {
    static ref STORAGE_MANAGEMENT_EVENT_HANDLER_REGISTERED: Mutex<bool> = Mutex::new(false);
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub struct OnTransferArgs {
    sender_id: AccountId,
    amount: TokenAmount,
    msg: TransferCallMessage,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(crate = "oysterpack_smart_near::near_sdk::serde")]
pub struct ResolveTransferArgs {
    sender_id: AccountId,
    receiver_id: AccountId,
    amount: TokenAmount,
}

const TOKEN_SUPPLY: u128 = 1953830723745925743018307013370321490;
type TokenSupply = Object<u128, u128>;
fn token_supply() -> TokenSupply {
    TokenSupply::load(&TOKEN_SUPPLY).unwrap()
}

fn burn_tokens(amount: u128) {
    let mut supply = token_supply();
    *supply -= amount;
    supply.save();
}

const METADATA_KEY: u128 = 1953827270399390220126384465824835887;
type MetadataObject = Object<u128, Metadata>;

impl<T> FungibleTokenMetadataProvider for FungibleTokenComponent<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    fn ft_metadata(&self) -> Metadata {
        MetadataObject::load(&METADATA_KEY).unwrap().deref().clone()
    }
}

impl<T> ResolveTransferCall for FungibleTokenComponent<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    fn ft_resolve_transfer_call(
        &mut self,
        sender_id: ValidAccountId,
        receiver_id: ValidAccountId,
        amount: TokenAmount,
    ) -> TokenAmount {
        // Get the refund amount from the `ft_on_transfer` call result.
        let refund_amount = match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Successful(value) => {
                match serde_json::from_slice::<TokenAmount>(&value) {
                    Ok(refund_amount) => {
                        if refund_amount > amount {
                            ERR_CODE_FT_RESOLVE_TRANSFER
                            .error("refund amount was greater than the transfer amount - full transfer amount will be refunded")
                            .log();
                            amount
                        } else {
                            refund_amount
                        }
                    }
                    Err(_) => {
                        ERR_CODE_FT_RESOLVE_TRANSFER
                            .error(
                                "failed to deserialize refund amount - no refund will be applied",
                            )
                            .log();
                        0.into()
                    }
                }
            }
            PromiseResult::Failed => {
                LOG_EVENT_FT_TRANSFER_CALL_FAILURE.log("full transfer amount will be refunded");
                amount
            }
        };

        if *refund_amount == 0 {
            return refund_amount;
        }

        // try to refund the refund amount from the receiver back to the sender
        let refund_amount = if let Some(mut receiver_account_balance) =
            AccountFTBalance::load(receiver_id.as_ref())
        {
            let refund_amount = if *receiver_account_balance < *refund_amount {
                LOG_EVENT_FT_TRANSFER_CALL_PARTIAL_REFUND.log(
                    "partial refund will be applied because receiver account has insufficient fund",
                );
                *receiver_account_balance
            } else {
                *refund_amount
            };
            *receiver_account_balance -= refund_amount;
            receiver_account_balance.save(receiver_id.as_ref());

            LOG_EVENT_FT_TRANSFER_CALL_RECEIVER_DEBIT.log(refund_amount);

            match AccountFTBalance::load(sender_id.as_ref()) {
                Some(mut sender_account_balance) => {
                    *sender_account_balance += refund_amount;
                    sender_account_balance.save(sender_id.as_ref());
                    LOG_EVENT_FT_TRANSFER_CALL_SENDER_CREDIT.log(refund_amount);
                }
                None => {
                    // - if balance is zero, then storage was cleaned up
                    // - the sender account most likely still exists, but might have been unregistered
                    //   while the transfer call workflow was in flight
                    if self.account_manager.account_exists(sender_id.as_ref()) {
                        AccountFTBalance::set_balance(sender_id.as_ref(), refund_amount);
                        LOG_EVENT_FT_TRANSFER_CALL_SENDER_CREDIT.log(refund_amount);
                    } else {
                        burn_tokens(refund_amount);
                        LOG_EVENT_FT_BURN.log(format!(
                            "sender account is not registered: {}",
                            refund_amount
                        ));
                    }
                }
            }
            refund_amount.into()
        } else {
            if self.account_manager.account_exists(receiver_id.as_ref()) {
                // this could happen if:
                // - the receiver account transferred out the tokens while this transfer call workflow
                //   was in flight
                // - there is a bug in the receiver contract
                // - the receiver contract is acting maliciously
                LOG_EVENT_FT_TRANSFER_CALL_REFUND_NOT_APPLIED
                    .log("receiver account has zero balance");
            } else {
                // if the receiver account is no longer registered then the refund amount of tokens
                // will be handled when the account unregistered
                // - the account will not be allowed to unregister with a non-zero token balance,
                //   otherwise if forced, the tokens will be burned
                LOG_EVENT_FT_TRANSFER_CALL_REFUND_NOT_APPLIED
                    .log("receiver account not registered");
            }
            0.into()
        };

        refund_amount
    }
}

const FT_ACCOUNT_KEY: u128 = 1953845438124731969041175284518648060;
type AccountFTBalanceObject = Object<Hash, u128>;
struct AccountFTBalance(AccountFTBalanceObject);

impl AccountFTBalance {
    fn ft_account_id_hash(account_id: &str) -> Hash {
        Hash::from((account_id, FT_ACCOUNT_KEY))
    }

    fn load(account_id: &str) -> Option<AccountFTBalance> {
        let account_hash_id = AccountFTBalance::ft_account_id_hash(account_id);
        AccountFTBalanceObject::load(&account_hash_id).map(Self)
    }

    fn get(account_id: &str) -> AccountFTBalance {
        Self::load(account_id).unwrap_or_else(|| {
            Self(AccountFTBalanceObject::new(
                Self::ft_account_id_hash(account_id),
                0,
            ))
        })
    }

    fn balance_of(account_id: &str) -> TokenAmount {
        Self::load(account_id).map_or(0.into(), |balance| (*balance).into())
    }

    fn save(&self, account_id: &str) {
        if *self.0 == 0 {
            let initial_storage_usage = env::storage_usage();
            AccountFTBalanceObject::delete_by_key(self.0.key());
            let storage_usage_change = initial_storage_usage - env::storage_usage();
            if storage_usage_change > 0 {
                post(&AccountStorageEvent::StorageUsageChanged(
                    account_id.into(),
                    (storage_usage_change as i64 * -1).into(),
                ));
            }
        } else {
            let initial_storage_usage = env::storage_usage();
            self.0.save();
            let storage_usage_change = env::storage_usage() - initial_storage_usage;
            if storage_usage_change > 0 {
                post(&AccountStorageEvent::StorageUsageChanged(
                    account_id.into(),
                    storage_usage_change.into(),
                ));
            }
        }
    }

    /// tracks storage
    /// - if balance is set to zero, then the balance record will be deleted from storage
    fn set_balance(account_id: &str, balance: u128) {
        let account_hash_id = Self::ft_account_id_hash(account_id);
        match AccountFTBalanceObject::load(&account_hash_id) {
            None => {
                if balance == 0 {
                    return;
                }
                let initial_storage_usage = env::storage_usage();
                AccountFTBalanceObject::new(account_hash_id, balance).save();
                let storage_usage_change = env::storage_usage() - initial_storage_usage;
                post(&AccountStorageEvent::StorageUsageChanged(
                    account_id.into(),
                    storage_usage_change.into(),
                ));
            }
            Some(mut account_balance) => {
                if balance == 0 {
                    let initial_storage_usage = env::storage_usage();
                    account_balance.delete();
                    let storage_usage_change = initial_storage_usage - env::storage_usage();
                    post(&AccountStorageEvent::StorageUsageChanged(
                        account_id.into(),
                        (storage_usage_change as i64 * -1).into(),
                    ));
                } else {
                    *account_balance = balance;
                    account_balance.save();
                }
            }
        }
    }
}

impl Deref for AccountFTBalance {
    type Target = u128;

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

impl DerefMut for AccountFTBalance {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.deref_mut()
    }
}

#[cfg(test)]
mod tests_fungible_token {
    use super::*;
    use crate::FungibleToken;
    use crate::*;
    use near_sdk::{test_utils, VMContext};
    use oysterpack_smart_account_management::components::account_management::{
        AccountManagementComponentConfig, ContractPermissions,
    };
    use oysterpack_smart_account_management::StorageManagement;
    use oysterpack_smart_near::YOCTO;
    use oysterpack_smart_near_test::*;

    type AccountDataType = ();
    type AccountManager = AccountManagementComponent<AccountDataType>;
    type STAKE = FungibleTokenComponent<AccountDataType>;

    const ADMIN: &str = "admin";

    fn deploy_comps() {
        AccountManager::deploy(AccountManagementComponentConfig::new(to_valid_account_id(
            ADMIN,
        )));

        STAKE::deploy(Config {
            metadata: Metadata {
                spec: FT_METADATA_SPEC.into(),
                name: "STAKE".into(),
                symbol: "STAKE".into(),
                icon: None,
                reference: None,
                reference_hash: None,
                decimals: 24,
            },
            token_supply: YOCTO,
        });
    }

    #[test]
    fn basic_workflow() {
        // Arrange
        let sender = "sender";
        let receiver = "receiver";
        let mut ctx = new_context(sender);
        testing_env!(ctx.clone());

        deploy_comps();

        let mut account_manager = AccountManager::new(&ContractPermissions::default());

        // register accounts
        {
            ctx.attached_deposit = YOCTO;
            testing_env!(ctx.clone());
            account_manager.storage_deposit(None, None);

            ctx.attached_deposit = YOCTO;
            ctx.predecessor_account_id = receiver.to_string();
            testing_env!(ctx.clone());
            account_manager.storage_deposit(None, None);
        }

        let mut stake = STAKE::new(account_manager);
        // mint some new stake for the sender
        AccountFTBalance::set_balance(sender, 100);

        // Act
        ctx.predecessor_account_id = sender.to_string();
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        stake.ft_transfer(to_valid_account_id(receiver), 50.into(), None);
        stake.ft_transfer(
            to_valid_account_id(receiver),
            50.into(),
            Some("memo".into()),
        );
        assert_eq!(*stake.ft_balance_of(to_valid_account_id(sender)), 0);
        assert_eq!(*stake.ft_balance_of(to_valid_account_id(receiver)), 100);

        ctx.predecessor_account_id = receiver.to_string();
        ctx.attached_deposit = 1;
        testing_env!(ctx.clone());
        stake.ft_transfer_call(to_valid_account_id(sender), 50.into(), None, "msg".into());

        let receipts = deserialize_receipts();
        assert_eq!(receipts.len(), 2);

        ctx.predecessor_account_id = receiver.to_string();
        ctx.attached_deposit = 0;
        testing_env_with_promise_results(
            ctx.clone(),
            vec![PromiseResult::Successful(
                serde_json::to_vec(&TokenAmount::from(10)).unwrap(),
            )],
        );
        stake.ft_resolve_transfer_call(
            to_valid_account_id(sender),
            to_valid_account_id(receiver),
            50.into(),
        );
        let logs = test_utils::get_logs();
        println!("logs: {:#?}", logs);

        assert_eq!(stake.ft_balance_of(to_valid_account_id(sender)), 60.into());
        assert_eq!(
            stake.ft_balance_of(to_valid_account_id(receiver)),
            40.into()
        );
    }

    const SENDER: &str = "sender";
    const RECEIVER: &str = "receiver";

    /// - if the balances are Some then register them and mint tokens for them
    fn run_test<F>(
        sender_balance: Option<TokenAmount>,
        receiver_balance: Option<TokenAmount>,
        test: F,
    ) where
        F: FnOnce(VMContext, STAKE),
    {
        // Arrange
        let ctx = new_context(SENDER);
        testing_env!(ctx.clone());

        deploy_comps();

        let account_manager = AccountManager::new(&ContractPermissions::default());

        let mut stake = STAKE::new(account_manager);

        // register accounts
        {
            let mut account_manager = AccountManager::new(&ContractPermissions::default());

            if let Some(balance) = sender_balance {
                let mut ctx = ctx.clone();
                ctx.predecessor_account_id = SENDER.to_string();
                ctx.attached_deposit = account_manager.storage_balance_bounds().min.value();
                testing_env!(ctx.clone());
                account_manager.storage_deposit(None, Some(true));

                if *balance > 0 {
                    stake.ft_mint(SENDER, balance);
                }
            }

            if let Some(balance) = receiver_balance {
                let mut ctx = ctx.clone();
                ctx.predecessor_account_id = RECEIVER.to_string();
                ctx.attached_deposit = account_manager.storage_balance_bounds().min.value();
                testing_env!(ctx.clone());
                account_manager.storage_deposit(None, Some(true));

                if *balance > 0 {
                    stake.ft_mint(RECEIVER, balance);
                }
            }
        }

        test(ctx, stake);
    }

    #[cfg(test)]
    mod test_ft_transfer {
        use super::*;

        #[test]
        fn valid_transfer_with_no_memo() {
            run_test(Some(1000.into()), Some(0.into()), |mut ctx, mut stake| {
                ctx.predecessor_account_id = SENDER.to_string();
                ctx.attached_deposit = 1;
                testing_env!(ctx.clone());
                stake.ft_transfer(to_valid_account_id(RECEIVER), 400.into(), None);

                assert_eq!(stake.ft_balance_of(to_valid_account_id(SENDER)), 600.into());
                assert_eq!(
                    stake.ft_balance_of(to_valid_account_id(RECEIVER)),
                    400.into()
                );

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(logs.len(), 1);
                assert_eq!(
                    &logs[0],
                    "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(88)"
                );
            });
        }

        #[test]
        fn valid_transfer_full_amount() {
            run_test(Some(1000.into()), Some(0.into()), |mut ctx, mut stake| {
                ctx.predecessor_account_id = SENDER.to_string();
                ctx.attached_deposit = 1;
                testing_env!(ctx.clone());
                stake.ft_transfer(to_valid_account_id(RECEIVER), 1000.into(), None);

                assert_eq!(stake.ft_balance_of(to_valid_account_id(SENDER)), 0.into());
                assert_eq!(
                    stake.ft_balance_of(to_valid_account_id(RECEIVER)),
                    1000.into()
                );

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(
                    &logs[0],
                    "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(-88)" // caused by sender debit zeroing FT balance
                );
                assert_eq!(
                    &logs[1],
                    "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(88)" // caused by receiver credit
                );
            });
        }

        #[test]
        fn valid_transfer_with_memo() {
            run_test(Some(1000.into()), Some(0.into()), |mut ctx, mut stake| {
                ctx.predecessor_account_id = SENDER.to_string();
                ctx.attached_deposit = 1;
                testing_env!(ctx.clone());
                stake.ft_transfer(
                    to_valid_account_id(RECEIVER),
                    400.into(),
                    Some(Memo("memo".to_string())),
                );

                assert_eq!(stake.ft_balance_of(to_valid_account_id(SENDER)), 600.into());
                assert_eq!(
                    stake.ft_balance_of(to_valid_account_id(RECEIVER)),
                    400.into()
                );

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(logs.len(), 2);
                assert_eq!(
                    &logs[0],
                    "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(88)"
                );
                assert_eq!(&logs[1], "[INFO] [FT_TRANSFER] memo");
            });
        }

        #[test]
        #[should_panic(
            expected = "[ERR] [ACCOUNT_NOT_REGISTERED] sender account is not registered"
        )]
        fn sender_not_registered() {
            run_test(None, Some(0.into()), |mut ctx, mut stake| {
                ctx.predecessor_account_id = SENDER.to_string();
                ctx.attached_deposit = 1;
                testing_env!(ctx.clone());
                stake.ft_transfer(
                    to_valid_account_id(RECEIVER),
                    400.into(),
                    Some(Memo("memo".to_string())),
                );
            });
        }

        #[test]
        #[should_panic(
            expected = "[ERR] [ACCOUNT_NOT_REGISTERED] receiver account is not registered"
        )]
        fn receiver_not_registered() {
            run_test(Some(1000.into()), None, |mut ctx, mut stake| {
                ctx.predecessor_account_id = SENDER.to_string();
                ctx.attached_deposit = 1;
                testing_env!(ctx.clone());
                stake.ft_transfer(
                    to_valid_account_id(RECEIVER),
                    400.into(),
                    Some(Memo("memo".to_string())),
                );
            });
        }

        #[test]
        #[should_panic(expected = "[ERR] [BAD_REQUEST] sender and receiver cannot be the same")]
        fn sender_is_receiver() {
            run_test(Some(1000.into()), Some(0.into()), |mut ctx, mut stake| {
                ctx.predecessor_account_id = SENDER.to_string();
                ctx.attached_deposit = 1;
                testing_env!(ctx.clone());
                stake.ft_transfer(
                    to_valid_account_id(SENDER),
                    400.into(),
                    Some(Memo("memo".to_string())),
                );
            });
        }

        #[test]
        #[should_panic(expected = "[ERR] [YOCTONEAR_DEPOSIT_REQUIRED]")]
        fn yocto_not_attached() {
            run_test(Some(1000.into()), Some(0.into()), |mut ctx, mut stake| {
                ctx.predecessor_account_id = SENDER.to_string();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                stake.ft_transfer(
                    to_valid_account_id(RECEIVER),
                    400.into(),
                    Some(Memo("memo".to_string())),
                );
            });
        }

        #[test]
        #[should_panic(expected = "[ERR] [BAD_REQUEST] transfer amount cannot be zero")]
        fn zero_transfer_amount() {
            run_test(Some(1000.into()), Some(0.into()), |mut ctx, mut stake| {
                ctx.predecessor_account_id = SENDER.to_string();
                ctx.attached_deposit = 1;
                testing_env!(ctx.clone());
                stake.ft_transfer(
                    to_valid_account_id(RECEIVER),
                    0.into(),
                    Some(Memo("memo".to_string())),
                );
            });
        }

        #[test]
        #[should_panic(expected = "[ERR] [INSUFFICIENT_FUNDS]")]
        fn insufficient_funds() {
            run_test(Some(1000.into()), Some(0.into()), |mut ctx, mut stake| {
                ctx.predecessor_account_id = SENDER.to_string();
                ctx.attached_deposit = 1;
                testing_env!(ctx.clone());
                stake.ft_transfer(
                    to_valid_account_id(RECEIVER),
                    1001.into(),
                    Some(Memo("memo".to_string())),
                );
            });
        }
    }

    #[cfg(test)]
    mod test_ft_transfer_call {
        use super::*;

        #[test]
        fn valid_transfer_with_no_memo() {
            run_test(Some(1000.into()), Some(0.into()), |mut ctx, mut stake| {
                ctx.predecessor_account_id = SENDER.to_string();
                ctx.attached_deposit = 1;
                testing_env!(ctx.clone());
                stake.ft_transfer_call(
                    to_valid_account_id(RECEIVER),
                    400.into(),
                    None,
                    TransferCallMessage("msg".to_string()),
                );

                assert_eq!(stake.ft_balance_of(to_valid_account_id(SENDER)), 600.into());
                assert_eq!(
                    stake.ft_balance_of(to_valid_account_id(RECEIVER)),
                    400.into()
                );

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(logs.len(), 1);
                assert_eq!(
                    &logs[0],
                    "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(88)"
                );

                let receipts = deserialize_receipts();
                assert_eq!(receipts.len(), 2);
                {
                    let receipt = &receipts[0];
                    assert_eq!(receipt.receiver_id, RECEIVER);
                    assert_eq!(receipt.actions.len(), 1);
                    let action = &receipt.actions[0];
                    match action {
                        Action::FunctionCall(action) => {
                            assert_eq!(action.method_name, "ft_on_transfer");
                            assert_eq!(action.deposit, 0);
                            let args: OnTransferArgs =
                                serde_json::from_str(action.args.as_str()).unwrap();
                            assert_eq!(args.sender_id, SENDER);
                            assert_eq!(args.amount, 400.into());
                            assert_eq!(args.msg, TransferCallMessage("msg".to_string()));
                        }
                        _ => panic!("expected FunctionCall action"),
                    }
                }

                {
                    let receipt = &receipts[1];
                    assert_eq!(receipt.receiver_id, env::current_account_id());
                    assert_eq!(receipt.actions.len(), 1);
                    let action = &receipt.actions[0];
                    match action {
                        Action::FunctionCall(action) => {
                            assert_eq!(action.method_name, "ft_resolve_transfer_call");
                            assert_eq!(action.deposit, 0);
                            let args: ResolveTransferArgs =
                                serde_json::from_str(action.args.as_str()).unwrap();
                            assert_eq!(args.sender_id, SENDER);
                            assert_eq!(args.amount, 400.into());
                            assert_eq!(args.receiver_id, RECEIVER);
                            assert_eq!(action.gas, transfer_callback_gas().value());
                        }
                        _ => panic!("expected FunctionCall action"),
                    }
                }
            });
        }

        #[test]
        fn valid_transfer_full_amount_with_no_memo() {
            run_test(Some(1000.into()), Some(0.into()), |mut ctx, mut stake| {
                ctx.predecessor_account_id = SENDER.to_string();
                ctx.attached_deposit = 1;
                testing_env!(ctx.clone());
                stake.ft_transfer_call(
                    to_valid_account_id(RECEIVER),
                    1000.into(),
                    None,
                    TransferCallMessage("msg".to_string()),
                );

                assert_eq!(stake.ft_balance_of(to_valid_account_id(SENDER)), 0.into());
                assert_eq!(
                    stake.ft_balance_of(to_valid_account_id(RECEIVER)),
                    1000.into()
                );

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(logs.len(), 2);
                assert_eq!(
                    &logs[0],
                    "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(-88)"
                );
                assert_eq!(
                    &logs[1],
                    "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(88)"
                );

                let receipts = deserialize_receipts();
                assert_eq!(receipts.len(), 2);
                {
                    let receipt = &receipts[0];
                    assert_eq!(receipt.receiver_id, RECEIVER);
                    assert_eq!(receipt.actions.len(), 1);
                    let action = &receipt.actions[0];
                    match action {
                        Action::FunctionCall(action) => {
                            assert_eq!(action.method_name, "ft_on_transfer");
                            assert_eq!(action.deposit, 0);
                            let args: OnTransferArgs =
                                serde_json::from_str(action.args.as_str()).unwrap();
                            assert_eq!(args.sender_id, SENDER);
                            assert_eq!(args.amount, 1000.into());
                            assert_eq!(args.msg, TransferCallMessage("msg".to_string()));
                        }
                        _ => panic!("expected FunctionCall action"),
                    }
                }

                {
                    let receipt = &receipts[1];
                    assert_eq!(receipt.receiver_id, env::current_account_id());
                    assert_eq!(receipt.actions.len(), 1);
                    let action = &receipt.actions[0];
                    match action {
                        Action::FunctionCall(action) => {
                            assert_eq!(action.method_name, "ft_resolve_transfer_call");
                            assert_eq!(action.deposit, 0);
                            let args: ResolveTransferArgs =
                                serde_json::from_str(action.args.as_str()).unwrap();
                            assert_eq!(args.sender_id, SENDER);
                            assert_eq!(args.amount, 1000.into());
                            assert_eq!(args.receiver_id, RECEIVER);
                            assert_eq!(action.gas, transfer_callback_gas().value());
                        }
                        _ => panic!("expected FunctionCall action"),
                    }
                }
            });
        }

        #[test]
        fn valid_transfer_with_memo() {
            run_test(Some(1000.into()), Some(0.into()), |mut ctx, mut stake| {
                ctx.predecessor_account_id = SENDER.to_string();
                ctx.attached_deposit = 1;
                testing_env!(ctx.clone());
                stake.ft_transfer_call(
                    to_valid_account_id(RECEIVER),
                    400.into(),
                    Some(Memo("memo".to_string())),
                    TransferCallMessage("msg".to_string()),
                );

                assert_eq!(stake.ft_balance_of(to_valid_account_id(SENDER)), 600.into());
                assert_eq!(
                    stake.ft_balance_of(to_valid_account_id(RECEIVER)),
                    400.into()
                );

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(logs.len(), 2);
                assert_eq!(
                    &logs[0],
                    "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(88)"
                );
                assert_eq!(&logs[1], "[INFO] [FT_TRANSFER] memo");

                let receipts = deserialize_receipts();
                assert_eq!(receipts.len(), 2);
                {
                    let receipt = &receipts[0];
                    assert_eq!(receipt.receiver_id, RECEIVER);
                    assert_eq!(receipt.actions.len(), 1);
                    let action = &receipt.actions[0];
                    match action {
                        Action::FunctionCall(action) => {
                            assert_eq!(action.method_name, "ft_on_transfer");
                            assert_eq!(action.deposit, 0);
                            let args: OnTransferArgs =
                                serde_json::from_str(action.args.as_str()).unwrap();
                            assert_eq!(args.sender_id, SENDER);
                            assert_eq!(args.amount, 400.into());
                            assert_eq!(args.msg, TransferCallMessage("msg".to_string()));
                        }
                        _ => panic!("expected FunctionCall action"),
                    }
                }

                {
                    let receipt = &receipts[1];
                    assert_eq!(receipt.receiver_id, env::current_account_id());
                    assert_eq!(receipt.actions.len(), 1);
                    let action = &receipt.actions[0];
                    match action {
                        Action::FunctionCall(action) => {
                            assert_eq!(action.method_name, "ft_resolve_transfer_call");
                            assert_eq!(action.deposit, 0);
                            let args: ResolveTransferArgs =
                                serde_json::from_str(action.args.as_str()).unwrap();
                            assert_eq!(args.sender_id, SENDER);
                            assert_eq!(args.amount, 400.into());
                            assert_eq!(args.receiver_id, RECEIVER);
                            assert_eq!(action.gas, transfer_callback_gas().value());
                        }
                        _ => panic!("expected FunctionCall action"),
                    }
                }
            });
        }

        #[test]
        #[should_panic(
            expected = "[ERR] [ACCOUNT_NOT_REGISTERED] sender account is not registered"
        )]
        fn sender_not_registered() {
            run_test(None, Some(0.into()), |mut ctx, mut stake| {
                ctx.predecessor_account_id = SENDER.to_string();
                ctx.attached_deposit = 1;
                testing_env!(ctx.clone());
                stake.ft_transfer_call(
                    to_valid_account_id(RECEIVER),
                    400.into(),
                    Some(Memo("memo".to_string())),
                    TransferCallMessage("msg".to_string()),
                );
            });
        }

        #[test]
        #[should_panic(
            expected = "[ERR] [ACCOUNT_NOT_REGISTERED] receiver account is not registered"
        )]
        fn receiver_not_registered() {
            run_test(Some(1000.into()), None, |mut ctx, mut stake| {
                ctx.predecessor_account_id = SENDER.to_string();
                ctx.attached_deposit = 1;
                testing_env!(ctx.clone());
                stake.ft_transfer_call(
                    to_valid_account_id(RECEIVER),
                    400.into(),
                    Some(Memo("memo".to_string())),
                    TransferCallMessage("msg".to_string()),
                );
            });
        }

        #[test]
        #[should_panic(expected = "[ERR] [BAD_REQUEST] sender and receiver cannot be the same")]
        fn sender_is_receiver() {
            run_test(Some(1000.into()), Some(0.into()), |mut ctx, mut stake| {
                ctx.predecessor_account_id = SENDER.to_string();
                ctx.attached_deposit = 1;
                testing_env!(ctx.clone());
                stake.ft_transfer_call(
                    to_valid_account_id(SENDER),
                    400.into(),
                    Some(Memo("memo".to_string())),
                    TransferCallMessage("msg".to_string()),
                );
            });
        }

        #[test]
        #[should_panic(expected = "[ERR] [YOCTONEAR_DEPOSIT_REQUIRED]")]
        fn yocto_not_attached() {
            run_test(Some(1000.into()), Some(0.into()), |mut ctx, mut stake| {
                ctx.predecessor_account_id = SENDER.to_string();
                ctx.attached_deposit = 0;
                testing_env!(ctx.clone());
                stake.ft_transfer_call(
                    to_valid_account_id(RECEIVER),
                    400.into(),
                    Some(Memo("memo".to_string())),
                    TransferCallMessage("msg".to_string()),
                );
            });
        }

        #[test]
        #[should_panic(expected = "[ERR] [BAD_REQUEST] transfer amount cannot be zero")]
        fn zero_transfer_amount() {
            run_test(Some(1000.into()), Some(0.into()), |mut ctx, mut stake| {
                ctx.predecessor_account_id = SENDER.to_string();
                ctx.attached_deposit = 1;
                testing_env!(ctx.clone());
                stake.ft_transfer_call(
                    to_valid_account_id(RECEIVER),
                    0.into(),
                    Some(Memo("memo".to_string())),
                    TransferCallMessage("msg".to_string()),
                );
            });
        }

        #[test]
        #[should_panic(expected = "[ERR] [INSUFFICIENT_FUNDS]")]
        fn insufficient_funds() {
            run_test(Some(1000.into()), Some(0.into()), |mut ctx, mut stake| {
                ctx.predecessor_account_id = SENDER.to_string();
                ctx.attached_deposit = 1;
                testing_env!(ctx.clone());
                stake.ft_transfer_call(
                    to_valid_account_id(RECEIVER),
                    1001.into(),
                    Some(Memo("memo".to_string())),
                    TransferCallMessage("msg".to_string()),
                );
            });
        }
    }

    #[cfg(test)]
    mod test_resolve_transfer_call {
        use super::*;

        #[test]
        fn zero_refund() {
            run_test(None, Some(1000.into()), |mut ctx, mut stake| {
                ctx.predecessor_account_id = ctx.current_account_id.clone();
                let refund_amount = TokenAmount(0.into());
                let refund_amount_bytes = serde_json::to_vec(&refund_amount).unwrap();
                testing_env_with_promise_results(
                    ctx,
                    vec![PromiseResult::Successful(refund_amount_bytes)],
                );
                stake.ft_resolve_transfer_call(
                    to_valid_account_id(SENDER),
                    to_valid_account_id(RECEIVER),
                    TokenAmount(500.into()),
                );

                let logs = test_utils::get_logs();
                assert!(logs.is_empty());
            });
        }

        #[test]
        fn full_refund_sender_with_zero_balance() {
            run_test(Some(0.into()), Some(1000.into()), |mut ctx, mut stake| {
                ctx.predecessor_account_id = ctx.current_account_id.clone();
                let refund_amount = TokenAmount(500.into());
                let refund_amount_bytes = serde_json::to_vec(&refund_amount).unwrap();
                testing_env_with_promise_results(
                    ctx,
                    vec![PromiseResult::Successful(refund_amount_bytes)],
                );
                let actual_refund_amount = stake.ft_resolve_transfer_call(
                    to_valid_account_id(SENDER),
                    to_valid_account_id(RECEIVER),
                    refund_amount,
                );
                assert_eq!(actual_refund_amount, refund_amount);

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(logs.len(), 3);
                assert_eq!(&logs[0], "[INFO] [FT_TRANSFER_CALL_RECEIVER_DEBIT] 500");
                assert_eq!(
                    &logs[1],
                    "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(88)"
                );
                assert_eq!(&logs[2], "[INFO] [FT_TRANSFER_CALL_SENDER_CREDIT] 500");

                assert_eq!(stake.ft_balance_of(to_valid_account_id(SENDER)), 500.into());
                assert_eq!(
                    stake.ft_balance_of(to_valid_account_id(RECEIVER)),
                    500.into()
                );
            });
        }

        #[test]
        fn full_refund_sender_with_non_zero_balance() {
            run_test(Some(100.into()), Some(1000.into()), |mut ctx, mut stake| {
                ctx.predecessor_account_id = ctx.current_account_id.clone();
                let refund_amount = TokenAmount(500.into());
                let refund_amount_bytes = serde_json::to_vec(&refund_amount).unwrap();
                testing_env_with_promise_results(
                    ctx,
                    vec![PromiseResult::Successful(refund_amount_bytes)],
                );
                let actual_refund_amount = stake.ft_resolve_transfer_call(
                    to_valid_account_id(SENDER),
                    to_valid_account_id(RECEIVER),
                    refund_amount,
                );
                assert_eq!(actual_refund_amount, refund_amount);

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(logs.len(), 2);
                assert_eq!(&logs[0], "[INFO] [FT_TRANSFER_CALL_RECEIVER_DEBIT] 500");
                assert_eq!(&logs[1], "[INFO] [FT_TRANSFER_CALL_SENDER_CREDIT] 500");

                assert_eq!(stake.ft_balance_of(to_valid_account_id(SENDER)), 600.into());
                assert_eq!(
                    stake.ft_balance_of(to_valid_account_id(RECEIVER)),
                    500.into()
                );
            });
        }

        #[test]
        fn partial_refund_with_sender_zero_balance() {
            run_test(Some(0.into()), Some(1000.into()), |mut ctx, mut stake| {
                ctx.predecessor_account_id = ctx.current_account_id.clone();
                let transfer_amount = TokenAmount(500.into());
                let refund_amount = TokenAmount(100.into());
                let refund_amount_bytes = serde_json::to_vec(&refund_amount).unwrap();
                testing_env_with_promise_results(
                    ctx,
                    vec![PromiseResult::Successful(refund_amount_bytes)],
                );
                let actual_refund_amount = stake.ft_resolve_transfer_call(
                    to_valid_account_id(SENDER),
                    to_valid_account_id(RECEIVER),
                    transfer_amount,
                );
                assert_eq!(actual_refund_amount, refund_amount);

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(logs.len(), 3);
                assert_eq!(&logs[0], "[INFO] [FT_TRANSFER_CALL_RECEIVER_DEBIT] 100");
                assert_eq!(
                    &logs[1],
                    "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(88)"
                );
                assert_eq!(&logs[2], "[INFO] [FT_TRANSFER_CALL_SENDER_CREDIT] 100");

                assert_eq!(stake.ft_balance_of(to_valid_account_id(SENDER)), 100.into());
                assert_eq!(
                    stake.ft_balance_of(to_valid_account_id(RECEIVER)),
                    900.into()
                );
            });
        }

        #[test]
        fn over_refund() {
            run_test(Some(100.into()), Some(1000.into()), |mut ctx, mut stake| {
                ctx.predecessor_account_id = ctx.current_account_id.clone();
                let transfer_amount = TokenAmount(500.into());
                let refund_amount = TokenAmount(1000.into());
                let refund_amount_bytes = serde_json::to_vec(&refund_amount).unwrap();
                testing_env_with_promise_results(
                    ctx,
                    vec![PromiseResult::Successful(refund_amount_bytes)],
                );
                let actual_refund_amount = stake.ft_resolve_transfer_call(
                    to_valid_account_id(SENDER),
                    to_valid_account_id(RECEIVER),
                    transfer_amount,
                );
                assert_eq!(actual_refund_amount, transfer_amount);

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(logs.len(), 3);
                assert_eq!(&logs[0], "[ERR] [FT_RESOLVE_TRANSFER] refund amount was greater than the transfer amount - full transfer amount will be refunded");
                assert_eq!(&logs[1], "[INFO] [FT_TRANSFER_CALL_RECEIVER_DEBIT] 500");
                assert_eq!(&logs[2], "[INFO] [FT_TRANSFER_CALL_SENDER_CREDIT] 500");

                assert_eq!(stake.ft_balance_of(to_valid_account_id(SENDER)), 600.into());
                assert_eq!(
                    stake.ft_balance_of(to_valid_account_id(RECEIVER)),
                    500.into()
                );
            });
        }

        #[test]
        fn deserialization_failure() {
            run_test(Some(100.into()), Some(1000.into()), |mut ctx, mut stake| {
                ctx.predecessor_account_id = ctx.current_account_id.clone();
                let transfer_amount = TokenAmount(500.into());
                testing_env_with_promise_results(ctx, vec![PromiseResult::Successful(vec![])]);
                let actual_refund_amount = stake.ft_resolve_transfer_call(
                    to_valid_account_id(SENDER),
                    to_valid_account_id(RECEIVER),
                    transfer_amount,
                );
                assert_eq!(actual_refund_amount, 0.into());

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(logs.len(), 1);
                assert_eq!(&logs[0], "[ERR] [FT_RESOLVE_TRANSFER] failed to deserialize refund amount - no refund will be applied");

                assert_eq!(stake.ft_balance_of(to_valid_account_id(SENDER)), 100.into());
                assert_eq!(
                    stake.ft_balance_of(to_valid_account_id(RECEIVER)),
                    1000.into()
                );
            });
        }

        #[test]
        fn transfer_call_promise_failed() {
            run_test(Some(100.into()), Some(1000.into()), |mut ctx, mut stake| {
                ctx.predecessor_account_id = ctx.current_account_id.clone();
                let transfer_amount = TokenAmount(500.into());
                testing_env_with_promise_results(ctx, vec![PromiseResult::Failed]);
                let actual_refund_amount = stake.ft_resolve_transfer_call(
                    to_valid_account_id(SENDER),
                    to_valid_account_id(RECEIVER),
                    transfer_amount,
                );
                assert_eq!(actual_refund_amount, transfer_amount);

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(logs.len(), 3);
                assert_eq!(
                    &logs[0],
                    "[WARN] [FT_TRANSFER_CALL_FAILURE] full transfer amount will be refunded"
                );
                assert_eq!(&logs[1], "[INFO] [FT_TRANSFER_CALL_RECEIVER_DEBIT] 500");
                assert_eq!(&logs[2], "[INFO] [FT_TRANSFER_CALL_SENDER_CREDIT] 500");

                assert_eq!(stake.ft_balance_of(to_valid_account_id(SENDER)), 600.into());
                assert_eq!(
                    stake.ft_balance_of(to_valid_account_id(RECEIVER)),
                    500.into()
                );
            });
        }

        #[test]
        fn transfer_call_promise_failed_and_receiver_not_registered() {
            run_test(Some(100.into()), None, |mut ctx, mut stake| {
                ctx.predecessor_account_id = ctx.current_account_id.clone();
                let transfer_amount = TokenAmount(500.into());
                testing_env_with_promise_results(ctx, vec![PromiseResult::Failed]);
                let actual_refund_amount = stake.ft_resolve_transfer_call(
                    to_valid_account_id(SENDER),
                    to_valid_account_id(RECEIVER),
                    transfer_amount,
                );
                assert_eq!(actual_refund_amount, 0.into());

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(logs.len(), 2);
                assert_eq!(
                    &logs[0],
                    "[WARN] [FT_TRANSFER_CALL_FAILURE] full transfer amount will be refunded"
                );
                assert_eq!(
                    &logs[1],
                    "[WARN] [FT_TRANSFER_CALL_REFUND_NOT_APPLIED] receiver account not registered"
                );

                assert_eq!(stake.ft_balance_of(to_valid_account_id(SENDER)), 100.into());
                assert_eq!(stake.ft_balance_of(to_valid_account_id(RECEIVER)), 0.into());
            });
        }

        #[test]
        fn refund_specified_with_receiver_not_registered() {
            run_test(Some(100.into()), None, |mut ctx, mut stake| {
                ctx.predecessor_account_id = ctx.current_account_id.clone();
                let transfer_amount = TokenAmount(500.into());
                let refund_amount = TokenAmount(100.into());
                let refund_amount_bytes = serde_json::to_vec(&refund_amount).unwrap();
                testing_env_with_promise_results(
                    ctx,
                    vec![PromiseResult::Successful(refund_amount_bytes)],
                );
                let actual_refund_amount = stake.ft_resolve_transfer_call(
                    to_valid_account_id(SENDER),
                    to_valid_account_id(RECEIVER),
                    transfer_amount,
                );
                assert_eq!(actual_refund_amount, 0.into());

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(logs.len(), 1);
                assert_eq!(
                    &logs[0],
                    "[WARN] [FT_TRANSFER_CALL_REFUND_NOT_APPLIED] receiver account not registered"
                );

                assert_eq!(stake.ft_balance_of(to_valid_account_id(SENDER)), 100.into());
                assert_eq!(stake.ft_balance_of(to_valid_account_id(RECEIVER)), 0.into());
            });
        }

        #[test]
        fn refund_specified_with_receiver_having_insufficient_funds() {
            run_test(Some(100.into()), Some(100.into()), |mut ctx, mut stake| {
                ctx.predecessor_account_id = ctx.current_account_id.clone();
                let transfer_amount = TokenAmount(500.into());
                let refund_amount = TokenAmount(200.into());
                let refund_amount_bytes = serde_json::to_vec(&refund_amount).unwrap();
                testing_env_with_promise_results(
                    ctx,
                    vec![PromiseResult::Successful(refund_amount_bytes)],
                );
                let actual_refund_amount = stake.ft_resolve_transfer_call(
                    to_valid_account_id(SENDER),
                    to_valid_account_id(RECEIVER),
                    transfer_amount,
                );
                assert_eq!(actual_refund_amount, 100.into());

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(logs.len(), 4);
                assert_eq!(
                    &logs[0],
                    "[WARN] [FT_TRANSFER_CALL_PARTIAL_REFUND] partial refund will be applied because receiver account has insufficient fund"
                );
                assert_eq!(
                    &logs[1],
                    "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(-88)"
                );
                assert_eq!(&logs[2], "[INFO] [FT_TRANSFER_CALL_RECEIVER_DEBIT] 100");
                assert_eq!(&logs[3], "[INFO] [FT_TRANSFER_CALL_SENDER_CREDIT] 100");

                assert_eq!(stake.ft_balance_of(to_valid_account_id(SENDER)), 200.into());
                assert_eq!(stake.ft_balance_of(to_valid_account_id(RECEIVER)), 0.into());
            });
        }

        #[test]
        fn refund_specified_receiver_having_zero_balance() {
            run_test(Some(100.into()), Some(0.into()), |mut ctx, mut stake| {
                ctx.predecessor_account_id = ctx.current_account_id.clone();
                let transfer_amount = TokenAmount(500.into());
                let refund_amount = TokenAmount(200.into());
                let refund_amount_bytes = serde_json::to_vec(&refund_amount).unwrap();
                testing_env_with_promise_results(
                    ctx,
                    vec![PromiseResult::Successful(refund_amount_bytes)],
                );
                let actual_refund_amount = stake.ft_resolve_transfer_call(
                    to_valid_account_id(SENDER),
                    to_valid_account_id(RECEIVER),
                    transfer_amount,
                );
                assert_eq!(actual_refund_amount, 0.into());

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(logs.len(), 1);
                assert_eq!(
                    &logs[0],
                    "[WARN] [FT_TRANSFER_CALL_REFUND_NOT_APPLIED] receiver account has zero balance"
                );

                assert_eq!(stake.ft_balance_of(to_valid_account_id(SENDER)), 100.into());
                assert_eq!(stake.ft_balance_of(to_valid_account_id(RECEIVER)), 0.into());
            });
        }

        #[test]
        fn refund_specified_sender_not_registered() {
            run_test(None, Some(1000.into()), |mut ctx, mut stake| {
                ctx.predecessor_account_id = ctx.current_account_id.clone();
                let transfer_amount = TokenAmount(500.into());
                let refund_amount = TokenAmount(200.into());
                let refund_amount_bytes = serde_json::to_vec(&refund_amount).unwrap();
                testing_env_with_promise_results(
                    ctx,
                    vec![PromiseResult::Successful(refund_amount_bytes)],
                );
                let initial_token_supply = stake.ft_total_supply();
                let actual_refund_amount = stake.ft_resolve_transfer_call(
                    to_valid_account_id(SENDER),
                    to_valid_account_id(RECEIVER),
                    transfer_amount,
                );
                assert_eq!(actual_refund_amount, 200.into());

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(logs.len(), 2);
                assert_eq!(&logs[0], "[INFO] [FT_TRANSFER_CALL_RECEIVER_DEBIT] 200");
                assert_eq!(
                    &logs[1],
                    "[INFO] [FT_BURN] sender account is not registered: 200"
                );

                assert_eq!(stake.ft_balance_of(to_valid_account_id(SENDER)), 0.into());
                assert_eq!(
                    stake.ft_balance_of(to_valid_account_id(RECEIVER)),
                    800.into()
                );
                // Assert token supply is reduced when tokens are burned
                assert_eq!(
                    stake.ft_total_supply(),
                    (*initial_token_supply - 200).into()
                );
            });
        }
    }
}

#[cfg(test)]
mod tests_operator {
    use super::*;
    use crate::*;
    use near_sdk::VMContext;
    use oysterpack_smart_account_management::components::account_management::{
        AccountManagementComponentConfig, ContractPermissions,
    };
    use oysterpack_smart_account_management::{PermissionsManagement, StorageManagement};
    use oysterpack_smart_near::YOCTO;
    use oysterpack_smart_near_test::*;

    type AccountDataType = ();
    type AccountManager = AccountManagementComponent<AccountDataType>;
    type STAKE = FungibleTokenComponent<AccountDataType>;

    const ADMIN: &str = "admin";

    fn deploy_comps() {
        AccountManager::deploy(AccountManagementComponentConfig::new(to_valid_account_id(
            ADMIN,
        )));

        STAKE::deploy(Config {
            metadata: Metadata {
                spec: FT_METADATA_SPEC.into(),
                name: "STAKE".into(),
                symbol: "STAKE".into(),
                icon: None,
                reference: None,
                reference_hash: None,
                decimals: 24,
            },
            token_supply: YOCTO,
        });
    }

    #[test]
    fn operator_commands() {
        // Arrange
        let operator = "operator";
        let mut ctx = new_context(operator);
        testing_env!(ctx.clone());

        deploy_comps();

        let mut account_manager = AccountManager::new(&ContractPermissions::default());

        // register operator account
        {
            ctx.attached_deposit = YOCTO;
            testing_env!(ctx.clone());
            account_manager.storage_deposit(None, None);

            ctx.attached_deposit = 0;
            ctx.predecessor_account_id = ADMIN.to_string();
            testing_env!(ctx.clone());
            account_manager.ops_permissions_grant_operator(to_valid_account_id(operator));
        }

        let mut stake = STAKE::new(account_manager);

        fn run_operator_commands(mut ctx: VMContext, stake: &mut STAKE, account_id: &str) {
            // Act
            ctx.predecessor_account_id = account_id.to_string();
            testing_env!(ctx.clone());
            let icon = Icon("data://image/svg+xml,<svg></svg>".to_string());
            let command = OperatorCommand::SetIcon(icon.clone());
            println!("{}", serde_json::to_string(&command).unwrap());
            stake.ft_operator_command(command);
            // Assert
            let metadata = stake.ft_metadata();
            assert_eq!(metadata.icon, Some(icon));

            // Act
            ctx.predecessor_account_id = account_id.to_string();
            testing_env!(ctx.clone());
            let reference = Reference("http://stake.json".to_string());
            let hash = Hash::from("reference");
            let command = OperatorCommand::SetReference(reference.clone(), hash);
            println!("{}", serde_json::to_string(&command).unwrap());
            stake.ft_operator_command(command);
            // Assert
            let metadata = stake.ft_metadata();
            assert_eq!(metadata.reference, Some(reference));
            assert_eq!(metadata.reference_hash, Some(hash));

            // Act
            ctx.predecessor_account_id = account_id.to_string();
            testing_env!(ctx.clone());
            let command = OperatorCommand::ClearIcon;
            println!("{}", serde_json::to_string(&command).unwrap());
            stake.ft_operator_command(command);
            // Assert
            let metadata = stake.ft_metadata();
            assert!(metadata.icon.is_none());

            // Act
            ctx.predecessor_account_id = account_id.to_string();
            testing_env!(ctx.clone());
            let command = OperatorCommand::ClearReference;
            println!("{}", serde_json::to_string(&command).unwrap());
            stake.ft_operator_command(command);
            // Assert
            let metadata = stake.ft_metadata();
            assert!(metadata.reference.is_none());
            assert!(metadata.reference_hash.is_none());
        }

        run_operator_commands(ctx.clone(), &mut stake, ADMIN);
        run_operator_commands(ctx.clone(), &mut stake, operator);
    }

    #[test]
    #[should_panic(expected = "[ERR] [NOT_AUTHORIZED]")]
    fn operator_commands_as_not_operator() {
        // Arrange
        let account = "account";
        let mut ctx = new_context(account);
        testing_env!(ctx.clone());

        deploy_comps();

        let mut account_manager = AccountManager::new(&ContractPermissions::default());

        // register normal account with no permissions
        {
            ctx.attached_deposit = YOCTO;
            testing_env!(ctx.clone());
            account_manager.storage_deposit(None, None);
        }

        let mut stake = STAKE::new(account_manager);
        stake.ft_operator_command(OperatorCommand::ClearReference);
    }

    #[test]
    #[should_panic(expected = "[ERR] [ACCOUNT_NOT_REGISTERED]")]
    fn operator_commands_with_unregistered_account() {
        // Arrange
        let account = "account";
        let ctx = new_context(account);
        testing_env!(ctx);

        deploy_comps();

        let account_manager = AccountManager::new(&ContractPermissions::default());

        let mut stake = STAKE::new(account_manager);
        stake.ft_operator_command(OperatorCommand::ClearReference);
    }
}

#[cfg(test)]
mod tests_token_service {
    use super::*;

    use crate::*;
    use near_sdk::{test_utils, VMContext};
    use oysterpack_smart_account_management::components::account_management::{
        AccountManagementComponentConfig, ContractPermissions,
    };
    use oysterpack_smart_account_management::StorageManagement;
    use oysterpack_smart_near::YOCTO;
    use oysterpack_smart_near_test::*;

    type AccountDataType = ();
    type AccountManager = AccountManagementComponent<AccountDataType>;
    type STAKE = FungibleTokenComponent<AccountDataType>;

    const ADMIN: &str = "admin";

    fn deploy_comps() {
        AccountManager::deploy(AccountManagementComponentConfig::new(to_valid_account_id(
            ADMIN,
        )));

        STAKE::deploy(Config {
            metadata: Metadata {
                spec: FT_METADATA_SPEC.into(),
                name: "STAKE".into(),
                symbol: "STAKE".into(),
                icon: None,
                reference: None,
                reference_hash: None,
                decimals: 24,
            },
            token_supply: YOCTO,
        });
    }

    const ACCOUNT: &str = "bob";

    /// - if the balances are Some then register them and mint tokens for them
    fn run_test<F>(account_balance: Option<TokenAmount>, test: F)
    where
        F: FnOnce(VMContext, STAKE),
    {
        // Arrange
        let ctx = new_context(ACCOUNT);
        testing_env!(ctx.clone());

        deploy_comps();

        let account_manager = AccountManager::new(&ContractPermissions::default());

        let mut stake = STAKE::new(account_manager);

        // register accounts
        {
            let mut account_manager = AccountManager::new(&ContractPermissions::default());

            if let Some(balance) = account_balance {
                let mut ctx = ctx.clone();
                ctx.predecessor_account_id = ACCOUNT.to_string();
                ctx.attached_deposit = account_manager.storage_balance_bounds().min.value();
                testing_env!(ctx.clone());
                account_manager.storage_deposit(None, Some(true));

                if *balance > 0 {
                    stake.ft_mint(ACCOUNT, balance);
                }
            }
        }

        test(ctx, stake);
    }

    #[cfg(test)]
    mod tests_mint {
        use super::*;

        #[test]
        fn account_registered_with_zero_token_balance() {
            run_test(Some(0.into()), |ctx, mut stake| {
                testing_env!(ctx);
                let initial_token_supply = stake.ft_total_supply();
                stake.ft_mint(ACCOUNT, 1000.into());
                assert_eq!(
                    stake.ft_total_supply(),
                    (*initial_token_supply + 1000).into()
                );

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(logs.len(), 2);
                assert_eq!(
                    &logs[0],
                    "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(88)"
                );
                assert_eq!(&logs[1], "[INFO] [FT_MINT] account: bob, amount: 1000");

                assert_eq!(
                    stake.ft_balance_of(to_valid_account_id(ACCOUNT)),
                    1000.into()
                );
            });
        }

        #[test]
        fn account_registered_with_token_balance() {
            run_test(Some(1000.into()), |ctx, mut stake| {
                testing_env!(ctx);
                let initial_token_supply = stake.ft_total_supply();
                stake.ft_mint(ACCOUNT, 1000.into());
                assert_eq!(
                    stake.ft_total_supply(),
                    (*initial_token_supply + 1000).into()
                );

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(logs.len(), 1);
                assert_eq!(&logs[0], "[INFO] [FT_MINT] account: bob, amount: 1000");

                assert_eq!(
                    stake.ft_balance_of(to_valid_account_id(ACCOUNT)),
                    2000.into()
                );
            });
        }

        #[test]
        #[should_panic(expected = "[ERR] [ACCOUNT_NOT_REGISTERED]")]
        fn account_not_registered() {
            run_test(None, |ctx, mut stake| {
                testing_env!(ctx);
                stake.ft_mint(ACCOUNT, 1000.into());
            });
        }

        #[test]
        #[should_panic(expected = "[ERR] [INVALID] mint amount cannot be zero")]
        fn zero_amount() {
            run_test(Some(1000.into()), |ctx, mut stake| {
                testing_env!(ctx);
                stake.ft_mint(ACCOUNT, 0.into());
            });
        }
    }

    #[cfg(test)]
    mod tests_burn {
        use super::*;

        #[test]
        fn burn_account_partial_balance() {
            run_test(Some(10000.into()), |ctx, mut stake| {
                testing_env!(ctx);
                let initial_token_supply = stake.ft_total_supply();
                stake.ft_burn(ACCOUNT, 1000.into());
                assert_eq!(
                    stake.ft_total_supply(),
                    (*initial_token_supply - 1000).into()
                );

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(logs.len(), 1);
                assert_eq!(&logs[0], "[INFO] [FT_BURN] account: bob, amount: 1000");

                assert_eq!(
                    stake.ft_balance_of(to_valid_account_id(ACCOUNT)),
                    9000.into()
                );
            });
        }

        #[test]
        fn burn_account_full_balance() {
            run_test(Some(10000.into()), |ctx, mut stake| {
                testing_env!(ctx);
                let initial_token_supply = stake.ft_total_supply();
                stake.ft_burn(ACCOUNT, 10000.into());
                assert_eq!(
                    stake.ft_total_supply(),
                    (*initial_token_supply - 10000).into()
                );

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(
                    logs,
                    vec![
                        "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(-88)",
                        "[INFO] [FT_BURN] account: bob, amount: 10000",
                    ]
                );

                assert_eq!(stake.ft_balance_of(to_valid_account_id(ACCOUNT)), 0.into());
            });
        }

        #[test]
        #[should_panic(expected = "[ERR] [ACCOUNT_NOT_REGISTERED]")]
        fn account_not_registered() {
            run_test(None, |_ctx, mut stake| {
                stake.ft_burn(ACCOUNT, 10000.into());
            });
        }

        #[test]
        #[should_panic(expected = "[ERR] [INVALID] burn amount cannot be zero")]
        fn burn_zero_amount() {
            run_test(Some(1000.into()), |_ctx, mut stake| {
                stake.ft_burn(ACCOUNT, 0.into());
            });
        }

        #[test]
        #[should_panic(expected = "[ERR] [INVALID] account has insufficient funds")]
        fn account_has_insufficient_funds() {
            run_test(Some(1.into()), |_ctx, mut stake| {
                stake.ft_burn(ACCOUNT, 10000.into());
            });
        }
    }

    #[cfg(test)]
    mod tests_burn_all {
        use super::*;

        #[test]
        fn account_registered_with_balance() {
            run_test(Some(10000.into()), |ctx, mut stake| {
                testing_env!(ctx);
                let initial_token_supply = stake.ft_total_supply();
                stake.ft_burn_all(ACCOUNT);
                assert_eq!(
                    stake.ft_total_supply(),
                    (*initial_token_supply - 10000).into()
                );

                let logs = test_utils::get_logs();
                println!("{:#?}", logs);
                assert_eq!(
                    logs,
                    vec![
                        "[INFO] [ACCOUNT_STORAGE_CHANGED] StorageUsageChange(-88)",
                        "[INFO] [FT_BURN] account: bob, amount: 10000",
                    ]
                );

                assert_eq!(stake.ft_balance_of(to_valid_account_id(ACCOUNT)), 0.into());
            });
        }

        #[test]
        fn account_registered_with_zero_balance() {
            run_test(Some(0.into()), |ctx, mut stake| {
                testing_env!(ctx);
                let initial_token_supply = stake.ft_total_supply();
                stake.ft_burn_all(ACCOUNT);
                assert_eq!(stake.ft_total_supply(), initial_token_supply);

                let logs = test_utils::get_logs();
                assert!(logs.is_empty());

                assert_eq!(stake.ft_balance_of(to_valid_account_id(ACCOUNT)), 0.into());
            });
        }

        #[test]
        fn account_not_registered() {
            run_test(Some(0.into()), |ctx, mut stake| {
                testing_env!(ctx);
                let initial_token_supply = stake.ft_total_supply();
                stake.ft_burn_all("doesnotexist");
                assert_eq!(stake.ft_total_supply(), initial_token_supply);

                let logs = test_utils::get_logs();
                assert!(logs.is_empty());

                assert_eq!(stake.ft_balance_of(to_valid_account_id(ACCOUNT)), 0.into());
            });
        }
    }
}

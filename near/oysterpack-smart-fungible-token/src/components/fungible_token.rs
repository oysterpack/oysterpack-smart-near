//! [`FungibleTokenComponent`]
//! - constructor: [`FungibleTokenComponent::new`]
//!   - [`AccountManagementComponent`]
//! - deployment: [`FungibleTokenComponent::deploy`]
//!   - config: [`Config`]
//! - [`UnregisterAccount`] -> [`UnregisterFungibleTokenAccount`]

#![allow(unused_variables)]

use crate::{
    FungibleToken, FungibleTokenMetadataProvider, Memo, Metadata, TokenAmount, TransferCallMessage,
    LOG_EVENT_FT_TRANSFER,
};
use near_sdk::{
    borsh::{BorshDeserialize, BorshSerialize},
    env,
    json_types::ValidAccountId,
    serde::{Deserialize, Serialize},
    serde_json, AccountId, Gas, Promise, PromiseOrValue, RuntimeFeesConfig,
};
use oysterpack_smart_account_management::components::account_management::{
    AccountManagementComponent, UnregisterAccount, ERR_CODE_UNREGISTER_FAILURE,
};
use oysterpack_smart_account_management::{AccountRepository, ERR_ACCOUNT_NOT_REGISTERED};
use oysterpack_smart_near::asserts::{
    assert_yocto_near_attached, ERR_CODE_BAD_REQUEST, ERR_INSUFFICIENT_FUNDS,
};
use oysterpack_smart_near::{component::Deploy, data::Object, to_valid_account_id, Hash, TERA};
use std::{fmt::Debug, ops::Deref};
use teloc::*;

pub struct FungibleTokenComponent<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    account_manager: AccountManagementComponent<T>,
}

#[inject]
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
#[serde(crate = "near_sdk::serde")]
pub struct Config {
    pub metadata: Metadata,
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

        let sender_balance = self.ft_balance_of(to_valid_account_id(sender_id));
        ERR_INSUFFICIENT_FUNDS.assert(|| *sender_balance >= *amount);

        ERR_ACCOUNT_NOT_REGISTERED.assert_with_message(
            || self.account_manager.account_exists(receiver_id.as_ref()),
            || "receiver account is not registered",
        );

        // transfer the tokens
        ft_set_balance(sender_id, *sender_balance - *amount);
        let receiver_balance = self.ft_balance_of(receiver_id.clone());
        ft_set_balance(receiver_id.as_ref(), *receiver_balance + *amount);

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
        ft_balance_of(account_id.as_ref())
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

        let ft_resolve_transfer_call = b"ft_resolve_transfer_call".to_vec();
        let ft_resolve_transfer_call_args = serde_json::to_vec(&ResolveTransferArgs {
            sender_id: sender_id.to_string(),
            receiver_id: receiver_id.to_string(),
            amount,
        })
        .expect("");
        let ft_resolve_transfer_call_bytes: u64 =
            (ft_resolve_transfer_call.len() + ft_resolve_transfer_call_args.len()) as u64;
        let ft_resolve_transfer_call_promise = Promise::new(env::current_account_id())
            .function_call(
                ft_resolve_transfer_call,
                ft_resolve_transfer_call_args,
                0,
                self.ft_resolve_transfer_call_gas(),
            );

        // compute how much gas is needed to complete this call and the resolve transfer callback
        // and then give the rest of the gas to the transfer receiver call
        let runtime_fees = RuntimeFeesConfig::default();
        let ft_on_transfer_receipt_action_cost = {
            let action_receipt_creation_cost =
                runtime_fees.action_receipt_creation_config.send_not_sir
                    + runtime_fees.action_receipt_creation_config.execution;

            let function_call_cost_per_byte = runtime_fees
                .action_creation_config
                .function_call_cost_per_byte
                .send_not_sir
                + runtime_fees
                    .action_creation_config
                    .function_call_cost_per_byte
                    .execution;
            let func_call_bytes: u64 = (ft_on_transfer.len() + ft_on_transfer_args.len()) as u64;
            let func_call_cost = function_call_cost_per_byte * func_call_bytes;

            runtime_fees.min_receipt_with_function_call_gas()
                + action_receipt_creation_cost
                + func_call_cost
        };
        let ft_resolve_transfer_call_receipt_action_cost = {
            let action_receipt_creation_cost = runtime_fees.action_receipt_creation_config.send_sir
                + runtime_fees.action_receipt_creation_config.execution;

            let function_call_cost_per_byte = runtime_fees
                .action_creation_config
                .function_call_cost_per_byte
                .send_sir
                + runtime_fees
                    .action_creation_config
                    .function_call_cost_per_byte
                    .execution;

            let func_call_cost = function_call_cost_per_byte * ft_resolve_transfer_call_bytes;

            let data_receipt_cost_per_byte = runtime_fees
                .data_receipt_creation_config
                .cost_per_byte
                .send_sir
                + runtime_fees
                    .data_receipt_creation_config
                    .cost_per_byte
                    .execution;
            let data_receipt_cost = runtime_fees.data_receipt_creation_config.base_cost.send_sir
                + runtime_fees
                    .data_receipt_creation_config
                    .base_cost
                    .execution
                + (data_receipt_cost_per_byte * 100);

            runtime_fees.min_receipt_with_function_call_gas()
                + action_receipt_creation_cost
                + func_call_cost
                + data_receipt_cost
        };
        let ft_on_transfer_gas = env::prepaid_gas()
            - env::used_gas()
            - self.ft_resolve_transfer_call_gas()
            - ft_on_transfer_receipt_action_cost
            - ft_resolve_transfer_call_receipt_action_cost;

        Promise::new(receiver_id.to_string())
            .function_call(ft_on_transfer, ft_on_transfer_args, 0, ft_on_transfer_gas)
            .then(ft_resolve_transfer_call_promise)
    }

    // TODO: make configurable
    fn ft_resolve_transfer_call_gas(&self) -> Gas {
        5 * TERA
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct OnTransferArgs {
    sender_id: AccountId,
    amount: TokenAmount,
    msg: TransferCallMessage,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct ResolveTransferArgs {
    sender_id: AccountId,
    receiver_id: AccountId,
    amount: TokenAmount,
}

const TOKEN_SUPPLY: u128 = 1953830723745925743018307013370321490;
type TokenSupply = Object<u128, u128>;

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
        unimplemented!()
    }
}

const FT_ACCOUNT_KEY: u128 = 1953845438124731969041175284518648060;
type AccountFTBalance = Object<Hash, u128>;

pub(crate) fn ft_set_balance(account_id: &str, balance: u128) {
    let account_hash_id = Hash::from((account_id, FT_ACCOUNT_KEY));
    AccountFTBalance::new(account_hash_id, balance).save();
}

pub(crate) fn ft_balance_of(account_id: &str) -> TokenAmount {
    let account_hash_id = Hash::from((account_id, FT_ACCOUNT_KEY));
    AccountFTBalance::load(&account_hash_id).map_or(0.into(), |balance| (*balance).into())
}

/// Must be registered with [`AccountManagementComponent`]
///
/// When an account is forced unregistered, any tokens it owned will be burned, which reduces the total
/// token supply.
#[derive(Dependency)]
pub struct UnregisterFungibleTokenAccount;

impl UnregisterAccount for UnregisterFungibleTokenAccount {
    /// if force = true, then any tokens that the account owned will be burned, which will reduce
    /// the total token supply
    fn unregister_account(&mut self, force: bool) {
        let delete_account = || {
            let account_hash_id =
                Hash::from((env::predecessor_account_id().as_str(), FT_ACCOUNT_KEY));
            AccountFTBalance::delete_by_key(&account_hash_id);
        };

        if force {
            // burn any account token balance
            let token_balance = *ft_balance_of(&env::predecessor_account_id());
            if token_balance > 0 {
                let mut token_supply = TokenSupply::load(&TOKEN_SUPPLY).unwrap();
                *token_supply -= token_balance;
                token_supply.save();
            }

            delete_account();
        } else {
            ERR_CODE_UNREGISTER_FAILURE.assert(
                || *ft_balance_of(&env::predecessor_account_id()) == 0,
                || "account failed to unregister because the account has a token balance",
            );
            delete_account();
        }
    }
}

/// Callback on fungible token contract to resolve transfer.
pub trait ResolveTransferCall {
    /// Callback to resolve transfer.
    /// Private method (`env::predecessor_account_id == env::current_account_id`).
    ///
    /// Called after the receiver handles the transfer call and returns unused token amount.
    ///
    /// This method must get `unused_amount` from the receiver's promise result and refund the
    /// `unused_amount` from the receiver's account back to the `sender_id` account.
    ///
    /// Arguments:
    /// - `sender_id` - the account ID that initiated the transfer.
    /// - `receiver_id` - the account ID of the receiver contract.
    /// - `amount` - the amount of tokens that were transferred to receiver's account.
    ///
    /// Promise result data dependency (`unused_amount`):
    /// - the amount of tokens that were unused by receiver's contract.
    /// - Received from `on_ft_receive`
    /// - `unused_amount` must be `U128` in range from `0` to `amount`. All other invalid values
    ///   are considered to be equal to be the total transfer amount.
    ///
    /// Returns amount that was refunded back to the sender.
    ///
    /// The callback should be designed to never panic.
    /// - if the `sender_id` is not registered, then refunded STAKE tokens will be burned
    /// - if the `receiver_id` is not registered, then the contract should be handle it
    ///
    /// #\[private\]
    fn ft_resolve_transfer_call(
        &mut self,
        sender_id: ValidAccountId,
        receiver_id: ValidAccountId,
        amount: TokenAmount,
        // NOTE: #[callback_result] is not supported yet and has to be handled using lower level interface.
        //
        // #[callback_result]
        // unused_amount: CallbackResult<TokenAmount>,
    ) -> TokenAmount;
}

// #[ext_contract(ext_transfer_receiver)]
trait ExtTransferReceiver {
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: TokenAmount,
        msg: TransferCallMessage,
    ) -> PromiseOrValue<TokenAmount>;
}

// #[ext_contract(ext_resolve_transfer_call)]
trait ExtResolveTransferCall {
    fn ft_resolve_transfer_call(
        &mut self,
        sender_id: AccountId,
        receiver_id: AccountId,
        amount: TokenAmount,
    ) -> TokenAmount;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FungibleToken;
    use crate::*;
    use oysterpack_smart_account_management::components::account_management::UnregisterAccountNOOP;
    use oysterpack_smart_account_management::{StorageManagement, StorageUsageBounds};
    use oysterpack_smart_near::YOCTO;
    use oysterpack_smart_near_test::*;

    type AccountDataType = ();
    type AccountManager = AccountManagementComponent<AccountDataType>;
    type STAKE = FungibleTokenComponent<AccountDataType>;

    #[test]
    fn basic_workflow() {
        // Arrange
        let sender = "sender";
        let receiver = "receiver";
        let mut ctx = new_context(sender);
        testing_env!(ctx.clone());

        AccountManager::deploy(StorageUsageBounds {
            min: AccountManager::measure_storage_usage(()),
            max: None,
        });

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

        let mut account_manager = AccountManager::new(Box::new(UnregisterAccountNOOP));

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
        ft_set_balance(sender, 100);

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
    }
}

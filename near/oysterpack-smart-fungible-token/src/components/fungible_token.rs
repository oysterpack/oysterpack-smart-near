//! [`FungibleTokenComponent`]
//! - constructor: [`FungibleTokenComponent::new`]
//!   - [`AccountManagementComponent`]
//! - deployment: [`FungibleTokenComponent::deploy`]
//!   - config: [`Config`]
//! - [`UnregisterAccount`] -> [`UnregisterFungibleTokenAccount`]

#![allow(unused_variables)]

use crate::contract::operator::{FungibleTokenOperator, OperatorCommand};
use crate::{
    FungibleToken, FungibleTokenMetadataProvider, Memo, Metadata, ResolveTransferCall, TokenAmount,
    TokenService, TransferCallMessage, LOG_EVENT_FT_BURN, LOG_EVENT_FT_MINT, LOG_EVENT_FT_TRANSFER,
    LOG_EVENT_FT_TRANSFER_CALL_FAILURE, LOG_EVENT_FT_TRANSFER_CALL_PARTIAL_REFUND,
    LOG_EVENT_FT_TRANSFER_CALL_RECEIVER_DEBIT, LOG_EVENT_FT_TRANSFER_CALL_REFUND_NOT_APPLIED,
    LOG_EVENT_FT_TRANSFER_CALL_SENDER_CREDIT,
};
use near_sdk::{
    borsh::{BorshDeserialize, BorshSerialize},
    env,
    json_types::ValidAccountId,
    serde::{Deserialize, Serialize},
    serde_json, AccountId, Gas, Promise, PromiseResult, RuntimeFeesConfig,
};
use oysterpack_smart_account_management::components::account_management::{
    AccountManagementComponent, UnregisterAccount, ERR_CODE_UNREGISTER_FAILURE,
};
use oysterpack_smart_account_management::{
    AccountRepository, ERR_ACCOUNT_NOT_REGISTERED, ERR_NOT_AUTHORIZED,
};
use oysterpack_smart_near::asserts::{
    assert_yocto_near_attached, ERR_CODE_BAD_REQUEST, ERR_INSUFFICIENT_FUNDS, ERR_INVALID,
};
use oysterpack_smart_near::{component::Deploy, data::Object, to_valid_account_id, Hash, TERA};
use std::{cmp::min, fmt::Debug, ops::Deref};
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

impl<T> FungibleTokenOperator for FungibleTokenComponent<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    fn ft_operator_command(&mut self, command: OperatorCommand) {
        self.assert_operator();
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
        }
        metadata.save();
    }
}

impl<T> TokenService for FungibleTokenComponent<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    fn ft_mint(&mut self, account_id: &str, amount: TokenAmount) {
        ERR_INVALID.assert(|| *amount > 0, || "amount cannot be zero");
        ERR_ACCOUNT_NOT_REGISTERED.assert(|| self.account_manager.account_exists(account_id));

        let mut ft_balance = account_ft_balance(account_id);
        *ft_balance += *amount;
        ft_balance.save();

        let mut token_supply = token_supply();
        *token_supply += *amount;
        token_supply.save();

        LOG_EVENT_FT_MINT.log(format!("account_id: {}, amount: {}", account_id, amount));
    }

    fn ft_burn(&mut self, account_id: &str, amount: TokenAmount) {
        ERR_INVALID.assert(|| *amount > 0, || "amount cannot be zero");
        ERR_ACCOUNT_NOT_REGISTERED.assert(|| self.account_manager.account_exists(account_id));

        let account_id_hash = Hash::from(account_id);
        let mut ft_balance = AccountFTBalance::load(&account_id_hash).unwrap();
        *ft_balance -= *amount;
        ft_balance.save();

        burn_tokens(*amount);

        LOG_EVENT_FT_BURN.log(format!("account_id: {}, amount: {}", account_id, amount));
    }
}

impl<T> FungibleTokenComponent<T>
where
    T: BorshSerialize + BorshDeserialize + Clone + Debug + PartialEq + Default,
{
    /// asserts that the predecessor account ID is registered and has operator permission
    fn assert_operator(&self) {
        let account = self
            .account_manager
            .registered_account_near_data(env::predecessor_account_id().as_str());
        ERR_NOT_AUTHORIZED.assert(|| account.is_operator());
    }

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
        // Get the unused amount from the `ft_on_transfer` call result.
        let unused_amount = match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Successful(value) => {
                if let Ok(unused_amount) = near_sdk::serde_json::from_slice::<TokenAmount>(&value) {
                    min(amount, unused_amount)
                } else {
                    amount
                }
            }
            PromiseResult::Failed => {
                LOG_EVENT_FT_TRANSFER_CALL_FAILURE.log("");
                amount
            }
        };

        if *unused_amount > 0 {
            // try to refund the unused amount from the receiver back to the sender
            if let Some(mut receiver_account_balance) =
                ft_load_account_balance(receiver_id.as_ref())
            {
                let receiver_balance = *receiver_account_balance;
                if receiver_balance > 0 {
                    let refund_amount = min(receiver_balance, *unused_amount);
                    if refund_amount < *unused_amount {
                        LOG_EVENT_FT_TRANSFER_CALL_PARTIAL_REFUND.log("partial refund will be applied because receiver account has insufficient fund");
                    }
                    *receiver_account_balance -= refund_amount;
                    receiver_account_balance.save();
                    LOG_EVENT_FT_TRANSFER_CALL_RECEIVER_DEBIT.log(refund_amount);

                    match ft_load_account_balance(sender_id.as_ref()) {
                        Some(mut sender_account_balance) => {
                            *sender_account_balance += refund_amount;
                            sender_account_balance.save();
                            LOG_EVENT_FT_TRANSFER_CALL_SENDER_CREDIT.log(refund_amount);
                        }
                        None => {
                            burn_tokens(refund_amount);
                            LOG_EVENT_FT_BURN
                                .log(format!(
                                "tokens were burned because sender account is not registered: {}"
                            ,refund_amount));
                        }
                    }
                } else {
                    LOG_EVENT_FT_TRANSFER_CALL_REFUND_NOT_APPLIED
                        .log("receiver account has zero balance");
                }
            } else {
                LOG_EVENT_FT_TRANSFER_CALL_REFUND_NOT_APPLIED
                    .log("receiver account not registered");
            }
        }

        unused_amount
    }
}

const FT_ACCOUNT_KEY: u128 = 1953845438124731969041175284518648060;
type AccountFTBalance = Object<Hash, u128>;

fn ft_set_balance(account_id: &str, balance: u128) {
    let account_hash_id = ft_account_id_hash(account_id);
    AccountFTBalance::new(account_hash_id, balance).save();
}

fn account_ft_balance(account_id: &str) -> AccountFTBalance {
    let account_hash_id = ft_account_id_hash(account_id);
    AccountFTBalance::load(&account_hash_id)
        .unwrap_or_else(|| AccountFTBalance::new(account_hash_id, 0))
}

fn ft_balance_of(account_id: &str) -> TokenAmount {
    ft_load_account_balance(account_id).map_or(0.into(), |balance| (*balance).into())
}

fn ft_load_account_balance(account_id: &str) -> Option<AccountFTBalance> {
    let account_hash_id = ft_account_id_hash(account_id);
    AccountFTBalance::load(&account_hash_id)
}

fn ft_account_id_hash(account_id: &str) -> Hash {
    Hash::from((account_id, FT_ACCOUNT_KEY))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FungibleToken;
    use crate::*;
    use near_sdk::{test_utils, VMContext};
    use oysterpack_smart_account_management::components::account_management::{
        AccountManagementComponentConfig, ContractPermissions, UnregisterAccountNOOP,
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
    fn basic_workflow() {
        // Arrange
        let sender = "sender";
        let receiver = "receiver";
        let mut ctx = new_context(sender);
        testing_env!(ctx.clone());

        deploy_comps();

        let mut account_manager = AccountManager::new(
            Box::new(UnregisterAccountNOOP),
            &ContractPermissions::default(),
        );

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

    #[test]
    fn operator_commands() {
        // Arrange
        let operator = "operator";
        let mut ctx = new_context(operator);
        testing_env!(ctx.clone());

        deploy_comps();

        let mut account_manager = AccountManager::new(
            Box::new(UnregisterAccountNOOP),
            &ContractPermissions::default(),
        );

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
        let metadata = stake.ft_metadata();

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

        let mut account_manager = AccountManager::new(
            Box::new(UnregisterAccountNOOP),
            &ContractPermissions::default(),
        );

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

        let account_manager = AccountManager::new(
            Box::new(UnregisterAccountNOOP),
            &ContractPermissions::default(),
        );

        let mut stake = STAKE::new(account_manager);
        stake.ft_operator_command(OperatorCommand::ClearReference);
    }

    #[cfg(test)]
    mod test_ft_transfer {
        use super::*;

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

            let account_manager = AccountManager::new(
                Box::new(UnregisterAccountNOOP),
                &ContractPermissions::default(),
            );

            let mut stake = STAKE::new(account_manager);

            // register accounts
            {
                let mut account_manager = AccountManager::new(
                    Box::new(UnregisterAccountNOOP),
                    &ContractPermissions::default(),
                );

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
                assert!(logs.is_empty());
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
                assert!(logs.is_empty());
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
                assert_eq!(logs.len(), 1);
                assert_eq!(&logs[0], "[INFO] [FT_TRANSFER] memo");
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
}

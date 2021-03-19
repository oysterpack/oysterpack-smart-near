use crate::{
    FungibleToken, FungibleTokenMetadataProvider, Memo, Metadata, TokenAmount, TransferCallMessage,
    LOG_EVENT_FT_TRANSFER,
};
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    env,
    json_types::ValidAccountId,
    serde::{Deserialize, Serialize},
    Promise, PromiseOrValue,
};
use oysterpack_smart_account_management::components::account_management::AccountManagementComponent;
use oysterpack_smart_account_management::AccountRepository;
use oysterpack_smart_near::asserts::{
    assert_yocto_near_attached, ERR_CODE_BAD_REQUEST, ERR_INSUFFICIENT_FUNDS,
};
use oysterpack_smart_near::{component::Deploy, data::Object};
use std::{fmt::Debug, ops::Deref};

pub struct FungibleTokenComponent<T>
where
    T: BorshSerialize
        + BorshDeserialize
        + Clone
        + Debug
        + PartialEq
        + Default
        + HasFungibleTokenBalance,
{
    account_manager: AccountManagementComponent<T>,
}

/// Account data type must implement [`AccountFungibleTokenBalance`]
pub trait HasFungibleTokenBalance {
    fn ft_balance(&self) -> u128;

    fn set_ft_balance(&mut self, balance: u128);
}

impl<T> Deploy for FungibleTokenComponent<T>
where
    T: BorshSerialize
        + BorshDeserialize
        + Clone
        + Debug
        + PartialEq
        + Default
        + HasFungibleTokenBalance,
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
    T: BorshSerialize
        + BorshDeserialize
        + Clone
        + Debug
        + PartialEq
        + Default
        + HasFungibleTokenBalance,
{
    fn ft_transfer(
        &mut self,
        receiver_id: ValidAccountId,
        amount: TokenAmount,
        memo: Option<Memo>,
    ) {
        assert_yocto_near_attached();
        ERR_CODE_BAD_REQUEST.assert(|| *amount > 0, || "transfer amount cannot be zero");
        let sender_id = env::predecessor_account_id();
        ERR_CODE_BAD_REQUEST.assert(
            || &sender_id != receiver_id.as_ref(),
            || "sender and receiver cannot be the same",
        );
        let mut sender = self
            .account_manager
            .registered_account_data(&env::predecessor_account_id());
        let sender_balance = sender.ft_balance();
        ERR_INSUFFICIENT_FUNDS.assert(|| sender_balance >= *amount);
        let mut receiver = self
            .account_manager
            .registered_account_data(receiver_id.as_ref());

        sender.set_ft_balance(sender_balance - *amount);
        sender.save();

        let receiver_balance = receiver.ft_balance();
        receiver.set_ft_balance(receiver_balance + *amount);
        receiver.save();

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
        unimplemented!()
    }

    fn ft_total_supply(&self) -> TokenAmount {
        TokenSupply::load(&TOKEN_SUPPLY).map_or(0.into(), |amount| (*amount).into())
    }

    fn ft_balance_of(&self, account_id: ValidAccountId) -> TokenAmount {
        unimplemented!()
    }
}

const TOKEN_SUPPLY: u128 = 1953830723745925743018307013370321490;
type TokenSupply = Object<u128, u128>;

const METADATA_KEY: u128 = 1953827270399390220126384465824835887;
type MetadataObject = Object<u128, Metadata>;

impl<T> FungibleTokenMetadataProvider for FungibleTokenComponent<T>
where
    T: BorshSerialize
        + BorshDeserialize
        + Clone
        + Debug
        + PartialEq
        + Default
        + HasFungibleTokenBalance,
{
    fn ft_metadata(&self) -> Metadata {
        MetadataObject::load(&METADATA_KEY).unwrap().deref().clone()
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
    ) -> PromiseOrValue<TokenAmount>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use oysterpack_smart_near_test::*;

    #[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Default)]
    struct Account {
        ft_balance: u128,
    }

    impl HasFungibleTokenBalance for Account {
        fn ft_balance(&self) -> u128 {
            self.ft_balance
        }

        fn set_ft_balance(&mut self, balance: u128) {
            self.ft_balance = balance
        }
    }

    type AccountManager = AccountManagementComponent<Account>;

    #[test]
    fn total_supply() {
        // Arrange
        let account = "alfio";
        let mut ctx = new_context(account);
    }
}

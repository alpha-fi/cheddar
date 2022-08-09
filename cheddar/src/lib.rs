/// Cheddar Token
/// Functionality:
/// - No account storage complexity - Since NEAR slashed storage price by 10x
/// it does not make sense to add that friction (storage backup per user).
/// Token creator must store enough NEAR in the contract to support growth.
/// - Multi-minters, no fixed total_supply:
/// The owner can add/remove allowed minters. This is useful if you want
/// an external contract, a farm for example, to be able to mint tokens
/// - Ultra-Lazy ft-metadata: ft-metadata is not stored unless changed
///
use near_contract_standards::fungible_token::{
    core::FungibleTokenCore,
    metadata::{FungibleTokenMetadata, FungibleTokenMetadataProvider, FT_METADATA_SPEC}, 
};
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LazyOption, LookupMap};
use near_sdk::json_types::U128;
use near_sdk::{
    assert_one_yocto, env, log, ext_contract, near_bindgen, AccountId, Balance,
    PanicOnDefault, PromiseOrValue,
};
mod internal;
mod migrations;
mod storage;
mod upgrade;
mod util;
mod vesting;

use util::*;
use vesting::{VestingRecord, VestingRecordJSON};

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    metadata: LazyOption<FungibleTokenMetadata>,

    pub accounts: LookupMap<AccountId, storage::AccBalance>,
    pub owner_id: AccountId,
    pub minters: Vec<AccountId>,
    pub total_supply: Balance,
    pub vested: LookupMap<AccountId, VestingRecord>,
}

#[near_bindgen]
impl Contract {
    /// Initializes the contract with the given total supply owned by the given `owner_id`.
    #[init]
    pub fn new(owner_id: AccountId) -> Self {
        let m = FungibleTokenMetadata {
            spec: FT_METADATA_SPEC.to_string(),
            name: "Cheddar".to_string(),
            symbol: "Cheddar".to_string(),
            icon: Some(String::from(
                r###"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 56 56"><style>.a{fill:#F4C647;}.b{fill:#EEAF4B;}</style><path d="M45 19.5v5.5l4.8 0.6 0-11.4c-0.1-3.2-11.2-6.7-24.9-6.7 -13.7 0-24.8 3.6-24.9 6.7L0 32.5c0 3.2 10.7 7.1 24.5 7.1 0.2 0 0.3 0 0.5 0V21.5l-4.7-7.2L45 19.5z" class="a"/><path d="M25 31.5v-10l-4.7-7.2L45 19.5v5.5l-14-1.5v10C31 33.5 25 31.5 25 31.5z" fill="#F9E295"/><path d="M24.9 7.5C11.1 7.5 0 11.1 0 14.3s10.7 7.2 24.5 7.2c0.2 0 0.3 0 0.5 0l-4.7-7.2 25 5.2c2.8-0.9 4.4-4 4.4-5.2C49.8 11.1 38.6 7.5 24.9 7.5z" class="b"/><path d="M36 29v19.6c8.3 0 15.6-1 20-2.5V26.5L31 23.2 36 29z" class="a"/><path d="M31 23.2l5 5.8c8.2 0 15.6-1 19.9-2.5L31 23.2z" class="b"/><polygon points="36 29 36 48.5 31 42.5 31 23.2 " fill="#FCDF76"/></svg>"###,
            )),
            reference: None,
            reference_hash: None,
            decimals: 24,
        };
        m.assert_valid();

        Self {
            owner_id: owner_id.clone(),
            metadata: LazyOption::new(b"m".to_vec(), Some(&m)),
            accounts: LookupMap::new(b"a".to_vec()),
            minters: vec![owner_id],
            total_supply: 0,
            vested: LookupMap::new(b"v".to_vec()),
        }
    }

    /// Returns account ID of the owner.
    pub fn get_owner_id(&self) -> AccountId {
        return self.owner_id.clone();
    }

    /// Mints new tokens to the `account_id`.
    /// Panics if the function is calle by a not registered minter.
    #[payable]
    pub fn ft_mint(&mut self, receiver_id: &AccountId, amount: U128String, memo: Option<String>) {
        assert!(
            env::attached_deposit() >= 1,
            "Requires attached deposit at least 1 yoctoNEAR"
        );
        log!(
            "Minting {} CHEDDAR to {}, memo: {}",
            amount.0,
            receiver_id,
            if let Some(m) = memo {
                m
            } else {
                "".to_string()
            }
        );
        self.assert_minter(env::predecessor_account_id());
        self.mint(receiver_id, amount.0);
    }

    /// burns `amount` from own supply of coins
    #[payable]
    pub fn self_burn(&mut self, amount: U128String) {
        assert_one_yocto();
        self.internal_burn(&env::predecessor_account_id(), amount.0);
    }

    //-----------
    //-- Admin
    //-----------

    /// owner can add/remove minters
    #[payable]
    pub fn add_minter(&mut self, account_id: AccountId) {
        assert_one_yocto();
        self.assert_owner();
        if let Some(_) = self.minters.iter().position(|x| *x == account_id) {
            //found
            panic!("already in the list");
        }
        self.minters.push(account_id);
    }

    #[payable]
    pub fn remove_minter(&mut self, account_id: &AccountId) {
        assert_one_yocto();
        self.assert_owner();
        if let Some(inx) = self.minters.iter().position(|x| x == account_id) {
            //found
            let _removed = self.minters.swap_remove(inx);
        } else {
            panic!("not a minter")
        }
    }

    pub fn get_minters(self) -> Vec<AccountId> {
        self.minters
    }

    #[payable]
    pub fn set_metadata_icon(&mut self, svg_string: String) {
        assert_one_yocto();
        self.assert_owner();
        let mut m = self.internal_get_ft_metadata();
        m.icon = Some(svg_string);
        self.metadata.set(&m);
    }

    #[payable]
    pub fn set_metadata_reference(&mut self, reference: String, reference_hash: String) {
        assert_one_yocto();
        self.assert_owner();
        let mut m = self.internal_get_ft_metadata();
        m.reference = Some(reference);
        m.reference_hash = Some(reference_hash.as_bytes().to_vec().into());
        m.assert_valid();
        self.metadata.set(&m);
    }

    pub fn set_owner(&mut self, owner_id: AccountId) {
        self.assert_owner();
        assert!(
            env::is_valid_account_id(owner_id.as_bytes()),
            "Account @{} is invalid!",
            owner_id.clone()
        );
        self.owner_id = owner_id.clone();
    }

    /// Get the owner of this account.
    pub fn get_owner(&self) -> AccountId {
        self.owner_id.clone()
    }

    //-----------
    //-- Vesting
    //-----------

    /// Get the amount of tokens that are locked in this account due to lockup or vesting.
    pub fn get_locked_amount(&self, account: AccountId) -> U128String {
        match self.vested.get(&account) {
            Some(vesting) => vesting.compute_amount_locked().into(),
            None => 0.into(),
        }
    }

    /// Get vesting information
    pub fn get_vesting_info(&self, account_id: AccountId) -> VestingRecordJSON {
        log!("{}", &account_id);
        let vesting = self.vested.get(&account_id).unwrap();
        VestingRecordJSON {
            amount: vesting.amount.into(),
            cliff_timestamp: vesting.cliff_timestamp.into(),
            end_timestamp: vesting.end_timestamp.into(),
        }
    }

    /// minters can mint with vesting/locked periods
    /// NOTE: we don't charge storage fees for vesting accounts.
    #[payable]
    pub fn mint_vested(
        &mut self,
        receiver_id: &AccountId,
        amount: U128String,
        cliff_timestamp: U64String,
        end_timestamp: U64String,
    ) {
        self.ft_mint(receiver_id, amount, Some("vesting".to_string()));
        let record =
            VestingRecord::new(amount.into(), cliff_timestamp.into(), end_timestamp.into());
        match self.vested.insert(&receiver_id, &record) {
            Some(_) => panic!("account already vested"),
            None => {}
        }
    }

    /// Cancels token allocation in a vesting account. All not vested tokens
    /// will be burned.
    /// Only owner can call this function.
    #[payable]
    pub fn cancel_vesting(&mut self, account_id: &AccountId) {
        assert_one_yocto();
        self.assert_owner();
        match self.vested.get(&account_id) {
            Some(vesting) => {
                if vesting.compute_amount_locked() == 0 {
                    panic!("past the cliff, vesting can't be changed")
                }
                self.internal_burn(account_id, vesting.amount);
                self.vested.remove(&account_id);
            }
            None => panic!("account not vested"),
        }
    }
}

#[near_bindgen]
impl FungibleTokenCore for Contract {
    #[payable]
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>) {
        assert_one_yocto();
        let sender_id = env::predecessor_account_id();
        let amount: Balance = amount.into();
        self.internal_transfer(&sender_id, &receiver_id, amount, memo);
    }

    #[payable]
    fn ft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        amount: U128,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<U128> {
        assert_one_yocto();
        let sender_id = env::predecessor_account_id();
        let amount: Balance = amount.into();
        self.internal_transfer(&sender_id, &receiver_id, amount, memo);
        // Initiating receiver's call and the callback
        // ext_ft calls like this was deprecated in v4.0.0 near-sdk-rs
        /*
        ext_ft_receiver::ft_on_transfer(
            sender_id.clone(),
            amount.into(),
            msg,
            receiver_id.clone(),
            NO_DEPOSIT,
            env::prepaid_gas() - GAS_FOR_FT_TRANSFER_CALL,
        )
        .then(ext_self::ft_resolve_transfer(
            sender_id,
            receiver_id,
            amount.into(),
            env::current_account_id(),
            NO_DEPOSIT,
            GAS_FOR_RESOLVE_TRANSFER,
        ))
        */
        ext_ft_receiver::ext(receiver_id.clone())
        .with_static_gas(env::prepaid_gas() - GAS_FOR_FT_TRANSFER_CALL)
        .ft_on_transfer(sender_id.clone(), amount.into(), msg)
        .then(
            ext_ft_resolver::ext(env::current_account_id())
                .with_static_gas(GAS_FOR_RESOLVE_TRANSFER)
                .ft_resolve_transfer(sender_id, receiver_id, amount.into()),
        )
        .into()
    }

    fn ft_total_supply(&self) -> U128 {
        self.total_supply.into()
    }

    fn ft_balance_of(&self, account_id: AccountId) -> U128 {
        self._balance_of(&account_id).into()
    }
}
#[ext_contract(ext_ft_receiver)]
pub trait FungibleTokenReceiver {
    /// Called by fungible token contract after `ft_transfer_call` was initiated by
    /// `sender_id` of the given `amount` with the transfer message given in `msg` field.
    /// The `amount` of tokens were already transferred to this contract account and ready to be used.
    ///
    /// The method must return the amount of tokens that are *not* used/accepted by this contract from the transferred
    /// amount. Examples:
    /// - The transferred amount was `500`, the contract completely takes it and must return `0`.
    /// - The transferred amount was `500`, but this transfer call only needs `450` for the action passed in the `msg`
    ///   field, then the method must return `50`.
    /// - The transferred amount was `500`, but the action in `msg` field has expired and the transfer must be
    ///   cancelled. The method must return `500` or panic.
    ///
    /// Arguments:
    /// - `sender_id` - the account ID that initiated the transfer.
    /// - `amount` - the amount of tokens that were transferred to this account in a decimal string representation.
    /// - `msg` - a string message that was passed with this transfer call.
    ///
    /// Returns the amount of unused tokens that should be returned to sender, in a decimal string representation.
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128>;
}

#[ext_contract(ext_ft_resolver)]
pub trait FungibleTokenResolver {
    /// Returns the amount of burned tokens in a corner case when the sender
    /// has deleted (unregistered) their account while the `ft_transfer_call` was still in flight.
    /// Returns (Used token amount, Burned token amount)
    fn ft_resolve_transfer(
        &mut self,
        sender_id: AccountId,
        receiver_id: AccountId,
        amount: U128,
    ) -> U128;
}

#[near_bindgen]
impl FungibleTokenMetadataProvider for Contract {
    fn ft_metadata(&self) -> FungibleTokenMetadata {
        self.internal_get_ft_metadata()
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::{testing_env, Balance};

    use super::*;

    const OWNER_SUPPLY: Balance = 1_000_000_000_000_000_000_000_000_000_000;

    fn get_context(predecessor_account_id: AccountId) -> VMContextBuilder {
        let mut builder = VMContextBuilder::new();
        builder
            .current_account_id(accounts(0))
            .signer_account_id(predecessor_account_id.clone())
            .predecessor_account_id(predecessor_account_id);
        builder
    }

    #[test]
    fn test_new() {
        let mut context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = Contract::new(accounts(1).into());

        testing_env!(context
            .attached_deposit(1)
            .predecessor_account_id(accounts(1))
            .build());
        contract.mint(&accounts(1), OWNER_SUPPLY.into());

        testing_env!(context.is_view(true).build());
        assert_eq!(contract.ft_total_supply().0, OWNER_SUPPLY);
        assert_eq!(contract.ft_balance_of(accounts(1)).0, OWNER_SUPPLY);
    }

    #[test]
    #[should_panic(expected = "The contract is not initialized")]
    fn test_default() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let _contract = Contract::default();
    }

    #[test]
    fn test_transfer() {
        let mut context = get_context(accounts(2));
        testing_env!(context.build());
        let mut contract = Contract::new(accounts(2).into());

        testing_env!(context
            .attached_deposit(1)
            .predecessor_account_id(accounts(2))
            .build());
        contract.mint(&accounts(2), OWNER_SUPPLY.into());

        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(1_000_000_000_000_000)
            .predecessor_account_id(accounts(1))
            .build());

        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(1)
            .predecessor_account_id(accounts(2))
            .build());
        let transfer_amount = OWNER_SUPPLY / 3;
        contract.ft_transfer(accounts(1), transfer_amount.into(), None);

        testing_env!(context
            .storage_usage(env::storage_usage())
            .account_balance(env::account_balance())
            .is_view(true)
            .attached_deposit(0)
            .build());
        assert_eq!(
            contract.ft_balance_of(accounts(2)).0,
            (OWNER_SUPPLY - transfer_amount)
        );
        assert_eq!(contract.ft_balance_of(accounts(1)).0, transfer_amount);
    }
}

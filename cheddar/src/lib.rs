// [AUDIT] NOTE: Storage cost is still an issue. An account with 125 bytes costs 0.00125 `NEAR`.
// I think the transaction to create it is cheaper than that, so it's possible to lock the contract
// due to the storage limitation.

/// Cheddar Token
///
/// Functionality:
/// - No account storage complexity - Since NEAR slashed storage price by 10x
/// it does not make sense to add that friction (storage backup per user).
/// Token creator must store enough NEAR in the contract to support growth.
/// - Multi-minters, no fixed total_supply:
/// The owner can add/remove allowed minters. This is useful if you want
/// an external contract, a farm for example, to be able to mint tokens
/// - Ultra-Lazy ft-metadata: ft-metadata is not stored unless changed
///
use near_sdk::collections::LookupMap;

use near_contract_standards::fungible_token::{
    core::FungibleTokenCore,
    metadata::{FungibleTokenMetadata, FungibleTokenMetadataProvider, FT_METADATA_SPEC},
    resolver::FungibleTokenResolver,
};

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::LazyOption;
use near_sdk::json_types::{ValidAccountId, U128};
use near_sdk::{
    assert_one_yocto, env, ext_contract, log, near_bindgen, AccountId, Balance, Gas,
    PanicOnDefault, PromiseOrValue,
};

const TGAS: Gas = 1_000_000_000_000;
const GAS_FOR_RESOLVE_TRANSFER: Gas = 5 * TGAS;
const GAS_FOR_FT_TRANSFER_CALL: Gas = 25 * TGAS + GAS_FOR_RESOLVE_TRANSFER;
const NO_DEPOSIT: Balance = 0;

near_sdk::setup_alloc!();

mod internal;
mod util;
mod vesting;

use util::*;
use vesting::{VestingRecord, VestingRecordJSON};

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    metadata: LazyOption<FungibleTokenMetadata>,

    pub accounts: LookupMap<AccountId, Balance>,

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
        //validate default metadata
        internal::default_ft_metadata().assert_valid();

        Self {
            owner_id: owner_id.clone(),
            metadata: LazyOption::new(b"m".to_vec(), None),
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

    /// minters can mint more
    #[payable]
    pub fn mint(&mut self, account_id: &AccountId, amount: U128String) {
        assert_one_yocto();
        env_log!("Minting {} CHEDDAR to {}", amount.0, account_id);
        self.assert_minter(env::predecessor_account_id());
        self.mint_into(account_id, amount.0);
    }

    /// burns `amount` from own supply of coins
    // AUDIT: Has to be payable
    #[payable]
    pub fn self_burn(&mut self, amount: U128String) {
        assert_one_yocto();
        self.internal_burn(&env::predecessor_account_id(), amount.0);
    }

    /// owner can add/remove minters
    #[payable]
    pub fn add_minter(&mut self, account_id: AccountId) {
        assert_one_yocto();
        self.assert_owner_calling();
        if let Some(_) = self.minters.iter().position(|x| *x == account_id) {
            //found
            panic!("already in the list");
        }
        self.minters.push(account_id);
    }

    #[payable]
    pub fn remove_minter(&mut self, account_id: &AccountId) {
        assert_one_yocto();
        self.assert_owner_calling();
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
        self.assert_owner_calling();
        let mut m = self.internal_get_ft_metadata();
        m.icon = Some(svg_string);
        self.metadata.set(&m);
    }

    #[payable]
    pub fn set_metadata_reference(&mut self, reference: String, reference_hash: String) {
        assert_one_yocto();
        self.assert_owner_calling();
        let mut m = self.internal_get_ft_metadata();
        m.reference = Some(reference);
        m.reference_hash = Some(reference_hash.as_bytes().to_vec().into());
        m.assert_valid();
        self.metadata.set(&m);
    }

    //-----------
    //-- Vesting
    //-----------

    /// Get the amount of tokens that are locked in this account due to lockup or vesting.
    pub fn get_locked_amount(&self) -> U128String {
        // [AUDIT] NOTE: View methods don't have `predecessor_account_id`.
        //    Just add an argument for the account ID.
        match self.vested.get(&env::predecessor_account_id()) {
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

    //minters can mint with vesting/locked periods
    #[payable]
    pub fn mint_vested(
        &mut self,
        account_id: &AccountId,
        amount: U128String,
        cliff_timestamp: U64String,
        end_timestamp: U64String,
    ) {
        self.mint(account_id, amount);
        let record =
            VestingRecord::new(amount.into(), cliff_timestamp.into(), end_timestamp.into());
        match self.vested.insert(&account_id, &record) {
            Some(_) => panic!("account already vested"),
            None => {}
        }
    }

    #[payable]
    /// terminate vesting before the cliff
    /// burn the tokens
    pub fn terminate_vesting(&mut self, account_id: &AccountId) {
        assert_one_yocto();
        self.assert_minter(env::predecessor_account_id());
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
    fn ft_transfer(&mut self, receiver_id: ValidAccountId, amount: U128, memo: Option<String>) {
        assert_one_yocto();
        let sender_id = env::predecessor_account_id();
        let amount: Balance = amount.into();
        self.internal_transfer(&sender_id, receiver_id.as_ref(), amount, memo);
    }

    #[payable]
    fn ft_transfer_call(
        &mut self,
        receiver_id: ValidAccountId,
        amount: U128,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<U128> {
        assert_one_yocto();
        let sender_id = env::predecessor_account_id();
        let amount: Balance = amount.into();
        self.internal_transfer(&sender_id, receiver_id.as_ref(), amount, memo);
        // Initiating receiver's call and the callback
        // ext_fungible_token_receiver::ft_on_transfer(
        ext_ft_receiver::ft_on_transfer(
            sender_id.clone(),
            amount.into(),
            msg,
            receiver_id.as_ref(),
            NO_DEPOSIT,
            env::prepaid_gas() - GAS_FOR_FT_TRANSFER_CALL,
        )
        .then(ext_self::ft_resolve_transfer(
            sender_id,
            receiver_id.into(),
            amount.into(),
            &env::current_account_id(),
            NO_DEPOSIT,
            GAS_FOR_RESOLVE_TRANSFER,
        ))
        .into()
    }

    fn ft_total_supply(&self) -> U128 {
        self.total_supply.into()
    }

    fn ft_balance_of(&self, account_id: ValidAccountId) -> U128 {
        self.accounts.get(account_id.as_ref()).unwrap_or(0).into()
    }
}

#[near_bindgen]
impl FungibleTokenResolver for Contract {
    /// Returns the amount of burned tokens in a corner case when the sender
    /// has deleted (unregistered) their account while the `ft_transfer_call` was still in flight.
    /// Returns (Used token amount, Burned token amount)
    #[private]
    fn ft_resolve_transfer(
        &mut self,
        sender_id: ValidAccountId,
        receiver_id: ValidAccountId,
        amount: U128,
    ) -> U128 {
        let sender_id: AccountId = sender_id.into();
        let (used_amount, burned_amount) =
            self.int_ft_resolve_transfer(&sender_id, receiver_id, amount);
        if burned_amount > 0 {
            log!("{} tokens burned", burned_amount);
        }
        return used_amount.into();
    }
}

#[near_bindgen]
impl FungibleTokenMetadataProvider for Contract {
    fn ft_metadata(&self) -> FungibleTokenMetadata {
        self.internal_get_ft_metadata()
    }
}

#[ext_contract(ext_ft_receiver)]
pub trait FungibleTokenReceiver {
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128>;
}

#[ext_contract(ext_self)]
trait FungibleTokenResolver {
    fn ft_resolve_transfer(
        &mut self,
        sender_id: AccountId,
        receiver_id: AccountId,
        amount: U128,
    ) -> U128;
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::MockedBlockchain;
    use near_sdk::{testing_env, Balance};

    use super::*;

    const OWNER_SUPPLY: Balance = 1_000_000_000_000_000_000_000_000_000_000;

    fn get_context(predecessor_account_id: ValidAccountId) -> VMContextBuilder {
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
        contract.mint(&accounts(1).to_string(), OWNER_SUPPLY.into());

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
        contract.mint(&accounts(2).to_string(), OWNER_SUPPLY.into());

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

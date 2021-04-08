use near_sdk::collections::LookupMap;

use near_contract_standards::fungible_token::{
    core::FungibleTokenCore,
    metadata::{FungibleTokenMetadata, FungibleTokenMetadataProvider, FT_METADATA_SPEC},
    resolver::FungibleTokenResolver,
};
use near_contract_standards::storage_management::{
    StorageBalance, StorageBalanceBounds, StorageManagement,
};
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::LazyOption;
use near_sdk::json_types::{ValidAccountId, U128};
use near_sdk::{
    assert_one_yocto, env, ext_contract, log, near_bindgen, AccountId, Balance, Gas,
    PanicOnDefault, Promise, PromiseOrValue, PromiseResult, StorageUsage,
};

const GAS_FOR_RESOLVE_TRANSFER: Gas = 5_000_000_000_000;
const GAS_FOR_FT_TRANSFER_CALL: Gas = 25_000_000_000_000 + GAS_FOR_RESOLVE_TRANSFER;
const NO_DEPOSIT: Balance = 0;

near_sdk::setup_alloc!();

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    metadata: LazyOption<FungibleTokenMetadata>,

    pub accounts: LookupMap<AccountId, Balance>,
    pub total_supply: Balance,
    // TODO: rename
    /// The storage size in bytes for one account.
    pub account_storage_usage: StorageUsage,
}

fn measure_account_storage(a: &mut LookupMap<AccountId, Balance>) -> StorageUsage {
    let initial_storage_usage = env::storage_usage();
    let tmp_account_id = "a".repeat(64);
    a.insert(&tmp_account_id, &0u128);
    let usage = env::storage_usage() - initial_storage_usage;
    a.remove(&tmp_account_id);
    return usage;
}

#[near_bindgen]
impl Contract {
    /// Initializes the contract with the given total supply owned by the given `owner_id`.
    #[init]
    pub fn new(owner_id: ValidAccountId, owner_supply: U128) -> Self {
        assert!(!env::state_exists(), "Already initialized");
        let m = FungibleTokenMetadata {
            spec: FT_METADATA_SPEC.to_string(),
            name: "Cheddar".to_string(),
            symbol: "CHDR".to_string(),
            icon: None,
            reference: None, // TODO
            reference_hash: None,
            decimals: 18,
        };
        m.assert_valid();
        let mut this = Self {
            metadata: LazyOption::new(b"m".to_vec(), Some(&m)),
            accounts: LookupMap::new(b"a".to_vec()),
            total_supply: 0,
            account_storage_usage: 0,
        };
        this.account_storage_usage = measure_account_storage(&mut this.accounts);
        this.internal_register_account(owner_id.as_ref());
        this.internal_deposit(owner_id.as_ref(), owner_supply.into());
        this
    }

    pub fn internal_unwrap_balance_of(&self, account_id: &AccountId) -> Balance {
        match self.accounts.get(&account_id) {
            Some(balance) => balance,
            None => env::panic(format!("The account {} is not registered", &account_id).as_bytes()),
        }
    }

    pub fn internal_deposit(&mut self, account_id: &AccountId, amount: Balance) {
        let balance = self.internal_unwrap_balance_of(account_id);
        if let Some(new_balance) = balance.checked_add(amount) {
            self.accounts.insert(&account_id, &new_balance);
            self.total_supply = self
                .total_supply
                .checked_add(amount)
                .expect("Total supply overflow");
        } else {
            env::panic(b"Balance overflow");
        }
    }

    pub fn internal_withdraw(&mut self, account_id: &AccountId, amount: Balance) {
        let balance = self.internal_unwrap_balance_of(account_id);
        if let Some(new_balance) = balance.checked_sub(amount) {
            self.accounts.insert(&account_id, &new_balance);
            self.total_supply = self
                .total_supply
                .checked_sub(amount)
                .expect("Total supply overflow");
        } else {
            env::panic(b"The account doesn't have enough balance");
        }
    }

    pub fn internal_transfer(
        &mut self,
        sender_id: &AccountId,
        receiver_id: &AccountId,
        amount: Balance,
        memo: Option<String>,
    ) {
        assert_ne!(
            sender_id, receiver_id,
            "Sender and receiver should be different"
        );
        assert!(amount > 0, "The amount should be a positive number");
        self.internal_withdraw(sender_id, amount);
        self.internal_deposit(receiver_id, amount);
        log!("Transfer {} from {} to {}", amount, sender_id, receiver_id);
        if let Some(memo) = memo {
            log!("Memo: {}", memo);
        }
    }

    pub fn internal_register_account(&mut self, account_id: &AccountId) {
        if self.accounts.insert(&account_id, &0).is_some() {
            env::panic(b"The account is already registered");
        }
    }

    // TODO rename
    fn int_ft_resolve_transfer(
        &mut self,
        sender_id: &AccountId,
        receiver_id: ValidAccountId,
        amount: U128,
    ) -> (u128, u128) {
        let sender_id: AccountId = sender_id.into();
        let receiver_id: AccountId = receiver_id.into();
        let amount: Balance = amount.into();

        // Get the unused amount from the `ft_on_transfer` call result.
        let unused_amount = match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Successful(value) => {
                if let Ok(unused_amount) = near_sdk::serde_json::from_slice::<U128>(&value) {
                    std::cmp::min(amount, unused_amount.0)
                } else {
                    amount
                }
            }
            PromiseResult::Failed => amount,
        };

        if unused_amount > 0 {
            let receiver_balance = self.accounts.get(&receiver_id).unwrap_or(0);
            if receiver_balance > 0 {
                let refund_amount = std::cmp::min(receiver_balance, unused_amount);
                self.accounts
                    .insert(&receiver_id, &(receiver_balance - refund_amount));

                if let Some(sender_balance) = self.accounts.get(&sender_id) {
                    self.accounts
                        .insert(&sender_id, &(sender_balance + refund_amount));
                    log!(
                        "Refund {} from {} to {}",
                        refund_amount,
                        receiver_id,
                        sender_id
                    );
                    return (amount - refund_amount, 0);
                } else {
                    // Sender's account was deleted, so we need to burn tokens.
                    self.total_supply -= refund_amount;
                    log!("The account of the sender was deleted");
                    return (amount, refund_amount);
                }
            }
        }
        (amount, 0)
    }

    fn internal_storage_balance_of(&self, account_id: &AccountId) -> Option<StorageBalance> {
        if self.accounts.contains_key(account_id) {
            Some(StorageBalance {
                total: self.storage_balance_bounds().min,
                available: 0.into(),
            })
        } else {
            None
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
        used_amount.into()
    }
}

#[near_bindgen]
impl FungibleTokenMetadataProvider for Contract {
    fn ft_metadata(&self) -> FungibleTokenMetadata {
        self.metadata.get().unwrap()
    }
}

#[near_bindgen]
impl StorageManagement for Contract {
    // `registration_only` doesn't affect the implementation for vanilla fungible token.
    #[allow(unused_variables)]
    #[payable]
    fn storage_deposit(
        &mut self,
        account_id: Option<ValidAccountId>,
        registration_only: Option<bool>,
    ) -> StorageBalance {
        let amount: Balance = env::attached_deposit();
        let account_id = account_id
            .map(|a| a.into())
            .unwrap_or_else(|| env::predecessor_account_id());
        if self.accounts.contains_key(&account_id) {
            log!("The account is already registered, refunding the deposit");
            if amount > 0 {
                Promise::new(env::predecessor_account_id()).transfer(amount);
            }
        } else {
            let min_balance = self.storage_balance_bounds().min.0;
            if amount < min_balance {
                env::panic(b"The attached deposit is less than the minimum storage balance");
            }

            self.internal_register_account(&account_id);
            let refund = amount - min_balance;
            if refund > 0 {
                Promise::new(env::predecessor_account_id()).transfer(refund);
            }
        }
        self.internal_storage_balance_of(&account_id).unwrap()
    }

    /// While storage_withdraw normally allows the caller to retrieve `available` balance, the basic
    /// Fungible Token implementation sets storage_balance_bounds.min == storage_balance_bounds.max,
    /// which means available balance will always be 0. So this implementation:
    /// * panics if `amount > 0`
    /// * never transfers Ⓝ to caller
    /// * returns a `storage_balance` struct if `amount` is 0
    #[payable]
    fn storage_withdraw(&mut self, amount: Option<U128>) -> StorageBalance {
        assert_one_yocto();
        let predecessor_account_id = env::predecessor_account_id();
        if let Some(storage_balance) = self.internal_storage_balance_of(&predecessor_account_id) {
            match amount {
                Some(amount) if amount.0 > 0 => {
                    env::panic(b"The amount is greater than the available storage balance");
                }
                _ => storage_balance,
            }
        } else {
            env::panic(
                format!("The account {} is not registered", &predecessor_account_id).as_bytes(),
            );
        }
    }

    #[payable]
    fn storage_unregister(&mut self, force: Option<bool>) -> bool {
        assert_one_yocto();
        let account_id = env::predecessor_account_id();
        let force = force.unwrap_or(false);
        if let Some(balance) = self.accounts.get(&account_id) {
            if balance != 0 && !force {
                env::panic(b"Can't unregister the account with the positive balance without force")
            }
            self.accounts.remove(&account_id);
            self.total_supply -= balance;
            Promise::new(account_id.clone()).transfer(self.storage_balance_bounds().min.0 + 1);
            // TODO on_account_closed
            return true;
        } else {
            log!("The account {} is not registered", &account_id);
            return false;
        }
    }

    fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        let required_storage_balance =
            Balance::from(self.account_storage_usage) * env::storage_byte_cost();
        StorageBalanceBounds {
            min: required_storage_balance.into(),
            max: Some(required_storage_balance.into()),
        }
    }

    fn storage_balance_of(&self, account_id: ValidAccountId) -> Option<StorageBalance> {
        self.internal_storage_balance_of(&account_id.into())
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

    const OWNER_SUPPLY: Balance = 1_000_000_000_000_000;

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
        let contract = Contract::new(accounts(1).into(), OWNER_SUPPLY.into());
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
        let mut contract = Contract::new(accounts(2).into(), OWNER_SUPPLY.into());
        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(contract.storage_balance_bounds().min.into())
            .predecessor_account_id(accounts(1))
            .build());
        // Paying for account registration, aka storage deposit
        contract.storage_deposit(None, None);

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

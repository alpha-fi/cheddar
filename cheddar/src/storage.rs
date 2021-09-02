use crate::Contract;
use near_contract_standards::storage_management::{
    StorageBalance, StorageBalanceBounds, StorageManagement,
};
use near_sdk::json_types::{ValidAccountId, U128};
use near_sdk::{assert_one_yocto, env, log, AccountId, Balance, Promise};

// The storage size in bytes for one account.
// 16 (u128) + 64 (acc id)
const ACCOUNT_STORAGE: u128 = 16 + 64; // must be same as above

impl Contract {
    /// Registers an account and panics if the account was already registered.
    fn register_account(&mut self, account_id: &AccountId) {
        if self.accounts.insert(account_id, &0).is_some() {
            env::panic("The account is already registered".as_bytes());
        }
    }

    /// It's like `register_account` but doesn't panic if the account already exists.
    #[inline]
    fn try_register_account(&mut self, account_id: &AccountId) {
        if !self.accounts.contains_key(account_id) {
            self.accounts.insert(account_id, &0);
        }
    }

    /// Internal method that returns the Account ID and the balance in case the account was
    /// registered.
    fn internal_storage_unregister(&mut self, force: Option<bool>) -> Option<(AccountId, Balance)> {
        assert_one_yocto();
        let account_id = env::predecessor_account_id();
        let force = force.unwrap_or(false);
        if let Some(balance) = self.accounts.get(&account_id) {
            if balance == 0 || force {
                self.accounts.remove(&account_id);
                if balance != 0 {
                    self.total_supply -= balance;
                    // we add 1 because the function requires 1 yocto payment
                    Promise::new(account_id.clone()).transfer(storage_deposit() + 1);
                }
                Some((account_id, balance))
            } else {
                env::panic(
                    "Can't unregister the account with the positive balance without force"
                        .as_bytes(),
                )
            }
        } else {
            log!("The account {} is not registered", &account_id);
            None
        }
    }
}

// We implement the NEP-145 standard for the Cheddar. However user can't make additional
// deposits. User registres an account by attaching `storage_deposit()` of NEAR. Deposits above
// that amount will be refunded.
// NOTE: when using farming / minting we will register an account for the user for free.
impl StorageManagement for Contract {
    /// Registers an account and records the deposit.
    /// `registration_only` doesn't affect the implementation for vanilla fungible token.
    #[allow(unused_variables)]
    fn storage_deposit(
        &mut self,
        account_id: Option<ValidAccountId>,
        registration_only: Option<bool>,
    ) -> StorageBalance {
        let amount: Balance = env::attached_deposit();
        let account_id: AccountId = if let Some(a) = account_id {
            a.into()
        } else {
            env::predecessor_account_id()
        };
        if self.accounts.contains_key(&account_id) {
            log!("The account is already registered, refunding the deposit");
            if amount > 0 {
                Promise::new(env::predecessor_account_id()).transfer(amount);
            }
        } else {
            let d = storage_deposit();
            if amount < d {
                env::panic(
                    "The attached deposit is less than the minimum storage balance".as_bytes(),
                );
            }
            self.register_account(&account_id);
            let refund = amount - d;
            if refund > 0 {
                Promise::new(env::predecessor_account_id()).transfer(refund);
            }
        }
        return storage_balance();
    }

    /// While storage_withdraw normally allows the caller to retrieve `available` balance, the basic
    /// Fungible Token implementation sets storage_balance_bounds.min == storage_balance_bounds.max,
    /// which means available balance will always be 0. So this implementation:
    /// * panics if `amount > 0`
    /// * never transfers Ⓝ to caller
    /// * returns a `storage_balance` struct if `amount` is 0
    fn storage_withdraw(&mut self, amount: Option<U128>) -> StorageBalance {
        assert_one_yocto();
        let predecessor_account_id = env::predecessor_account_id();
        if self.accounts.contains_key(&predecessor_account_id) {
            match amount {
                Some(amount) if amount.0 > 0 => {
                    env::panic(
                        "The amount is greater than the available storage balance".as_bytes(),
                    );
                }
                _ => storage_balance(),
            }
        } else {
            env::panic(
                format!("The account {} is not registered", &predecessor_account_id).as_bytes(),
            );
        }
    }

    fn storage_unregister(&mut self, force: Option<bool>) -> bool {
        self.internal_storage_unregister(force).is_some()
    }

    fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        let d = U128::from(storage_deposit());
        StorageBalanceBounds {
            min: d,
            max: Some(d),
        }
    }

    fn storage_balance_of(&self, account_id: ValidAccountId) -> Option<StorageBalance> {
        if self.accounts.contains_key(&account_id.into()) {
            Some(storage_balance())
        } else {
            None
        }
    }
}

fn storage_balance() -> StorageBalance {
    StorageBalance {
        total: U128::from(storage_deposit()),
        available: 0.into(),
    }
}

fn storage_deposit() -> u128 {
    ACCOUNT_STORAGE * env::storage_byte_cost()
}

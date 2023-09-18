use crate::*;
use near_contract_standards::storage_management::{
    StorageBalance, StorageBalanceBounds, StorageManagement,
};

#[near_bindgen]
impl StorageManagement for Contract {
    /// Registers a new account
    #[allow(unused_variables)]
    #[payable]
    fn storage_deposit(
        &mut self,
        account_id: Option<AccountId>,
        registration_only: Option<bool>,
    ) -> StorageBalance {
        let amount: Balance = env::attached_deposit();
        let account_id = account_id
            .map(|a| a.into())
            .unwrap_or_else(|| env::predecessor_account_id());

        if self.vaults.contains_key(&account_id) {
            log!("The account is already registered, refunding the deposit");
            if amount > 0 {
                Promise::new(env::predecessor_account_id()).transfer(amount);
            }
        } else {
            assert!(
                amount >= STORAGE_COST,
                "The attached deposit is less than the minimum storage balance ({})",
                STORAGE_COST
            );
            self.create_account(&account_id);

            let refund = amount - STORAGE_COST;
            if refund > 0 {
                Promise::new(env::predecessor_account_id()).transfer(refund);
            }
        }
        storage_balance()
    }

    /// Method not supported. Close the account (`close()` or
    /// `storage_unregister(true)`) to close the account and withdraw deposited NEAR.
    #[allow(unused_variables)]
    fn storage_withdraw(&mut self, amount: Option<U128>) -> StorageBalance {
        panic!("Storage withdraw not possible, close the account instead");
    }

    /// When force == true it will close the account. Otherwise this is noop.
    #[payable]
    fn storage_unregister(&mut self, force: Option<bool>) -> bool {
        assert_one_yocto();
        if Some(true) == force {
            self.close();
            return true;
        }
        false
    }

    /// Mix and min balance is always MIN_BALANCE.
    fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        StorageBalanceBounds {
            min: STORAGE_COST.into(),
            max: Some(STORAGE_COST.into()),
        }
    }

    /// If the account is registered the total and available balance is always MIN_BALANCE.
    /// Otherwise None.
    fn storage_balance_of(&self, account_id: AccountId) -> Option<StorageBalance> {
        let account_id: AccountId = account_id.into();
        if self.vaults.contains_key(&account_id) {
            return Some(storage_balance());
        }
        None
    }
}

fn storage_balance() -> StorageBalance {
    StorageBalance {
        total: STORAGE_COST.into(),
        available: U128::from(0),
    }
}

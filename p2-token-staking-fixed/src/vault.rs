//! Vault is information per user about their balances in the exchange.

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::{ValidAccountId, U128};
use near_sdk::{env, log, AccountId, Balance, PromiseOrValue};

use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_contract_standards::storage_management::{
    StorageBalance, StorageBalanceBounds, StorageManagement,
};

// use crate::constants::*;
// use crate::errors::*;
// use crate::util::*;
use crate::*;

#[derive(BorshSerialize, BorshDeserialize, Default)]
#[cfg_attr(feature = "test", derive(Default, Clone))]
pub struct Vault {
    // round when the last time ping was called
    pub previous: usize,
    /// amount of $near locked in this vault
    pub staked: Balance,
    /// Amount of accumulated rewards from staking;
    pub rewards: Balance,
}

impl Contract {
    pub(crate) fn get_vault(&self) -> (AccountId, Vault) {
        let a = env::predecessor_account_id();
        let v = self.vaults.get(&a).expect(ERR10_NO_ACCOUNT);
        (a, v)
    }
    /**
    Update rewards for locked tokens in past epochs
    returns total account rewards
     */
    pub(crate) fn ping(&self, v: &mut Vault) -> u128 {
        // note: the round counting stops at self.farming_end
        let r = self.current_round();
        // if farming doesn't started, ignore the rewards update
        if r == 0 {
            return 0;
        }
        let rate = big(self.rate) * big(v.staked);
        while v.previous < r - 1 {
            // we don't farm in the current round
            let farmed = (rate / self.rounds[v.previous]).as_u128();
            v.rewards += farmed;
            println!(
                "FARMING {},  round_staked={}, user={}",
                farmed, self.rounds[v.previous], v.staked
            );
            v.previous += 1;
        }
        return v.rewards;
    }

    pub(crate) fn _stake(&mut self, amount: Balance, v: &mut Vault) {
        let r = self.current_round();
        // note: r==1 at start
        self.rounds[r - 1] += amount;
        self.ping(v);
        v.staked += amount;
    }

    pub(crate) fn _unstake(&mut self, amount: Balance, v: &mut Vault) {
        assert!(v.staked >= amount, "{}", ERR30_NOT_ENOUGH_STAKE);
        let r = self.current_round();
        // note: r==1 at start
        self.rounds[r - 1] -= amount;
        self.ping(v);
        v.staked -= amount;
    }
}

// token deposits are done through NEP-141 ft_transfer_call to the NEARswap contract.
#[near_bindgen]
impl FungibleTokenReceiver for Contract {
    /**
    FungibleTokenReceiver implementation
    Callback on receiving tokens by this contract.
    Automatically stakes receiving tokens.
    Returns zero.
    Panics when account is not registered or when receiving wrong token. */
    #[allow(unused_variables)]
    fn ft_on_transfer(
        &mut self,
        sender_id: ValidAccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        let token = env::predecessor_account_id();
        assert!(token == self.token_id, "Token not accepted");
        let sender_id = AccountId::from(sender_id);
        let mut v = self.vaults.get(&sender_id).expect(ERR10_NO_ACCOUNT);

        let amount = amount.0;
        // TODO: make sure we account it correctly
        self._stake(amount, &mut v);

        self.vaults.insert(&sender_id, &v);
        env_log!("Staked, {} {}", amount, token);

        return PromiseOrValue::Value(U128(0));
    }
}

#[near_bindgen]
impl StorageManagement for Contract {
    /// Registers a new account
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
        if self.vaults.contains_key(&account_id) {
            log!("The account is already registered, refunding the deposit");
            if amount > 0 {
                Promise::new(env::predecessor_account_id()).transfer(amount);
            }
        } else {
            assert!(
                amount >= MIN_BALANCE,
                "{}",
                "The attached deposit is less than the minimum storage balance"
            );
            self.create_account(&account_id, 0);

            let refund = amount - MIN_BALANCE;
            if refund > 0 {
                Promise::new(env::predecessor_account_id()).transfer(refund);
            }
        }
        storage_balance()
    }

    /// Close the account (`close()` or `storage_unregister(true)`) to close the account and
    /// withdraw deposited NEAR.
    #[allow(unused_variables)]
    fn storage_withdraw(&mut self, amount: Option<U128>) -> StorageBalance {
        panic!("Storage withdraw not possible, close the account instead");
    }

    /// When force == true to close the account. Otherwise this is noop.
    fn storage_unregister(&mut self, force: Option<bool>) -> bool {
        if Some(true) == force {
            self.close();
            return true;
        }
        false
    }

    /// Mix and min balance is always MIN_BALANCE.
    fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        StorageBalanceBounds {
            min: MIN_BALANCE.into(),
            max: Some(MIN_BALANCE.into()),
        }
    }

    /// If the account is registered the total and available balance is always MIN_BALANCE.
    /// Otherwise None.
    fn storage_balance_of(&self, account_id: ValidAccountId) -> Option<StorageBalance> {
        let account_id: AccountId = account_id.into();
        if self.vaults.contains_key(&account_id) {
            return Some(storage_balance());
        }
        None
    }
}

fn storage_balance() -> StorageBalance {
    StorageBalance {
        total: MIN_BALANCE.into(),
        available: U128::from(0),
    }
}

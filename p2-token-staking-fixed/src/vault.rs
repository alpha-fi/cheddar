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

#[derive(BorshSerialize, BorshDeserialize)]
#[cfg_attr(feature = "test", derive(Default, Clone))]
pub struct Vault {
    /// Contract.s value when the last ping was called and rewards calculated
    pub reward_acc: Balance,
    /// amount of staking token locked in this vault
    pub staked: Balance,
    /// Amount of accumulated, not withdrawn rewards from staking;
    pub farmed: Balance,
}

impl Vault {
    /**
    Update rewards for locked tokens in past epochs
    returns total account rewards
    Arguments:
    `s`: Contract.s value
    `round`: current round
     */
    pub fn ping(&mut self, reward_acc: u128, round: u64) -> u128 {
        // note: the round counting stops at self.farming_end
        // if farming didn't start, ignore the rewards update
        if round == 0 {
            return 0;
        }
        // ping in the same round
        if self.reward_acc == reward_acc {
            return self.farmed;
        }
        let farmed;
        // using a new farm iteration. Reset.
        // TODO: on restart we could remember the previous state
        if self.reward_acc > reward_acc {
            farmed = 0;
        } else {
            farmed = self.staked * (reward_acc - self.reward_acc) / ACC_OVERFLOW;
        }

        self.farmed += farmed;
        self.reward_acc = reward_acc;
        return self.farmed;
    }
}

impl Contract {
    /// Returns the registered vault.
    /// Panics if the account is not registered.
    #[inline]
    pub(crate) fn get_vault(&self, account_id: &AccountId) -> Vault {
        self.vaults.get(account_id).expect(ERR10_NO_ACCOUNT)
    }

    pub(crate) fn ping_all(&mut self, v: &mut Vault) -> u128 {
        let r = self.current_round();
        self.update_reward_acc(r);
        v.ping(self.reward_acc, r)
    }

    /// updates the rewards accumulator
    pub(crate) fn update_reward_acc(&mut self, round: u64) {
        let new_s = self.compute_reward_acc(round);
        // we should advance with rounds if self.t is zero, otherwise we have a jump and
        // don't compute properly the accumulator.
        if self.total_stake == 0 || new_s != self.reward_acc {
            self.reward_acc = new_s;
            self.reward_acc_round = round;
        }
    }

    /// computes the rewards accumulator.
    /// NOTE: the current, optimized algorithm will not farm anything if
    ///   `self.rate * ACC_OVERFLOW / self.t < 1`
    pub(crate) fn compute_reward_acc(&self, round: u64) -> u128 {
        // covers also when round == 0
        if self.reward_acc_round == round || self.total_stake == 0 {
            return self.reward_acc;
        }

        self.reward_acc
            + u128::from(round - self.reward_acc_round) * self.rate * ACC_OVERFLOW
                / self.total_stake
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
    Panics when account is not registered or when receiving a wrong token. */
    #[allow(unused_variables)]
    fn ft_on_transfer(
        &mut self,
        sender_id: ValidAccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        self.assert_is_active();
        let token = env::predecessor_account_id();
        assert!(
            token == self.staking_token,
            "Only {} token transfers are accepted",
            self.staking_token
        );
        assert!(amount.0 > 0, "staked amount must be positive");
        let sender_id: &AccountId = sender_id.as_ref();
        let mut v = self.get_vault(sender_id);

        // firstly update the past rewards
        self.ping_all(&mut v);

        log!("Staked, {} {}", amount.0, token);
        v.staked += amount.0;
        self.vaults.insert(sender_id, &v);
        self.total_stake += amount.0; // must be called after ping_s

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
                amount >= STORAGE_COST,
                "The attached deposit is less than the minimum storage balance ({})",
                STORAGE_COST
            );
            self.create_account(&account_id, 0);

            let refund = amount - STORAGE_COST;
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

    /// When force == true it will close the account. Otherwise this is noop.
    #[payable]
    fn storage_unregister(&mut self, force: Option<bool>) -> bool {
        assert_one_yocto();
        self.assert_is_active();
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
        total: STORAGE_COST.into(),
        available: U128::from(0),
    }
}

// use uint::construct_uint;
// construct_uint! {
//     /// 256-bit unsigned integer.
//     struct U256(4);
// }

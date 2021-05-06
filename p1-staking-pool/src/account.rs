//! Account deposit is information per user about their balances in the exchange.

use std::collections::HashMap;
use std::convert::TryInto;

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::{ValidAccountId, U128};
use near_sdk::{
    assert_one_yocto, env, near_bindgen, AccountId, Balance, PromiseResult, StorageUsage,
};

/// Account deposits information and storage cost.
#[derive(BorshSerialize, BorshDeserialize)]
#[cfg_attr(feature = "test", derive(Clone))]
pub struct Account {
    /// Cheddar minted in the LockPool
    pub cheddar_amount: Balance,
    /// NEAR deposited.Used for storage and staking.
    pub near_amount: Balance,
    ///// Amounts of other tokens.
    // pub tokens: HashMap<AccountId, Balance>,
    // pub storage_used: StorageUsage,
}

impl Contract {
    /// Registers account in deposited amounts with given amount of $NEAR.
    /// If account already exists, adds amount to it.
    /// This should be used when it's known that storage is prepaid.
    pub(crate) fn register_account(&mut self, account_id: &AccountId, amount: Balance) {
        let d = if let Some(mut d) = self.deposits.get(&account_id) {
            d.near_amount += amount;
            d
        } else {
            Deposit {
                cheddar_amount: 0,
                near_amount: amount,
            }
        };
        self.deposits.insert(&account_id, &d);
    }

    // Returns `account` Account.
    #[inline]
    pub(crate) fn get_account(&self, account: &AccountId) -> Account {
        self.deposits.get(account).expect(ERR10_ACC_NOT_REGISTERED)
    }

    pub(crate) fn get_vault(&self, account: &AccountId) -> Vault {
        self.vaults.get(account).expect(ERR10_ACC_NOT_REGISTERED)
    }
}

#[derive(BorshSerialize, BorshDeserialize, Default)]
#[cfg_attr(feature = "test", derive(Clone))]
pub struct Vault {
    pub epoch: u64,
    /// amount of $near locked in the current epoch
    pub locked: Balance,
    /// amount of $near locked for the next epoch
    pub locked_new: Balance,

    // TODO: decide if we want to have a fixed rate rewards (fixed rate per $near staked)
    // or have a total amount of cheddar to distribute for each epoch
    /// Amount of accumulated rewards from staking;
    rewards: Balance,
}

impl Valut {
    /**
    Update state and rewards for locked tokens in past epochs
    Parameters:
    * `epoch`: beginning of current epoch
    * `reward_rate`: amount of $CHEDDAR received for 1,000,000 $NEAR staked.
    */
    pub(crate) fn ping(&mut self, epoch: u64, reward_rate: u128) {
        while self.epoch < epoch {
            self.epoch += EPOCH_DURATION;
            self.rewards += self.locked * reward_rate / 1_000_000;
            self.locked += self.locked_new;
            self.locked_new = 0;
        }
    }

    pub(crate) fn stake(&mut self, amount: Balance, epoch: u64) {
        self.ping(epoch);
        self.locked_new += amount;
    }

    pub(crate) fn unstake(&mut self, amount: Balance, epoch: u64) {
        self.ping(epoch);
        self.locked_new += amount;

        if self.locked_new > amount {
            self.locked_new -= amount;
            return;
        }
        amount -= self.locked_new;
        self.locked_new = 0;
        assert(self.locked >= amount, ERR30_NOT_ENOUGH_STAKE);
        self.locked -= amount;
    }
}

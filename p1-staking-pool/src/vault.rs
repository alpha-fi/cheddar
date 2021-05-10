//! Account deposit is information per user about their balances in the exchange.

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::{AccountId, Balance};

use crate::*;

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
    pub rewards: Balance,
}

impl Vault {
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

    pub(crate) fn stake(&mut self, amount: Balance, epoch: u64, reward_rate: u128) {
        self.ping(epoch, reward_rate);
        self.locked_new += amount;
    }

    pub(crate) fn unstake(&mut self, mut amount: Balance, epoch: u64, reward_rate: u128) {
        self.ping(epoch, reward_rate);

        if self.locked_new > amount {
            self.locked_new -= amount;
            return;
        }
        amount -= self.locked_new;
        self.locked_new = 0;
        assert!(self.locked >= amount, "{}", ERR30_NOT_ENOUGH_STAKE);
        self.locked -= amount;
    }

    /// Returns amount of deposited NEAR.
    #[inline]
    pub fn total(&self) -> Balance {
        return self.locked + self.locked_new;
    }
}

impl Contract {
    pub fn get_vault(&self) -> (AccountId, Vault) {
        let a = env::predecessor_account_id();
        let v = self.vaults.get(&a).expect(ERR10_NO_ACCOUNT);
        (a, v)
    }
}

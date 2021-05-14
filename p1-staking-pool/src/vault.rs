//! Account deposit is information per user about their balances in the exchange.

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::{AccountId, Balance};

use crate::*;
use util::U256;

#[derive(BorshSerialize, BorshDeserialize, Default)]
#[cfg_attr(feature = "test", derive(Clone))]
pub struct Vault {
    // env::block_timestamp for the last time ping was called
    pub previous: u64,
    /// amount of $near locked in this vault
    pub staked: Balance,
    /// Amount of accumulated rewards from staking;
    pub rewards: Balance,
}

impl Vault {
    /**
    Update rewards for locked tokens in past epochs
    returns total rewards
    */
    pub(crate) fn ping(&mut self, rewards_per_hour: u32) -> u128 {
        if self.previous != 0 {
            let current = env::block_timestamp(); //nanoseconds
            assert!(current >= self.previous);
            let delta_seconds = (current - self.previous) / NANO;
            if delta_seconds>0 {
                self.rewards += (U256::from(delta_seconds) * U256::from(rewards_per_hour) * U256::from(self.staked) / U256::from(SECONDS_PER_HOUR)).as_u128();
                self.previous = current;
            }
        }
        self.rewards
    }

    pub(crate) fn stake(&mut self, amount: Balance, rewards_per_hour: u32) {
        self.ping(rewards_per_hour);
        self.staked += amount;
    }

    pub(crate) fn unstake(&mut self, amount: Balance, rewards_per_hour: u32) {
        assert!(self.staked >= amount, "{}", ERR30_NOT_ENOUGH_STAKE);
        self.ping(rewards_per_hour);
        self.staked -= amount;
    }
}

impl Contract {
    pub fn get_vault(&self) -> (AccountId, Vault) {
        let a = env::predecessor_account_id();
        let v = self.vaults.get(&a).expect(ERR10_NO_ACCOUNT);
        (a, v)
    }
}

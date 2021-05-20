//! Account deposit is information per user about their balances in the exchange.

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::Balance;

use crate::errors::*;
use crate::util::*;

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
    pub(crate) fn ping(&mut self, emission_rate: u128, total_stake: u128) -> u128 {
        assert!(
            self.previous != 0,
            "Previously registered epoch can't be zero"
        );
        let now = current_epoch();
        // TODO: add start epoch control.

        let delta = now - self.previous;
        if delta > 0 {
            self.rewards +=
                (U256::from(delta) * U256::from(emission_rate) * U256::from(self.staked)
                    / U256::from(total_stake))
                .as_u128();
            self.previous = now;
        }
        self.rewards
    }

    pub(crate) fn stake(&mut self, amount: Balance, emission_rate: u128, total_stake: u128) {
        self.ping(emission_rate, total_stake);
        self.staked += amount;
    }

    pub(crate) fn unstake(&mut self, amount: Balance, emission_rate: u128, total_stake: u128) {
        assert!(self.staked >= amount, "{}", ERR30_NOT_ENOUGH_STAKE);
        self.ping(emission_rate, total_stake);
        self.staked -= amount;
    }
}

//! Account deposit is information per user about their balances in the exchange.

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::{env, AccountId, Balance};

use crate::constants::*;
use crate::errors::*;
use crate::util::*;
use crate::Contract;

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
}

impl Contract {
    pub(crate) fn get_vault(&self) -> (AccountId, Vault) {
        let a = env::predecessor_account_id();
        let v = self.vaults.get(&a).expect(ERR10_NO_ACCOUNT);
        (a, v)
    }

    pub(crate) fn ping(&mut self, v: &mut Vault) {
        assert!(
            v.previous != 0,
            "Wrong state. Previously registered epoch can't be zero"
        );
        let now = current_epoch();
        // if farming doesn't started, ignore the rewards update
        if now < self.farming_end {
            return;
        }
        assert!(
            now <= self.farming_end,
            "Farming only possible between farming_start and farming_end epoch"
        );
        let delta = now - v.previous;
        if delta > 0 {
            v.rewards +=
                (U256::from(delta) * U256::from(self.emission_rate) * U256::from(v.staked)
                    / U256::from(self.total_stake))
                .as_u128();
            v.previous = now;
        }
    }

    pub(crate) fn _stake(&mut self, amount: Balance, v: &mut Vault) {
        self.ping(v);
        v.staked += amount;
    }

    pub(crate) fn _unstake(&mut self, amount: Balance, v: &mut Vault) {
        assert!(v.staked >= amount, "{}", ERR30_NOT_ENOUGH_STAKE);
        self.ping(v);
        v.staked -= amount;

        assert!(v.staked >= MIN_STAKE, "{}", ERR02_MIN_BALANCE);
    }
}

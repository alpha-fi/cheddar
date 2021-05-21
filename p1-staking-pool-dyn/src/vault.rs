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
    // epoch when the last time ping was called
    pub previous: u64,
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
        assert!(
            v.previous != 0,
            "Wrong state. Previously registered epoch can't be zero"
        );
        let mut now = current_round();
        // if farming doesn't started, ignore the rewards update
        if now < self.farming_start {
            return 0;
        }
        if now >= self.farming_end {
            now = self.farming_end;
        }
        let delta = now - v.previous;
        if delta > 0 {
            let farmed = (U256::from(delta) * big(self.rate) * big(v.staked / E24)).as_u128();
            v.rewards += farmed;
            println!(
                "FARMING {}, delta={}, emission_rate={}, total={}, user={}",
                farmed, delta, self.rate, self.total_stake, v.staked
            );
            v.previous = now;
        }
        return v.rewards;
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

fn big(x: u128) -> U256 {
    x.into()
}

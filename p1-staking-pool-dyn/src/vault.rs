//! Account deposit is information per user about their balances in the exchange.

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::{AccountId, Balance};

use crate::constants::*;
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

impl Vault {
    pub fn is_empty(&self) -> bool {
        self.rewards == 0 && self.staked == 0
    }
}

impl Contract {
    #[inline]
    pub(crate) fn get_vault_or_default(&self, account_id: &AccountId) -> Vault {
        self.vaults.get(account_id).unwrap_or_default()
    }

    #[inline]
    pub(crate) fn get_vault(&self, account_id: &AccountId) -> Vault {
        self.vaults.get(account_id).expect("account not registered")
    }

    pub(crate) fn save_vault(&mut self, account: &AccountId, vault: &Vault) {
        if vault.is_empty() {
            // if the vault is empty, remove
            self.vaults.remove(account);
        } else {
            // save
            self.vaults.insert(account, vault);
        }
    }

    /**
    Update rewards for locked tokens in past epochs
    returns total account rewards
     */
    pub(crate) fn ping(&self, v: &mut Vault) -> u128 {
        if v.is_empty() {
            return 0; // empty vault
        }
        assert!(
            v.previous != 0,
            "Wrong state. Previously registered epoch can't be zero"
        );
        let mut now = current_round();
        // if farming doesn't started, ignore the rewards update
        if now < self.farming_start {
            return 0;
        }
        // compute rewards until the end of the farming
        if now >= self.farming_end {
            now = self.farming_end;
        }
        // if we restarted the farming period, only consider the new period
        if v.previous < self.farming_start {
            v.previous = self.farming_start;
        }
        // avoid subtract with overflow
        if v.previous > now {
            v.previous = now;
        }
        let delta = now - v.previous;
        if delta > 0 {
            // AUDIT: Why multiple as U256, if there are no division by U256?
            //     Maybe `big(v.staked / E24)` should be `big(v.staked) / big(E24)`?
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
}

fn big(x: u128) -> U256 {
    x.into()
}

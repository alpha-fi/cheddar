//! Vault is information per user about their balances in the exchange.

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::{ValidAccountId, U128};
use near_sdk::{env, AccountId, Balance, PromiseOrValue};

// use crate::constants::*;
// use crate::errors::*;
// use crate::util::*;
use crate::*;

#[derive(BorshSerialize, BorshDeserialize, Default)]
#[cfg_attr(feature = "test", derive(Default, Clone))]
pub struct Vault {
    // epoch when the last time ping was called
    pub previous: u64,
    /// amount of $near locked in this vault
    pub staked: Balance,
    /// Amount of accumulated rewards from staking;
    pub rewards: Balance,

    /// Deposited NEAR.
    pub ynear: Balance,
    /// Deposited token balances.
    pub tokens: Balance,
}

#[allow(non_fmt_panic)]
impl Vault {
    #[inline]
    pub(crate) fn remove_token(&mut self, token: &AccountId, amount: u128) {
        assert!(self.tokens >= amount, ERR22_NOT_ENOUGH_TOKENS);
        self.tokens -= amount;
    }

    #[inline]
    pub(crate) fn remove_near(&mut self, ynear: u128) {
        assert!(self.ynear >= ynear + MIN_BALANCE, ERR22_NOT_ENOUGH_TOKENS);
        self.ynear -= ynear;
    }
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
            let farmed = u128::from(delta) * self.rate * v.staked / E24;
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

        assert!(v.staked >= MIN_BALANCE, "{}", ERR02_MIN_BALANCE);
    }
}

// token deposits are done through NEP-141 ft_transfer_call to the NEARswap contract.
#[near_bindgen]
impl Contract {
    /**
    FungibleTokenReceiver implementation
    Callback on receiving tokens by this contract.
    Returns zero.
    Panics when account is not registered. */
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
        // TODO: make sure we account it ocrrectly
        v.tokens += amount.0;
        self.vaults.insert(&sender_id, &v);
        env_log!("Deposit, {} {}", amount.0, token);

        return PromiseOrValue::Value(U128(0));
    }
}

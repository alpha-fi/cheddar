//! Vault is information per user about their balances in the exchange.

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::{ValidAccountId, U128};
use near_sdk::{env, log, AccountId, Balance, PromiseOrValue};

use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_contract_standards::storage_management::{
    StorageBalance, StorageBalanceBounds, StorageManagement,
};

use crate::*;

#[derive(BorshSerialize, BorshDeserialize)]
#[cfg_attr(feature = "test", derive(Default, Clone))]
pub struct Vault {
    /// Contract.reward_acc value when the last ping was called and rewards calculated
    pub reward_acc: Balance,
    /// amount of staking token locked in this vault
    pub staked: Vec<Balance>,
    pub min_stake: Balance,
    /// Amount of accumulated, not withdrawn farmed units. When withdrawing the
    /// farmed units are translated to all `Contract.farm_tokens` based on
    /// `Contract.farm_token_rates`
    pub farmed: Balance,
    /// farmed tokens which failed to withdraw to the user.
    pub farmed_recovered: Vec<Balance>,
    /// Cheddy NFT deposited to get an extra boost. Only one Cheddy can be deposited to a
    /// single acocunt.
    pub cheddy: String,
}

impl Vault {
    pub fn new(staked_len: usize, farmed_len: usize, reward_acc: Balance) -> Self {
        Self {
            reward_acc,
            staked: vec![0; staked_len],
            min_stake: 0,
            farmed: 0,
            farmed_recovered: vec![0; farmed_len],
            cheddy: "".into(),
        }
    }

    /**
    Update rewards for locked tokens in past epochs
    Arguments:
     - `reward_acc`: Contract.reward_acc value
     - `round`: current round
     */
    pub fn ping(&mut self, reward_acc: Balance, round: u64) {
        // note: the last round is at self.farming_end
        // if farming didn't start, ignore the rewards update
        if round == 0 {
            return;
        }
        // no new rewards
        if self.reward_acc >= reward_acc {
            return; // self.farmed;
        }
        self.farmed += self.min_stake * (reward_acc - self.reward_acc) / ACC_OVERFLOW;
        self.reward_acc = reward_acc;
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        all_zeros(&self.staked) && self.farmed == 0 && self.cheddy.is_empty()
    }
}

impl Contract {
    /// Returns the registered vault.
    /// Panics if the account is not registered.
    #[inline]
    pub(crate) fn get_vault(&self, account_id: &AccountId) -> Vault {
        self.vaults.get(account_id).expect(ERR10_NO_ACCOUNT)
    }

    pub(crate) fn ping_all(&mut self, v: &mut Vault) {
        let r = self.current_round();
        self.update_reward_acc(r);
        v.ping(self.reward_acc, r);
    }

    /// updates the rewards accumulator
    pub(crate) fn update_reward_acc(&mut self, round: u64) {
        let new_acc = self.compute_reward_acc(round);
        // we should advance with rounds if self.t is zero, otherwise we have a jump and
        // don't compute properly the accumulator.
        if self.staked_units == 0 || new_acc != self.reward_acc {
            self.reward_acc = new_acc;
            self.reward_acc_round = round;
        }
    }

    /// computes the rewards accumulator.
    /// NOTE: the current, optimized algorithm will not farm anything if
    ///   `self.rate * ACC_OVERFLOW / self.t < 1`
    pub(crate) fn compute_reward_acc(&self, round: u64) -> u128 {
        // covers also when round == 0
        if self.reward_acc_round == round || self.staked_units == 0 {
            return self.reward_acc;
        }

        self.reward_acc
            + u128::from(round - self.reward_acc_round) * self.farm_unit_emission * ACC_OVERFLOW
                / self.staked_units
    }

    pub(crate) fn _recompute_stake(&mut self, v: &mut Vault) {
        let mut s = min_stake(&v.staked, &self.stake_rates);
        if !v.cheddy.is_empty() {
            s += s * u128::from(self.cheddar_nft_boost) / BASIS_P;
        }
        if s > v.min_stake {
            let diff = s - v.min_stake;
            self.staked_units += diff; // must be called after ping_s
            v.min_stake = s;
        } else if s < v.min_stake {
            let diff = v.min_stake - s;
            self.staked_units -= diff; // must be called after ping_s
            v.min_stake = s;
        }
    }

    /// Returns new stake units
    pub(crate) fn _stake(
        &mut self,
        user: &AccountId,
        token: &AccountId,
        amount: Balance,
    ) -> Balance {
        assert!(amount > 0, "staked amount must be positive");
        let token_i = find_acc_idx(&token, &self.stake_tokens);
        let mut v = self.get_vault(user);

        // firstly update the past rewards
        self.ping_all(&mut v);

        v.staked[token_i] += amount;
        self.total_stake[token_i] += amount;
        self._recompute_stake(&mut v);
        self.vaults.insert(user, &v);
        log!("Staked {} {}, stake_units: {}", amount, token, v.min_stake);
        return v.min_stake;
    }

    /// Returns remaining amount of `token` user has staked after the unstake.
    pub(crate) fn _unstake(
        &mut self,
        user: &AccountId,
        token: &AccountId,
        amount: Balance,
    ) -> Balance {
        let token_i = find_acc_idx(token, &self.stake_tokens);

        let mut v = self.get_vault(user);
        assert!(amount <= v.staked[token_i], "{}", ERR30_NOT_ENOUGH_STAKE);
        if amount == v.staked.iter().sum() {
            // no other token is staked,  => close -- simplify UX
            self.close();
            return 0;
        }

        self.ping_all(&mut v);
        // self.total_stake is updated in transfer_staked_tokens
        let remaining = v.staked[token_i] - amount;
        v.staked[token_i] = remaining;
        self._recompute_stake(&mut v);
        self.vaults.insert(user, &v);
        self.transfer_staked_tokens(user.clone(), token_i, amount);
        return remaining;
    }

    pub(crate) fn _withdraw_nft(&mut self, user: &AccountId, v: &mut Vault, receiver: AccountId) {
        assert!(!v.cheddy.is_empty(), "Sender has no NFT deposit");
        self.ping_all(v);
        ext_nft::nft_transfer(
            receiver,
            v.cheddy.clone(),
            None,
            Some("Cheddy withdraw".to_string()),
            &self.cheddar_nft,
            1,
            GAS_FOR_FT_TRANSFER,
        )
        .then(ext_self::withdraw_nft_callback(
            user.clone(),
            v.cheddy.clone(),
            &env::current_account_id(),
            0,
            GAS_FOR_MINT_CALLBACK,
        ));

        v.cheddy = "".into();
        self._recompute_stake(v);
    }
}

// token deposits are done through NEP-141 ft_transfer_call to the NEARswap contract.
#[near_bindgen]
impl FungibleTokenReceiver for Contract {
    /**
    FungibleTokenReceiver implementation Callback on receiving tokens by this contract.
    Handles both farm deposits and stake deposits. For farm deposit (sending tokens
    to setup the farm) you must set "setup reward deposit" msg.
    Otherwise tokens will be staken.
    Returns zero.
    Panics when:
    - account is not registered
    - or receiving a wrong token
    - or making a farm deposit after farm is finalized
    - or staking before farm is finalized. */
    #[allow(unused_variables)]
    fn ft_on_transfer(
        &mut self,
        sender_id: ValidAccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        assert!(self.is_active, "contract is not active");

        let token = env::predecessor_account_id();
        assert!(
            token != NEAR_TOKEN,
            "near must be sent using deposit_near()"
        );
        assert!(amount.0 > 0, "staked amount must be positive");
        if msg == "setup reward deposit" {
            self._setup_deposit(&token, amount.0);
        } else {
            self.assert_is_active();
            self._stake(sender_id.as_ref(), &token, amount.0);
        }

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
        assert!(self.is_active, "contract is not active");
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
            self.create_account(&account_id);

            let refund = amount - STORAGE_COST;
            if refund > 0 {
                Promise::new(env::predecessor_account_id()).transfer(refund);
            }
        }
        storage_balance()
    }

    /// Method not supported. Close the account (`close()` or
    /// `storage_unregister(true)`) to close the account and withdraw deposited NEAR.
    #[allow(unused_variables)]
    fn storage_withdraw(&mut self, amount: Option<U128>) -> StorageBalance {
        panic!("Storage withdraw not possible, close the account instead");
    }

    /// When force == true it will close the account. Otherwise this is noop.
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

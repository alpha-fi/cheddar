use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::LookupMap;
use near_sdk::json_types::{ValidAccountId, U128};
use near_sdk::{
    assert_one_yocto, env, log, near_bindgen, AccountId, Balance, PanicOnDefault, Promise,
    PromiseOrValue, PromiseResult,
};

pub mod constants;
pub mod errors;
pub mod interfaces;
// pub mod util;
pub mod helpers;
pub mod vault;

use crate::helpers::*;
use crate::interfaces::*;
use crate::{constants::*, errors::*, vault::*};

near_sdk::setup_alloc!();

/// P2 rewards distribution contract implementing the "Scalable Reward Distribution on the Ethereum Blockchain"
/// algorithm:
/// https://uploads-ssl.webflow.com/5ad71ffeb79acc67c8bcdaba/5ad8d1193a40977462982470_scalable-reward-distribution-paper.pdf
#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    /// if farming is opened
    pub is_active: bool,
    pub setup_finalized: bool,
    pub owner_id: AccountId,
    /// Treasury address - a destination for the collected fees.
    pub treasury: AccountId,
    /// user vaults
    pub vaults: LookupMap<AccountId, Vault>,
    pub stake_tokens: Vec<AccountId>,
    /// total number of units currently staked
    pub staked_units: Balance,
    /// Rate between the staking token and stake units.
    /// When farming the `min(staking_token[i]*stake_rate[i]/1e24)` is taken
    /// to allocate farm_units.
    /// Cheddar should be the first stake token.
    pub stake_rates: Vec<u128>,
    pub farm_tokens: Vec<AccountId>,
    /// Ratios between the farm unit and all farm tokens when computing reward.
    /// When farming, for each token index i in `farm_tokens` we allocate to
    /// a user `vault.farmed*farm_token_rates[i]/1e24`.
    /// Farmed tokens are distributed to all users proportionally to their stake.
    pub farm_token_rates: Vec<u128>,
    /// amount of $farm_units farmed during each round. Round duration is defined in constants.rs
    /// Farmed $farm_units are distributed to all users proportionally to their stake.
    pub farm_unit_emission: u128,
    /// received deposits for farming reward
    pub farm_deposits: Vec<u128>,
    /// unix timestamp (seconds) when the farming starts.
    pub farming_start: u64,
    /// unix timestamp (seconds) when the farming ends (first time with no farming).
    pub farming_end: u64,
    /// NFT token used for boost
    pub cheddar_nft: AccountId,
    /// boost when staking cheddy in basis points
    pub cheddar_nft_boost: u32,
    /// total number of harvested farm tokens
    pub total_harvested: Vec<Balance>,
    /// rewards accumulator: running sum of farm_units per token (equals to the total
    /// number of farmed unit tokens).
    reward_acc: u128,
    /// round number when the s was previously updated.
    reward_acc_round: u64,
    /// total amount of currently staked tokens.
    total_stake: Vec<Balance>,
    /// total number of accounts currently registered.
    pub accounts_registered: u64,
    /// Free rate in basis points. The fee is charged from the user staked tokens
    /// on withdraw. Example: if fee=2 and user withdraws 10000e24 staking tokens
    /// then the protocol will charge 2e24 staking tokens.
    pub fee_rate: u128,
    /// amount of fee collected (in staking token).
    pub fee_collected: Vec<Balance>,
}

#[near_bindgen]
impl Contract {
    /// Initializes the contract with the account where the NEP-141 token contract resides, start block-timestamp & rewards_per_year.
    /// Parameters:
    /// * `stake_tokens`: tokens we are staking, cheddar should be one of them.
    /// * `farming_start` & `farming_end` are unix timestamps (in seconds).
    /// * `fee_rate`: the Contract.fee parameter (in basis points)
    /// The farm starts desactivated. To activate, you must send required farming deposits and
    /// call `self.finalize_setup()`.
    #[init]
    pub fn new(
        owner_id: ValidAccountId,
        stake_tokens: Vec<ValidAccountId>,
        stake_rates: Vec<U128>,
        farm_unit_emission: U128,
        farm_tokens: Vec<ValidAccountId>,
        farm_token_rates: Vec<U128>,
        farming_start: u64,
        farming_end: u64,
        cheddar_nft: ValidAccountId,
        cheddar_nft_boost: u32,
        fee_rate: u32,
        treasury: ValidAccountId,
    ) -> Self {
        assert!(farming_end > farming_start, "End must be after start");
        assert!(stake_rates[0].0 == E24, "stake_rate[0] must be 1e24");
        // assert!(
        //     farm_token_rates[0].0 == E24,
        //     "farm_token_rates[0] must be 1e24"
        // );
        let stake_len = stake_tokens.len();
        let farm_len = farm_tokens.len();
        let c = Self {
            is_active: true,
            setup_finalized: false,
            owner_id: owner_id.into(),
            treasury: treasury.into(),
            vaults: LookupMap::new(b"v".to_vec()),
            stake_tokens: stake_tokens.iter().map(|x| x.to_string()).collect(),
            staked_units: 0,
            stake_rates: stake_rates.iter().map(|x| x.0).collect(),
            farm_tokens: farm_tokens.iter().map(|x| x.to_string()).collect(),
            farm_token_rates: farm_token_rates.iter().map(|x| x.0).collect(),
            farm_unit_emission: farm_unit_emission.0,
            farm_deposits: vec![0; farm_len],
            farming_start,
            farming_end,
            cheddar_nft: cheddar_nft.into(),
            cheddar_nft_boost,
            total_harvested: vec![0; farm_len],
            reward_acc: 0,
            reward_acc_round: 0,
            total_stake: vec![0; stake_len],
            accounts_registered: 0,
            fee_rate: fee_rate.into(),
            fee_collected: vec![0; stake_len],
        };
        c.check_vectors();
        c
    }

    fn check_vectors(&self) {
        let fl = self.farm_tokens.len();
        let sl = self.stake_tokens.len();
        assert!(
            fl == self.farm_token_rates.len()
                && fl == self.total_harvested.len()
                && fl == self.farm_deposits.len(),
            "farm token vector length is not correct"
        );
        assert!(
            sl == self.stake_rates.len()
                && sl == self.total_stake.len()
                && sl == self.fee_collected.len(),
            "stake token vector length is not correct"
        );
    }

    // ************ //
    // view methods //

    /// Returns amount of staked NEAR and farmed CHEDDAR of given account.
    pub fn get_contract_params(&self) -> ContractParams {
        ContractParams {
            owner_id: self.owner_id.clone(),
            stake_tokens: self.stake_tokens.clone(),
            stake_rates: to_U128s(&self.stake_rates),
            farm_unit_emission: self.farm_unit_emission.into(),
            farm_tokens: self.farm_tokens.clone(),
            farm_token_rates: to_U128s(&self.farm_token_rates),
            farm_deposits: to_U128s(&self.farm_deposits),
            is_active: self.is_active,
            farming_start: self.farming_start,
            farming_end: self.farming_end,
            cheddar_nft: self.cheddar_nft.clone(),
            total_staked: to_U128s(&self.total_stake),
            total_farmed: to_U128s(&self.total_harvested),
            fee_rate: self.fee_rate.into(),
            accounts_registered: self.accounts_registered,
        }
    }

    pub fn status(&self, account_id: AccountId) -> Option<Status> {
        return match self.vaults.get(&account_id) {
            Some(mut v) => {
                let r = self.current_round();
                v.ping(self.compute_reward_acc(r), r);
                // round starts from 1 when now >= farming_start
                let r0 = if r > 1 { r - 1 } else { 0 };
                let farmed = self
                    .farm_token_rates
                    .iter()
                    .map(|rate| U128::from(farmed_tokens(v.farmed, *rate)))
                    .collect();
                return Some(Status {
                    stake_tokens: to_U128s(&v.staked),
                    stake: v.min_stake.into(),
                    farmed_units: v.farmed.into(),
                    farmed_tokens: farmed,
                    cheddy_nft: v.cheddy,
                    timestamp: self.farming_start + r0 * ROUND,
                });
            }
            None => None,
        };
    }

    // ******************* //
    // transaction methods //

    /// Implements nft receiver handler
    #[allow(unused_variables)]
    pub fn nft_on_transfer(
        &mut self,
        sender_id: AccountId,
        previous_owner_id: AccountId,
        token_id: String,
        msg: String,
    ) -> PromiseOrValue<bool> {
        if env::predecessor_account_id() != self.cheddar_nft {
            log!("Only Cheddy NFTs ({}) are supported", self.cheddar_nft);
            return PromiseOrValue::Value(true);
        }
        let v = self.vaults.get(&previous_owner_id);
        if v.is_none() {
            log!("Account not registered. Register prior to depositing NFT");
            return PromiseOrValue::Value(true);
        }
        let mut v = v.unwrap();
        if !v.cheddy.is_empty() {
            log!("Account already has Cheddy deposited. You can only deposit one cheddy");
            return PromiseOrValue::Value(true);
        }
        log!("Staking Cheddy NFT - you will obtain a special farming boost");
        self.ping_all(&mut v);

        v.cheddy = token_id;
        self._recompute_stake(&mut v);
        self.vaults.insert(&previous_owner_id, &v);
        return PromiseOrValue::Value(false);
    }

    /// withdraw NFT to a destination account using the `nft_transfer` method.
    pub fn withdraw_nft(&mut self, receiver_id: ValidAccountId) {
        let user = env::predecessor_account_id();
        let mut v = self.get_vault(&user);
        self._withdraw_nft(&mut v, receiver_id.into());
        self.vaults.insert(&user, &v);
    }

    pub(crate) fn _setup_deposit(&mut self, token: &AccountId, amount: u128) {
        assert!(
            !self.setup_finalized,
            "setup deposits must be done when contract setup is not finalized"
        );
        let token_i = find_acc_idx(token, &self.farm_tokens);
        let total_rounds = round_number(self.farming_start, self.farming_end, self.farming_end);
        let expected = farmed_tokens(
            u128::from(total_rounds) * self.farm_unit_emission,
            self.farm_token_rates[token_i],
        );
        assert_eq!(
            self.farm_deposits[token_i], 0,
            "deposit already done for the given token"
        );
        assert_eq!(
            amount, expected,
            "Expected deposit for token {} is {}, got {}",
            self.farm_tokens[token_i], expected, amount
        );
        self.farm_deposits[token_i] = amount;
    }

    /// Deposit native near during the setup phase for farming rewards.
    /// Panics when the deposit was already done or the setup is completed.
    #[payable]
    pub fn setup_deposit_near(&mut self) {
        self._setup_deposit(&NEAR_TOKEN.to_string(), env::attached_deposit())
    }

    /// stakes native near.
    /// The transaction fails if near is not included in `self.stake_amount`.
    #[payable]
    pub fn stake_near(&mut self) {
        let a = env::predecessor_account_id();
        self._stake(&a, &NEAR_TOKEN.to_string(), env::attached_deposit());
    }

    // NEP-141 token staking is done via ft_transfer_call

    /// Unstakes given amount of tokens and transfers it back to the user.
    /// If amount equals to the amount staked then we close the account.
    /// NOTE: account once closed must re-register to stake again.
    /// Returns amount of staked tokens left (still staked) after the call.
    /// Panics if the caller doesn't stake anything or if he doesn't have enough staked tokens.
    /// Requires 1 yNEAR payment for wallet 2FA.
    #[payable]
    pub fn unstake(&mut self, token: ValidAccountId, amount: U128) -> U128 {
        self.assert_is_active();
        assert_one_yocto();
        let user = env::predecessor_account_id();
        self._unstake(&user, token.as_ref(), amount.0).into()
    }

    /// Unstakes everything and close the account. Sends all farmed tokens using a ft_transfer
    /// and all staked tokens back to the caller.
    /// Panics if the caller doesn't stake anything.
    /// Requires 1 yNEAR payment for wallet validation.
    #[payable]
    pub fn close(&mut self) {
        self.assert_is_active();
        assert_one_yocto();
        let a = env::predecessor_account_id();
        let mut v = self.get_vault(&a);
        self.ping_all(&mut v);
        log!("Closing {} account, farmed: {:?}", &a, v.farmed);

        // if user doesn't stake anything and has no rewards then we can make a shortcut
        // and remove the account and return storage deposit.
        if v.is_empty() {
            self.vaults.remove(&a);
            Promise::new(a.clone()).transfer(STORAGE_COST);
            return;
        }

        let s = min_stake(&v.staked, &self.stake_rates);
        self.staked_units -= s;
        for i in 0..self.total_stake.len() {
            self.transfer_staked_tokens(a.clone(), i, v.staked[i]);
        }
        self._withdraw_crop(&a, v.farmed);
        if !v.cheddy.is_empty() {
            self._withdraw_nft(&mut v, a.clone());
        }
        self.vaults.remove(&a);
    }

    /// Withdraws all farmed tokens to the user. It doesn't close the account.
    /// Panics if user has not staked anything.
    pub fn withdraw_crop(&mut self) {
        self.assert_is_active();
        let a = env::predecessor_account_id();
        let mut v = self.get_vault(&a);
        self.ping_all(&mut v);
        let farmed_units = v.farmed;
        v.farmed = 0;
        self.vaults.insert(&a, &v);
        self._withdraw_crop(&a, farmed_units);
    }

    /** transfers harvested tokens to the user
    / NOTE: the destination account must be registered on CHEDDAR first!
    / NOTE: callers MUST set user `vault.farmed_units` to zero prior to the call
    /       because in case of failure the callbacks will re-add rewards to the vault */
    fn _withdraw_crop(&mut self, user: &AccountId, farmed_units: u128) {
        if farmed_units == 0 {
            // nothing to mint nor return.
            return;
        }
        for i in 0..self.farm_tokens.len() {
            let amount = farmed_tokens(farmed_units, self.farm_token_rates[i]);
            self.transfer_farmed_tokens(user, i, amount);
        }
    }

    /// Returns the amount of collected fees which are not withdrawn yet.
    pub fn get_collected_fee(&self) -> Vec<U128> {
        to_U128s(&self.fee_collected)
    }

    /// Withdraws all collected fee to the treasury.
    /// Must make sure treasury is registered
    /// Panics if the collected fees == 0.
    pub fn withdraw_fees(&mut self) {
        log!("Withdrawing collected fee: {:?} tokens", self.fee_collected);
        for i in 0..self.stake_tokens.len() {
            if self.fee_collected[i] != 0 {
                ext_ft::ft_transfer(
                    self.treasury.clone(),
                    self.fee_collected[i].into(),
                    Some("fee withdraw".to_string()),
                    &self.stake_tokens[i],
                    1,
                    GAS_FOR_FT_TRANSFER,
                );
                self.fee_collected[i] = 0;
            }
        }
    }

    // ******************* //
    // management          //

    /// Opens or closes smart contract operations. When the contract is not active, it won't
    /// reject every user call, until it will be open back again.
    pub fn set_active(&mut self, is_open: bool) {
        self.assert_owner();
        self.is_active = is_open;
    }

    /// start and end are unix timestamps (in seconds)
    pub fn set_start_end(&mut self, start: u64, end: u64) {
        self.assert_owner();
        assert!(
            start > env::block_timestamp() / SECOND,
            "start must be in the future"
        );
        assert!(start < end, "start must be before end");
        self.farming_start = start;
        self.farming_end = end;
    }

    /// withdraws farming tokens back to owner
    pub fn admin_withdraw(&mut self, token: AccountId, amount: U128) {
        self.assert_owner();
        ext_ft::ft_transfer(
            self.owner_id.clone(),
            amount,
            Some("admin-withdrawing-back".to_string()),
            &token,
            1,
            GAS_FOR_FT_TRANSFER,
        );
    }

    pub fn finalize_setup(&mut self) {
        assert!(
            !self.setup_finalized,
            "setup deposits must be done when contract setup is not finalized"
        );
        let now = env::block_timestamp() / SECOND;
        assert!(
            now < self.farming_start - ROUND, // TODO: change to 1 day?
            "must be finalized at last before farm start"
        );
        for i in 0..self.farm_deposits.len() {
            assert_ne!(
                self.farm_deposits[i], 0,
                "Deposit for token {} not done",
                self.farm_tokens[i]
            )
        }
        self.setup_finalized = true;
    }

    /// Returns expected and received deposits for farmed tokens
    pub fn finalize_setup_expected(&self) -> (Vec<U128>, Vec<U128>) {
        let total_rounds = u128::from(round_number(
            self.farming_start,
            self.farming_end,
            self.farming_end,
        ));
        let out = self
            .farm_token_rates
            .iter()
            .map(|rate| farmed_tokens(total_rounds * self.farm_unit_emission, *rate))
            .collect();
        (to_U128s(&out), to_U128s(&self.farm_deposits))
    }

    /*****************
     * internal methods */

    fn assert_is_active(&self) {
        assert!(self.setup_finalized, "contract is not setup yet");
        assert!(self.is_active, "contract is not active");
    }

    /// transfers staked tokens (token identified by an index in
    /// self.stake_tokens) back to the user.
    /// `self.staked_units` must be adjusted in the caller. The callback will fix the
    /// `self.staked_units` if the transfer will fails.
    fn transfer_staked_tokens(&mut self, user: AccountId, token_i: usize, amount: u128) -> Promise {
        if amount == 0 {
            return Promise::new(user);
        }
        let fee = amount * self.fee_rate / 10_000;
        let amount = amount - fee;
        let token = &self.stake_tokens[token_i];
        log!("unstaking {}, fee: {}", token, fee);
        if token == NEAR_TOKEN {
            return Promise::new(user).transfer(amount);
        }

        return ext_ft::ft_transfer(
            user.clone(),
            amount.into(),
            Some("unstaking".to_string()),
            token,
            1,
            GAS_FOR_FT_TRANSFER,
        )
        .then(ext_self::transfer_staked_callback(
            user,
            token_i,
            amount.into(),
            fee.into(),
            &env::current_account_id(),
            0,
            GAS_FOR_MINT_CALLBACK,
        ));
    }

    #[inline]
    fn transfer_farmed_tokens(&mut self, u: &AccountId, token_i: usize, amount: u128) -> Promise {
        let token = &self.farm_tokens[token_i];
        self.total_harvested[token_i] += amount;
        if token == NEAR_TOKEN {
            return Promise::new(u.clone()).transfer(amount);
        }

        self.total_stake[token_i] -= amount;
        let amount: U128 = amount.into();
        return ext_ft::ft_transfer(
            u.clone(),
            amount,
            Some("farming".to_string()),
            token,
            1,
            GAS_FOR_FT_TRANSFER,
        )
        .then(ext_self::transfer_farmed_callback(
            u.clone(),
            token_i,
            amount,
            &env::current_account_id(),
            0,
            GAS_FOR_MINT_CALLBACK,
        ));
    }

    #[private]
    pub fn transfer_staked_callback(
        &mut self,
        user: AccountId,
        token_i: usize,
        amount: U128,
        fee: U128,
    ) {
        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),

            PromiseResult::Successful(_) => {
                self.fee_collected[token_i] += fee.0;
                log!("tokens withdrawn {}", amount.0);
                // we can't remove the vault here, because we don't know if `mint` succeded
                //  if it didn't succeed, the the mint_callback will try to recover the vault
                //  and recreate it - so potentially we will send back to the user NEAR deposit
                //  multiple times. User should call `close` second time to get back
                //  his NEAR deposit.
            }

            PromiseResult::Failed => {
                log!(
                    "transferring {} {} token failed. Recovering account state",
                    amount.0,
                    self.stake_tokens[token_i],
                );
                let full_amount = amount.0 + fee.0;
                self.total_stake[token_i] += full_amount;
                self.recover_state(&user, true, token_i, full_amount);
            }
        }
    }

    #[private]
    pub fn transfer_farmed_callback(&mut self, user: AccountId, token_i: usize, amount: U128) {
        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),

            PromiseResult::Successful(_) => {
                // see comment in transfer_staked_callback function
            }

            PromiseResult::Failed => {
                log!(
                    "harvesting {} {} token failed. recovering account state",
                    amount.0,
                    self.stake_tokens[token_i],
                );
                self.recover_state(&user, false, token_i, amount.0);
            }
        }
    }

    // // TODO: remove?
    // #[private]
    // pub fn close_account(&mut self, user: AccountId) {
    //     let mut all_good = true;
    //     for i in 0..env::promise_results_count() {
    //         match env::promise_result(i) {
    //             PromiseResult::Failed => all_good = false,
    //             _ => {}
    //         }
    //     }
    //     if !all_good {
    //         return;
    //     }
    //     if let Some(v) = self.vaults.get(&user) {
    //         if all_zeros(&v.staked) && v.farmed == 0 {
    //             log!("returning storage deposit");
    //             self.vaults.remove(&user);
    //             self.accounts_registered -= 1;
    //             Promise::new(user).transfer(STORAGE_COST);
    //         }
    //     }
    // }

    fn recover_state(&mut self, user: &AccountId, is_staked: bool, token_i: usize, amount: u128) {
        let mut v;
        if let Some(v2) = self.vaults.get(&user) {
            v = v2;
        } else {
            // If the vault was closed before by another TX, then we must recover the state
            self.accounts_registered += 1;
            v = Vault::new(self.stake_tokens.len(), self.reward_acc)
        }
        if is_staked {
            v.staked[token_i] += amount;
            let s = min_stake(&v.staked, &self.stake_rates);
            let diff = s - v.min_stake;
            if diff > 0 {
                self.staked_units += diff;
            }
        } else {
            self.total_harvested[token_i] -= amount;
            // TODO: maybe we should add a list of harvested tokens which still have to be withdrawn?
            //     v.farmed[token_i] += amount;
        }

        self.vaults.insert(user, &v);
    }

    /// Returns the round number since `start`.
    /// If now < start  return 0.
    /// If now == start return 0.
    /// if now == start + ROUND return 1...
    fn current_round(&self) -> u64 {
        round_number(
            self.farming_start,
            self.farming_end,
            env::block_timestamp() / SECOND,
        )
    }

    /// creates new empty account. User must deposit tokens using transfer_call
    fn create_account(&mut self, user: &AccountId) {
        self.vaults
            .insert(&user, &Vault::new(self.stake_tokens.len(), self.reward_acc));
        self.accounts_registered += 1;
    }

    fn assert_owner(&self) {
        assert!(
            env::predecessor_account_id() == self.owner_id,
            "can only be called by the owner"
        );
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[allow(unused_imports)]
mod tests {
    use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
    use near_contract_standards::storage_management::StorageManagement;
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::{testing_env, Balance};
    use near_sdk::{MockedBlockchain, ValidatorId};
    use serde::de::IntoDeserializer;
    use std::convert::TryInto;
    use std::vec;

    use super::*;

    fn acc_cheddar() -> ValidAccountId {
        "cheddar1".to_string().try_into().unwrap()
    }

    fn acc_farming2() -> ValidAccountId {
        "cheddar2".to_string().try_into().unwrap()
    }

    fn acc_staking1() -> ValidAccountId {
        "atom1".try_into().unwrap()
    }

    fn acc_staking2() -> ValidAccountId {
        "atom2".try_into().unwrap()
    }

    fn acc_nft_cheddy() -> ValidAccountId {
        "nft_cheddy".try_into().unwrap()
    }

    fn acc_u1() -> ValidAccountId {
        "user1".try_into().unwrap()
    }

    fn acc_u2() -> ValidAccountId {
        "user2".try_into().unwrap()
    }

    #[allow(dead_code)]
    fn acc_u3() -> ValidAccountId {
        "user3".try_into().unwrap()
    }

    fn acc_owner() -> ValidAccountId {
        "user_owner".try_into().unwrap()
    }

    /// half of the block round
    // const ROUND_NS_H: u64 = ROUND_NS / 2;
    /// first and last round
    const END: i64 = 10;
    const RATE: u128 = E24 * 2; // 2 farming_units / round (60s)
    const BOOST: u32 = 250;

    fn round(r: i64) -> u64 {
        let r: u64 = (10 + r).try_into().unwrap();
        return r * ROUND_NS;
    }

    /// deposit_dec = size of deposit in e24 to set for the next transacton
    fn setup_contract(
        predecessor: ValidAccountId,
        deposit_dec: u128,
        fee_rate: u32,
    ) -> (VMContextBuilder, Contract) {
        let mut context = VMContextBuilder::new();
        testing_env!(context.build());
        let contract = Contract::new(
            acc_owner(),
            vec![acc_staking1(), acc_staking2()],
            to_U128s(&vec![E24, E24 / 10]),      // staking rates
            RATE.into(),                         // farm_unit_emission
            vec![acc_cheddar(), acc_farming2()], // farming tokens
            to_U128s(&vec![E24, E24 / 2]),       // farming rates
            round(0) / SECOND,                   // farming start
            round(END) / SECOND,                 // farmnig end
            acc_nft_cheddy(),                    // cheddy nft
            BOOST,                               // cheddy boost
            fee_rate,
            accounts(1), // treasury
        );
        contract.check_vectors();
        testing_env!(context
            .predecessor_account_id(predecessor)
            .attached_deposit(deposit_dec.into())
            .block_timestamp(round(-10))
            .build());
        (context, contract)
    }

    /// epoch is a timer in rounds (rather than miliseconds)
    fn stake(
        ctx: &mut VMContextBuilder,
        ctr: &mut Contract,
        user: &ValidAccountId,
        token: &ValidAccountId,
        amount: u128,
    ) {
        testing_env!(ctx
            .attached_deposit(0)
            .predecessor_account_id(token.clone())
            .build());
        ctr.ft_on_transfer(user.clone(), amount.into(), "transfer to farm".to_string());
    }

    /// epoch is a timer in rounds (rather than miliseconds)
    fn unstake(
        ctx: &mut VMContextBuilder,
        ctr: &mut Contract,
        user: &ValidAccountId,
        token: &ValidAccountId,
        amount: u128,
    ) {
        testing_env!(ctx
            .attached_deposit(1)
            .predecessor_account_id(user.clone())
            .build());
        ctr.unstake(token.clone(), amount.into());
    }

    /// epoch is a timer in rounds (rather than miliseconds)
    fn close(ctx: &mut VMContextBuilder, ctr: &mut Contract, user: &ValidAccountId) {
        testing_env!(ctx
            .attached_deposit(1)
            .predecessor_account_id(user.clone())
            .build());
        ctr.close();
    }

    /// epoch is a timer in rounds (rather than miliseconds)
    fn register_user_and_stake(
        ctx: &mut VMContextBuilder,
        ctr: &mut Contract,
        user: &ValidAccountId,
        stake_amounts: &Vec<u128>,
        r: i64,
    ) {
        testing_env!(ctx
            .attached_deposit(STORAGE_COST)
            .predecessor_account_id(user.clone())
            .block_timestamp(round(r))
            .build());
        ctr.storage_deposit(None, None);
        for i in 0..ctr.stake_tokens.len() {
            stake(
                ctx,
                ctr,
                user,
                &ctr.stake_tokens[i].to_string().try_into().unwrap(),
                stake_amounts[i].into(),
            );
        }
    }

    #[test]
    fn test_set_active() {
        let (_, mut ctr) = setup_contract(acc_owner(), 5, 0);
        assert_eq!(ctr.is_active, true);
        ctr.set_active(false);
        assert_eq!(ctr.is_active, false);
    }

    #[test]
    #[should_panic(expected = "can only be called by the owner")]
    fn test_set_active_not_admin() {
        let (_, mut ctr) = setup_contract(accounts(0), 0, 1);
        ctr.set_active(false);
    }

    fn finalize(ctr: &mut Contract) {
        ctr._setup_deposit(&acc_cheddar().into(), 20 * E24);
        ctr._setup_deposit(&acc_farming2().into(), 10 * E24);
        ctr.finalize_setup();
    }

    #[test]
    fn test_finalize_setup() {
        let (_, mut ctr) = setup_contract(accounts(1), 0, 0);
        assert_eq!(
            ctr.setup_finalized, false,
            "at the beginning setup mut not be finalized"
        );
        finalize(&mut ctr);
        assert_eq!(ctr.setup_finalized, true)
    }

    #[test]
    #[should_panic(expected = "must be finalized at last before farm start")]
    fn test_finalize_setup_too_late() {
        let (mut ctx, mut ctr) = setup_contract(accounts(1), 0, 0);
        ctr._setup_deposit(&acc_cheddar().into(), 20 * E24);
        ctr._setup_deposit(&acc_farming2().into(), 10 * E24);
        testing_env!(ctx.block_timestamp(10 * ROUND_NS).build());
        ctr.finalize_setup();
    }

    #[test]
    #[should_panic(expected = "Expected deposit for token cheddar1 is 20000000000000000000000000")]
    fn test_finalize_setup_wrong_deposit() {
        let (_, mut ctr) = setup_contract(accounts(1), 0, 0);
        ctr._setup_deposit(&acc_cheddar().into(), 10 * E24);
    }

    #[test]
    #[should_panic(expected = "Deposit for token cheddar2 not done")]
    fn test_finalize_setup_not_enough_deposit() {
        let (_, mut ctr) = setup_contract(accounts(1), 0, 0);
        ctr._setup_deposit(&acc_cheddar().into(), 20 * E24);
        ctr.finalize_setup();
    }

    #[test]
    fn test_round_number() {
        let (mut ctx, ctr) = setup_contract(acc_u1(), 0, 0);
        assert_eq!(ctr.current_round(), 0);

        assert_eq!(round(-9), ROUND_NS);
        assert_eq!(ctr.farming_start, 10 * ROUND);

        testing_env!(ctx.block_timestamp(round(-2)).build());
        assert_eq!(ctr.current_round(), 0);

        testing_env!(ctx.block_timestamp(round(0)).build());
        assert_eq!(ctr.current_round(), 0);

        assert_eq!(round(1), 11 * ROUND_NS);

        testing_env!(ctx.block_timestamp(round(1)).build());
        assert_eq!(ctr.current_round(), 1);

        testing_env!(ctx.block_timestamp(round(10)).build());
        assert_eq!(ctr.current_round(), 10);
        testing_env!(ctx.block_timestamp(round(11)).build());
        assert_eq!(ctr.current_round(), 10);

        let total_rounds = round_number(ctr.farming_start, ctr.farming_end, ctr.farming_end);
        assert_eq!(total_rounds, 10);
    }

    #[test]
    #[should_panic(
        expected = "The attached deposit is less than the minimum storage balance (60000000000000000000000)"
    )]
    fn test_min_storage_deposit() {
        let (mut ctx, mut ctr) = setup_contract(acc_u1(), 0, 0);
        testing_env!(ctx.attached_deposit(STORAGE_COST / 4).build());
        ctr.storage_deposit(None, None);
    }

    #[test]
    fn test_storage_deposit() {
        let user = acc_u1();
        let (mut ctx, mut ctr) = setup_contract(user.clone(), 0, 0);

        match ctr.storage_balance_of(user.clone()) {
            Some(_) => panic!("unregistered account must not have a balance"),
            _ => {}
        };

        testing_env!(ctx.attached_deposit(STORAGE_COST).build());
        ctr.storage_deposit(None, None);
        match ctr.storage_balance_of(user) {
            None => panic!("user account should be registered"),
            Some(s) => {
                assert_eq!(s.available.0, 0, "availabe should be 0");
                assert_eq!(
                    s.total.0, STORAGE_COST,
                    "total user storage deposit should be correct"
                );
            }
        }
    }

    #[test]
    fn test_alone_staking() {
        let u1 = acc_u1();
        let u1_a: AccountId = u1.clone().into();
        let t_s1 = acc_staking1(); // token 1
        let t_s2 = acc_staking2();

        let (mut ctx, mut ctr) = setup_contract(u1.clone(), 0, 0);
        finalize(&mut ctr);

        assert!(
            ctr.status(u1_a.clone()).is_none(),
            "u1 is not registered yet"
        );

        // register user1 account
        testing_env!(ctx.attached_deposit(STORAGE_COST).build());
        ctr.storage_deposit(None, None);
        let mut a1 = ctr.status(u1_a.clone()).unwrap();
        assert_eq!(to_u128s(&a1.stake_tokens), vec![0, 0], "a1 didn't stake");
        assert_eq!(a1.farmed_units.0, 0, "a1 didn't stake so no cheddar");

        // ------------------------------------------------
        // stake before farming_start
        testing_env!(ctx.block_timestamp(round(-3)).build());
        stake(&mut ctx, &mut ctr, &u1, &t_s1, E24);
        a1 = ctr.status(u1_a.clone()).unwrap();
        let mut a1_stake = vec![E24, 0];
        assert_eq!(to_u128s(&a1.stake_tokens), a1_stake, "a1 stake");
        assert_eq!(a1.farmed_units.0, 0, "farming didn't start yet");
        assert_eq!(
            ctr.total_stake, a1_stake,
            "total stake should equal to account1 stake"
        );

        // ------------------------------------------------
        // stake one more time before farming_start
        testing_env!(ctx.block_timestamp(round(-2)).build());
        stake(&mut ctx, &mut ctr, &u1, &t_s1, 3 * E24);
        stake(&mut ctx, &mut ctr, &u1, &t_s2, 2 * E24);
        a1 = ctr.status(u1_a.clone()).unwrap();
        a1_stake = vec![4 * E24, 2 * E24];
        assert_eq!(to_u128s(&a1.stake_tokens), a1_stake, "a1 stake");
        assert_eq!(a1.farmed_units.0, 0, "a1 didn't stake so no cheddar");
        assert_eq!(
            ctr.total_stake, a1_stake,
            "total stake should equal to account1 stake"
        );

        // ------------------------------------------------
        // Staking before the beginning won't yield rewards
        testing_env!(ctx.block_timestamp(round(0) - 1).build());
        a1 = ctr.status(u1_a.clone()).unwrap();
        assert_eq!(
            to_u128s(&a1.stake_tokens),
            a1_stake,
            "account1 stake didn't change"
        );
        assert_eq!(
            a1.farmed_units.0, 0,
            "no farmed_units should be rewarded before start"
        );

        // ------------------------------------------------
        // First round - a whole epoch needs to pass first to get first rewards
        testing_env!(ctx.block_timestamp(round(0) + 1).build());
        a1 = ctr.status(u1_a.clone()).unwrap();
        assert_eq!(a1.farmed_units.0, 0, "need to stake whole round to farm");

        // ------------------------------------------------
        // 3rd round. We are alone - we should get 100% of emission of first 2 rounds.

        testing_env!(ctx.block_timestamp(round(2)).build());
        a1 = ctr.status(u1_a.clone()).unwrap();
        assert_eq!(
            to_u128s(&a1.stake_tokens),
            a1_stake,
            "account1 stake didn't change"
        );
        assert_eq!(a1.farmed_units.0, 2 * RATE, "we take all harvest");

        // ------------------------------------------------
        // middle of the 3rd round.
        // second check in same epoch shouldn't change rewards
        testing_env!(ctx.block_timestamp(round(2) + 100).build());
        a1 = ctr.status(u1_a.clone()).unwrap();
        assert_eq!(
            a1.farmed_units.0,
            2 * RATE,
            "in the same epoch we should harvest only once"
        );

        // ------------------------------------------------
        // last round
        testing_env!(ctx.block_timestamp(round(9)).build());
        let total_rounds: u128 =
            round_number(ctr.farming_start, ctr.farming_end, ctr.farming_end).into();
        a1 = ctr.status(u1_a.clone()).unwrap();
        assert_eq!(
            a1.farmed_units.0,
            (total_rounds - 1) * RATE,
            "in the last round we should get rewards minus one round"
        );

        // ------------------------------------------------
        // end of farming
        testing_env!(ctx.block_timestamp(round(END) + 100).build());
        a1 = ctr.status(u1_a.clone()).unwrap();
        assert_eq!(
            a1.farmed_units.0,
            total_rounds * RATE,
            "after end we should get all rewards"
        );

        testing_env!(ctx.block_timestamp(round(END + 1) + 100).build());
        a1 = ctr.status(u1_a.clone()).unwrap();
        let total_farmed = total_rounds * RATE;
        assert_eq!(
            a1.farmed_units.0, total_farmed,
            "after end there is no more farming"
        );

        // ------------------------------------------------
        // withdraw
        testing_env!(ctx.predecessor_account_id(u1.clone()).build());
        ctr.withdraw_crop();
        a1 = ctr.status(u1_a.clone()).unwrap();
        assert_eq!(
            a1.farmed_units.0, 0,
            "after withdrawing we should have 0 farming units"
        );
        assert_eq!(ctr.total_harvested, vec![20 * E24, 10 * E24]);
    }

    #[test]
    fn test_alone_staking_late() {
        let u1 = acc_u1();
        let u1_a: AccountId = u1.clone().into();
        let t_s1 = acc_staking1(); // token 1
        let t_s2 = acc_staking2();

        let (mut ctx, mut ctr) = setup_contract(u1.clone(), 0, 0);
        finalize(&mut ctr);
        // register user1 account
        testing_env!(ctx.attached_deposit(STORAGE_COST).build());
        ctr.storage_deposit(None, None);

        // ------------------------------------------------
        // stake only one token at round 2
        testing_env!(ctx.block_timestamp(round(1)).build());
        stake(&mut ctx, &mut ctr, &u1, &t_s1, E24 / 10);

        // ------------------------------------------------
        // stake second token in the middle of round 4
        // but firstly verify that we didn't farm anything
        testing_env!(ctx.block_timestamp(round(3) + 100).build());
        let mut a1 = ctr.status(u1_a.clone()).unwrap();
        let mut a1_stake = vec![E24 / 10, 0];
        assert_eq!(to_u128s(&a1.stake_tokens), a1_stake, "a1 stake");
        assert_eq!(a1.farmed_units.0, 0, "need to stake all tokens to farm");

        testing_env!(ctx.block_timestamp(round(4) + 500).build());
        stake(&mut ctx, &mut ctr, &u1, &t_s2, E24 / 10);
        a1 = ctr.status(u1_a.clone()).unwrap();
        a1_stake = vec![E24 / 10, E24 / 10];
        assert_eq!(to_u128s(&a1.stake_tokens), a1_stake, "a1 stake");
        assert_eq!(a1.farmed_units.0, 0, "full round needs to pass to farm");

        // ------------------------------------------------
        // at round 6th, after full round of staking we farm the first tokens!
        testing_env!(ctx.block_timestamp(round(5)).build());
        a1 = ctr.status(u1_a.clone()).unwrap();
        assert_eq!(a1.farmed_units.0, RATE, "full round needs to pass to farm");

        testing_env!(ctx.block_timestamp(round(END)).build());
        a1 = ctr.status(u1_a.clone()).unwrap();
        assert_eq!(
            a1.farmed_units.0,
            6 * RATE,
            "farming form round 5 (including) to 10"
        );
    }

    #[test]
    fn test_staking_2_users() {
        let u1 = acc_u1();
        let u1_a: AccountId = u1.clone().into();
        let u2 = acc_u2();
        let u2_a: AccountId = u2.clone().into();
        // let t_s1 = acc_staking1(); // token 1
        let t_s2 = acc_staking2();
        let (mut ctx, mut ctr) = setup_contract(u1.clone(), 0, 0);
        assert_eq!(
            ctr.total_stake,
            vec![0, 0],
            "at the beginning there should be 0 total stake"
        );
        finalize(&mut ctr);

        // register user1 account and stake  before farming_start
        let a1_stake = vec![4 * E24, 3 * E24];
        register_user_and_stake(&mut ctx, &mut ctr, &u1, &a1_stake, -2);

        // ------------------------------------------------
        // at round 4, user2 registers and stakes
        // firstly register u2 account (storage_deposit) and then stake.
        let a2_stake = vec![E24, E24];
        register_user_and_stake(&mut ctx, &mut ctr, &u2, &a2_stake, 3);

        let mut a1 = ctr.status(u1_a.clone()).unwrap();
        assert_eq!(
            to_u128s(&a1.stake_tokens),
            a1_stake,
            "account1 stake didn't change"
        );
        assert_eq!(
            a1.farmed_units.0,
            3 * RATE,
            "adding new stake doesn't change current issuance"
        );
        assert_eq!(a1.stake.0, 3 * E24 / 10);

        let mut a2 = ctr.status(u2_a.clone()).unwrap();
        assert_eq!(
            to_u128s(&a2.stake_tokens),
            a2_stake,
            "account2 stake got updated"
        );
        assert_eq!(a2.farmed_units.0, 0, "u2 doesn't farm now");
        assert_eq!(a2.stake.0, E24 / 10);

        // ------------------------------------------------
        // 1 epochs later (5th round) user2 should have farming reward
        testing_env!(ctx.block_timestamp(round(4)).build());
        a1 = ctr.status(u1_a.clone()).unwrap();
        assert_eq!(
            to_u128s(&a1.stake_tokens),
            a1_stake,
            "account1 stake didn't change"
        );
        assert_eq!(
            a1.farmed_units.0,
            3 * RATE + RATE * 3 / 4,
            "5th round of account1 farming"
        );

        a2 = ctr.status(u2_a.clone()).unwrap();
        assert_eq!(
            to_u128s(&a2.stake_tokens),
            a2_stake,
            "account1 stake didn't change"
        );
        assert_eq!(a1.stake.0, 3 * E24 / 10);
        assert_eq!(
            a2.farmed_units.0,
            RATE / 4,
            "account2 first farming is correct"
        );

        // ------------------------------------------------
        // go to the last round of farming, and try to stake - it shouldn't change the rewards.
        testing_env!(ctx.block_timestamp(round(END)).build());
        stake(&mut ctx, &mut ctr, &u2, &t_s2, 40 * E24);

        a1 = ctr.status(u1_a.clone()).unwrap();
        assert_eq!(a1.farmed_units.0, 3 * RATE + RATE * 7 * 3 / 4);
        assert_eq!(
            a1.farmed_units.0,
            3 * RATE + 7 * RATE * 3 / 4,
            "last round of account1 farming"
        );

        a2 = ctr.status(u2_a.clone()).unwrap();
        let a2_stake = vec![E24, 41 * E24];
        assert_eq!(
            to_u128s(&a2.stake_tokens),
            a2_stake,
            "account2 stake is updated"
        );
        assert_eq!(
            a2.farmed_units.0,
            7 * RATE / 4,
            "account2 first farming is correct"
        );

        // ------------------------------------------------
        // After farm end farming is disabled
        testing_env!(ctx.block_timestamp(round(END + 2)).build());

        a1 = ctr.status(u1_a.clone()).unwrap();
        assert_eq!(a1.stake.0, 3 * E24 / 10, "account1 stake didn't change");
        assert_eq!(
            a1.farmed_units.0,
            3 * RATE + 7 * RATE * 3 / 4,
            "last round of account1 farming"
        );

        a2 = ctr.status(u2_a.clone()).unwrap();
        assert_eq!(a2.stake.0, E24, "account2 min stake have been updated ");
        assert_eq!(
            a1.farmed_units.0,
            3 * RATE + 7 * RATE * 3 / 4,
            "but there is no more farming"
        );
    }

    #[test]
    fn test_stake_unstake() {
        let u1 = acc_u1();
        let u1_a: AccountId = u1.clone().into();
        let u2 = acc_u2();
        let u2_a: AccountId = u2.clone().into();
        let t_s1 = acc_staking1(); // token 1

        let (mut ctx, mut ctr) = setup_contract(u1.clone(), 0, 0);
        finalize(&mut ctr);

        // ------------------------------------------------
        // register and stake by user1 and user2 - both will stake the same amounts
        let a1_stake = vec![E24, 2 * E24];
        register_user_and_stake(&mut ctx, &mut ctr, &u1, &a1_stake, -2);
        register_user_and_stake(&mut ctx, &mut ctr, &u2, &a1_stake, -2);

        // user1 unstake at round 5
        testing_env!(ctx.block_timestamp(round(4)).build());
        unstake(&mut ctx, &mut ctr, &u1, &t_s1, a1_stake[0]);
        let a1 = ctr.status(u1_a.clone()).unwrap();
        let a2 = ctr.status(u2_a.clone()).unwrap();

        assert_eq!(ctr.total_stake[0], a1_stake[0], "token1 stake was reduced");
        assert_eq!(ctr.total_stake[1], 2 * a1_stake[1], "token2 stake is same");
        assert_eq!(
            a1.farmed_units.0,
            4 / 2 * RATE,
            "user1 and user2 should farm equally in first 4 rounds"
        );
        assert_eq!(
            a2.farmed_units.0,
            4 / 2 * RATE,
            "user1 and user2 should farm equally in first 4 rounds"
        );

        // check at round 7 - user1 should not farm any more
        testing_env!(ctx.block_timestamp(round(6)).build());
        let a1 = ctr.status(u1_a.clone()).unwrap();
        let a2 = ctr.status(u2_a.clone()).unwrap();

        assert_eq!(
            a1.farmed_units.0,
            4 / 2 * RATE,
            "user1 doesn't farm any more"
        );
        assert_eq!(
            a2.farmed_units.0,
            (4 / 2 + 2) * RATE,
            "user2 gets 100% of farming"
        );

        // unstake other tokens
        unstake(&mut ctx, &mut ctr, &u1, &acc_staking2(), a1_stake[1]);
        assert_eq!(ctr.total_stake[0], a1_stake[0], "token1 stake was reduced");
        assert_eq!(ctr.total_stake[1], a1_stake[1], "token2 is reduced");
        assert!(
            ctr.status(u1_a.clone()).is_none(),
            "u1 should be removed when unstaking everything"
        );

        // close accounts
        testing_env!(ctx.block_timestamp(round(7)).build());
        close(&mut ctx, &mut ctr, &u2);
        assert_eq!(ctr.total_stake[0], 0, "token1");
        assert_eq!(ctr.total_stake[1], 0, "token2");
        assert!(
            ctr.status(u2_a.clone()).is_none(),
            "u1 should be removed when unstaking everything"
        );
    }

    #[test]
    fn test_nft_boost() {
        let u1 = acc_u1();
        let u1_a: AccountId = u1.clone().into();
        let u2 = acc_u2();
        let u2_a: AccountId = u2.clone().into();
        let (mut ctx, mut ctr) = setup_contract(u1.clone(), 0, 0);
        finalize(&mut ctr);

        // ------------------------------------------------
        // register and stake by user1 and user2 - both will stake the same amounts,
        // but user1 will have nft boost
        let a1_stake = vec![E24, 2 * E24];
        register_user_and_stake(&mut ctx, &mut ctr, &u1, &a1_stake, -2);
        testing_env!(ctx.predecessor_account_id(acc_nft_cheddy()).build());
        ctr.nft_on_transfer(u1_a.clone(), u1_a.clone(), "1".into(), "".into());
        register_user_and_stake(&mut ctx, &mut ctr, &u2, &a1_stake, -2);

        // check at round 3
        testing_env!(ctx.block_timestamp(round(2)).build());
        let a1 = ctr.status(u1_a.clone()).unwrap();
        let a2 = ctr.status(u2_a.clone()).unwrap();

        assert!(
            a1.farmed_units.0 > 2 / 2 * RATE,
            "user1 should farm more than the 'normal' rate"
        );
        assert!(
            a2.farmed_units.0 < 2 / 2 * RATE,
            "user2 should farm less than the 'normal' rate"
        );

        // withdraw nft during round 3
        testing_env!(ctx
            .predecessor_account_id(u1.clone())
            .block_timestamp(round(2) + 1000)
            .build());
        ctr.withdraw_nft(u1.clone());

        // check at round 4 - user1 should farm at equal rate as user2
        testing_env!(ctx.block_timestamp(round(3)).build());
        let a1_4 = ctr.status(u1_a.clone()).unwrap();
        let a2_4 = ctr.status(u2_a.clone()).unwrap();

        assert_eq!(
            a1_4.farmed_units.0 - a1.farmed_units.0,
            RATE / 2,
            "user1 farming rate is equal to user2"
        );
        assert_eq!(
            a2_4.farmed_units.0 - a2.farmed_units.0,
            RATE / 2,
            "user1 farming rate is equal to user2",
        );
    }

    /*
    #[test]
    fn test_staking_few_users() {
        let user = acc_user1();
        let user_a: AccountId = user.clone().into();
        let user2 = acc_user2();
        let user2_a: AccountId = user2.clone().into();
        let user3 = acc_user3();
        let user3_a: AccountId = user3.clone().into();

        let (mut ctx, mut ctr) =
            setup_contract(user.clone(), STORAGE_COST.try_into().unwrap(), 9, 0);

        // register accounts
        ctr.storage_deposit(None, None);
        ctr.storage_deposit(Some(user2.clone()), None);
        ctr.storage_deposit(Some(user3.clone()), None);
        assert_eq!(
            ctr.total_stake, 0,
            "at the beginning there should be 0 total stake"
        );

        // ------------------------------------------------
        // All users will stake before the farm started
        stake(&mut ctx, &mut ctr, &user, 8 * E24, 9);
        stake(&mut ctx, &mut ctr, &user2, 4 * E24, 9);
        stake(&mut ctx, &mut ctr, &user3, 12 * E24, 9);

        testing_env!(ctx.block_timestamp(10 * ROUND_NS + ROUND_NS_H).build());
        let (a1_s, a1_r, _) = ctr.status(user_a.clone());
        assert_eq!(a1_s.0, 8 * E24, "user stake is correct");
        assert_eq!(a1_r.0, 0, "no cheddar should be rewarded");
        assert_eq!(ctr.total_stake, 24 * E24, "total stake should be correct");

        // ------------------------------------------------
        // After second round all users should have correct rewards
        testing_env!(ctx.block_timestamp(12 * ROUND_NS).build());
        let (s, r, _) = ctr.status(user_a.clone());
        assert_eq!(s.0, 8 * E24, "user1 stake is correct");
        assert_eq!(r.0, 4 * 2 * E24, "cheddar should be rewarded");
        assert_eq!(ctr.total_stake, 24 * E24, "total stake should be correct");

        let (s, r, _) = ctr.status(user2_a.clone());
        assert_eq!(s.0, 4 * E24, "user2 stake is correct");
        assert_eq!(r.0, 2 * 2 * E24, "cheddar should be rewarded");

        let (s, r, _) = ctr.status(user3_a.clone());
        assert_eq!(s.0, 12 * E24, "user3 stake is correct");
        assert_eq!(r.0, 6 * 2 * E24, "cheddar should be rewarded");

        // ------------------------------------------------
        // At round 15 user2 unstakes 2 tokens and withdraws crop.
        testing_env!(ctx
            .block_timestamp(15 * ROUND_NS + ROUND_NS_H)
            .predecessor_account_id(user2.clone())
            .attached_deposit(1)
            .build());
        ctr.unstake((2 * E24).into());
        let (s, r, _) = ctr.status(user2_a.clone());
        assert_eq!(s.0, 2 * E24, "user2 stake should decrease by 2");
        let user2_withdraw = 5 * 2 * E24;
        assert_eq!(
            r.0, user2_withdraw,
            "after stake withdraw, rewards should stay in the account"
        );
        ctr.withdraw_crop();
        let (s, r, _) = ctr.status(user2_a.clone());
        assert_eq!(s.0, 2 * E24, "user2 harvesting shouldn't change the stake");
        assert_eq!(r.0, 0, "after harvest, user rewards should = 0");

        // ------------------------------------------------
        // At the end of the farming we should have a correct state.
        testing_env!(ctx.block_timestamp(22 * ROUND_NS).build());
        let (s, r, _) = ctr.status(user_a.clone());
        let (s2, r2, _) = ctr.status(user2_a.clone());
        let (s3, r3, _) = ctr.status(user3_a.clone());
        assert_eq!(s.0, 8 * E24, "user1 stake didn't change");
        assert_eq!(s2.0, 2 * E24, "user2 stake didn't change");
        assert_eq!(s3.0, 12 * E24, "user3 stake didn't change");

        assert_close(
            r.0 + r2.0 + r3.0 + user2_withdraw,
            12 * E24 * 10,
            "sanity check: total farmed cheddar should work",
        );

        let base = 12 * 5 * E24 / 22;
        assert_close(
            r.0,
            4 * 5 * E24 + base * 8,
            "user1 total cheddar should be correct",
        );

        assert_close(r2.0, base * 2, "cheddar should be rewarded");
        assert_close(r3.0, 6 * 5 * E24 + base * 12, "cheddar should be rewarded");
    }

    */

    // fn assert_close(a: u128, b: u128, msg: &'static str) {
    //     assert!(a * 99999 / 100_000 < b, "{}, {} <> {}", msg, a, b);
    //     assert!(a * 100001 / 100_000 > b, "{}, {} <> {}", msg, a, b);
    // }

    fn to_u128s(v: &Vec<U128>) -> Vec<Balance> {
        v.iter().map(|x| x.0).collect()
    }
}

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
#[allow(unused_imports)]
use near_sdk::serde::{Deserialize, Serialize};

use near_contract_standards::non_fungible_token::TokenId;
use near_sdk::collections::LookupMap;
use near_sdk::json_types::U128;
use near_sdk::{
    assert_one_yocto, env, log, near_bindgen, require, AccountId, Balance, PanicOnDefault, Promise,
    PromiseOrValue, PromiseResult, ONE_YOCTO,
};

use p3_lib::constants::*;
use p3_lib::errors::*;
use p3_lib::helpers::*;
use p3_lib::interfaces::*;

pub mod helpers;
pub mod interfaces;
pub mod storage_management;
pub mod token_standards;
pub mod vault;

use crate::helpers::*;
use crate::interfaces::*;
use crate::vault::*;

/// Implementing the "Scalable Reward Distribution on the Ethereum Blockchain"
/// algorithm:
/// https://uploads-ssl.webflow.com/5ad71ffeb79acc67c8bcdaba/5ad8d1193a40977462982470_scalable-reward-distribution-paper.pdf
#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    /// Status
    pub is_active: bool,
    pub setup_finalized: bool,
    pub owner_id: AccountId,
    /// Treasury address - a destination for the collected fees.
    pub treasury: AccountId,

    /// user vaults
    pub vaults: LookupMap<AccountId, Vault>,

    /// Nft contract ids allowed to stake in farm
    pub stake_nft_tokens: Vec<NftContractId>,
    /// total number of units currently staked
    pub staked_units: u128,
    /// Rate between the staking token and stake units.
    /// When farming the `min(sum(staking_token[i]*stake_rate[i]/1e24))` is taken
    /// to allocate farm_units.
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

    /// NFT contract(s) used for boost
    pub boost_nft_contracts: Vec<NftContractId>,
    /// Cheddy NFT
    pub cheddy: NftContractId,
    /// total number of received boost NFT tokens
    total_boost: Vec<Balance>,
    /// boost when staking NFT from `Contract.boost_nft_contracts` in basis points
    pub nft_boost: u32,
    /// boost when staking NFT from `Contract.boost_nft_contracts` in basis points for Cheddy NFT(s)
    pub cheddy_boost: u32,
    /// total number of harvested farm tokens
    pub total_harvested: Vec<Balance>,
    /// rewards accumulator: running sum of farm_units per token (equals to the total
    /// number of farmed unit tokens).
    reward_acc: u128,
    /// round number when the s was previously updated.
    reward_acc_round: u64,
    /// total amount of currently staked tokens.
    total_stake: Vec<Balance>,
    /// total amount of currently staked Cheddar.
    total_cheddar_stake: Balance,
    /// total number of accounts currently registered.
    pub accounts_registered: u64,
    /// charge in Cheddar from stakers for 1 staked NFT token
    pub cheddar_rate: Balance,
    /// Cheddar contract AccountId
    pub cheddar: AccountId,
}

#[near_bindgen]
impl Contract {
    /// Initializes the contract with the account where the NEP-141 token contract resides, start block-timestamp & rewards_per_year.
    /// Parameters:
    /// * `stake_tokens`: NFT tokens we are staking.
    /// * `farming_start` & `farming_end` are unix timestamps (in seconds).
    /// * `fee_rate`: the Contract.fee parameter (in basis points)
    /// * `cheddar_rate`: charge from stakers per 1 NFT token in Cheddar
    /// * `cheddar`     : Cheddar token account
    /// The farm starts desactivated. To activate, you must send required farming deposits and
    /// call `self.finalize_setup()`.
    #[init]
    pub fn new(
        owner_id: AccountId,
        stake_nft_tokens: Vec<NftContractId>,
        stake_rates: Vec<U128>,
        farm_unit_emission: U128,
        farm_tokens: Vec<AccountId>,
        farm_token_rates: Vec<U128>,
        farming_start: u64,
        farming_end: u64,
        boost_nft_contracts: Vec<NftContractId>,
        cheddy: NftContractId,
        nft_boost: u32,
        cheddy_boost: u32,
        cheddar_rate: U128,
        cheddar: AccountId,
        treasury: AccountId,
    ) -> Self {
        assert!(
            farming_start > env::block_timestamp() / SECOND,
            "start must be in the future"
        );
        assert!(farming_end > farming_start, "End must be after start");
        assert!(cheddar_rate > U128(0), "cheddar_rate should be positive");

        let stake_len = stake_nft_tokens.len();
        let farm_len = farm_tokens.len();
        let boost_len = boost_nft_contracts.len();

        let c = Self {
            is_active: true,
            setup_finalized: false,
            owner_id,
            treasury,
            vaults: LookupMap::new(b"v".to_vec()),
            stake_nft_tokens,
            staked_units: 0,
            stake_rates: stake_rates.iter().map(|x| x.0).collect(),
            farm_tokens,
            farm_token_rates: farm_token_rates.iter().map(|x| x.0).collect(),
            farm_unit_emission: farm_unit_emission.0,
            farm_deposits: vec![0; farm_len],
            farming_start,
            farming_end,
            boost_nft_contracts,
            cheddy,
            total_boost: vec![0; boost_len],
            nft_boost,
            cheddy_boost,
            total_harvested: vec![0; farm_len],
            reward_acc: 0,
            reward_acc_round: 0,
            total_stake: vec![0; stake_len],
            total_cheddar_stake: 0,
            accounts_registered: 0,
            cheddar_rate: cheddar_rate.0,
            cheddar,
        };
        c.check_vectors();
        c
    }

    fn check_vectors(&self) {
        let fl = self.farm_tokens.len();
        let sl = self.stake_nft_tokens.len();
        let bl = self.boost_nft_contracts.len();
        assert!(
            fl == self.farm_token_rates.len()
                && fl == self.total_harvested.len()
                && fl == self.farm_deposits.len(),
            "farm token vector length is not correct"
        );
        assert!(
            sl == self.stake_rates.len() && sl == self.total_stake.len(),
            "stake token vector length is not correct"
        );
        assert!(
            bl == self.total_boost.len(),
            "boost contracts vector length is not correct"
        )
    }

    // ************ //
    // view methods //
    // ************ //

    /// Returns amount of staked NEAR and farmed CHEDDAR of given account.
    pub fn get_contract_params(&self) -> P4ContractParams {
        P4ContractParams {
            owner_id: self.owner_id.clone(),
            stake_tokens: self.stake_nft_tokens.clone(),
            stake_rates: to_U128s(&self.stake_rates),
            farm_unit_emission: self.farm_unit_emission.into(),
            farm_tokens: self.farm_tokens.clone(),
            farm_token_rates: to_U128s(&self.farm_token_rates),
            farm_deposits: to_U128s(&self.farm_deposits),
            is_active: self.is_active,
            farming_start: self.farming_start,
            farming_end: self.farming_end,
            boost_nft_contracts: self.boost_nft_contracts.clone(),
            total_staked: to_U128s(&self.total_stake),
            total_farmed: to_U128s(&self.total_harvested),
            total_boost: to_U128s(&self.total_boost),
            accounts_registered: self.accounts_registered,
            cheddar_rate: U128(self.cheddar_rate),
            cheddar: self.cheddar.clone(),
        }
    }

    pub fn status(&self, account_id: AccountId) -> Option<P4Status> {
        return match self.vaults.get(&account_id) {
            Some(mut v) => {
                let r = self.current_round();
                v.ping(self.compute_reward_acc(r), r);
                // round starts from 1 when now >= farming_start
                let r0 = if r > 1 { r - 1 } else { 0 };
                let farmed = self
                    .farm_token_rates
                    .iter()
                    .map(|rate| U128::from(safe_mul(v.farmed, *rate)))
                    .collect();
                return Some(P4Status {
                    stake_tokens: v.staked,
                    stake: v.min_stake.into(),
                    farmed_units: v.farmed.into(),
                    farmed_tokens: farmed,
                    boost_nfts: v.boost_nft,
                    timestamp: self.farming_start + r0 * ROUND,
                    total_cheddar_staked: v.cheddar_staked.into(),
                });
            }
            None => None,
        };
    }

    // ******************* //
    // transaction methods //
    // ******************* //

    /// withdraw NFT to a destination account using the `nft_transfer` method.
    /// This function is considered safe and will work when contract is paused to allow user
    /// to withdraw his NFTs.
    #[payable]
    pub fn withdraw_boost_nft(&mut self) {
        assert_one_yocto();
        let user = env::predecessor_account_id();
        let mut vault = self.get_vault(&user);
        self._withdraw_boost_nft(&user, &mut vault);
    }

    /// Deposit native near during the setup phase for farming rewards.
    /// Panics when the deposit was already done or the setup is completed.
    #[payable]
    pub fn setup_deposit_near(&mut self) {
        self._setup_deposit(&near(), env::attached_deposit())
    }
    /// FT Receiver `setup deposit` scenario
    /// Panics on failed `unwrap()` if FT not set in `Contract.farm_tokens`
    pub(crate) fn _setup_deposit(&mut self, token: &AccountId, amount: u128) {
        assert!(
            !self.setup_finalized,
            "setup deposits must be done when contract setup is not finalized"
        );
        let token_i = find_acc_idx(token, &self.farm_tokens);
        let total_rounds = round_number(self.farming_start, self.farming_end, self.farming_end);
        let expected = safe_mul(
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

    /// FT Receiver `cheddar stake` scenario
    pub(crate) fn stake_cheddar(&mut self, sender_id: &AccountId, amount: u128) {
        self.assert_is_active();
        let user = sender_id.clone();
        let mut vault = self.get_vault(&user);

        // Expected cheddar for stake per one token
        let expected = self.cheddar_rate;
        assert!(
            expected >= amount,
            "User need at least {} to stake one more token. Got {}",
            self.cheddar_rate,
            amount
        );

        // update vault
        vault.cheddar_staked += amount;
        // update total cheddar staked info
        self.total_cheddar_stake += self.cheddar_rate;
        log!(
            "User stake {} Cheddar, which is required rate for stake 1 NFT token more",
            amount
        );
        self.vaults.insert(&sender_id, &vault);
    }

    /// Unstakes given token and transfers it back to the user.
    /// If there is last staked token in vault - unstake and close the account
    /// NOTE: account once closed must re-register to stake again.
    /// Returns vector of staked tokens left (still staked) after the call.
    /// Panics if the caller doesn't stake anything or if he doesn't have enough staked tokens.
    /// Requires 1 yNEAR payment for wallet 2FA.
    #[payable]
    pub fn unstake(&mut self, nft_contract_id: &NftContractId, token_id: TokenId) -> Vec<TokenId> {
        self.assert_is_active();
        assert_one_yocto();
        let user = env::predecessor_account_id();
        self._nft_unstake(&user, nft_contract_id, token_id)
    }

    /// Unstakes everything and close the account. Sends all farmed tokens using a ft_transfer
    /// and all staked tokens back to the caller.
    /// Panics if the caller doesn't stake anything.
    /// Requires 1 yNEAR payment for wallet validation.
    /// Max unstaking tokens per time limited - 5 tokens (greedy gas).
    #[payable]
    pub fn close(&mut self) {
        self.assert_is_active();
        assert_one_yocto();

        let user = env::predecessor_account_id();
        let mut vault = self.get_vault(&user);

        assert!(
            vault.get_number_of_staked_tokens() <= NFT_UNITS_MAX_TRANSFER_NUM,
            "Because of gas limit for single transaction action is not allowed. 
            You have {} staked NFTs in vault. Max allowed num on close account: {}. 
            Use `unstake` instead",
            vault.get_number_of_staked_tokens(),
            NFT_UNITS_MAX_TRANSFER_NUM
        );

        self.ping_all(&mut vault);
        log!("Closing {} account, farmed: {:?}", &user, vault.farmed);

        // if user doesn't stake anything and has no rewards then we can make a shortcut
        // and remove the account and return storage deposit.
        if vault.is_empty() {
            self.accounts_registered -= 1;
            self.vaults.remove(&user);
            Promise::new(user.clone()).transfer(STORAGE_COST);
            return;
        }

        let units = min_stake(&vault.staked, &self.stake_rates);
        self.staked_units -= units;

        // transfer all tokens to user
        for nft_ctr_idx in 0..self.total_stake.len() {
            let staked_tokens_ids = &vault.staked[nft_ctr_idx];
            for token_idx in 0..staked_tokens_ids.clone().len() {
                self.transfer_staked_nft(
                    user.clone(),
                    nft_ctr_idx,
                    staked_tokens_ids[token_idx].clone(),
                );
            }
        }
        // withdraw farmed to user
        self._withdraw_crop(&user, vault.farmed);

        if !vault.boost_nft.is_empty() {
            self._withdraw_boost_nft(&user, &mut vault);
        }

        if vault.cheddar_staked > 0 {
            self.transfer_staked_cheddar(user.clone(), Some(vault.cheddar_staked));
        }

        // NOTE: we don't return deposit because it will dramatically complicate logic
        // in case we need to recover an account.
        self.accounts_registered -= 1;
        self.vaults.remove(&user);
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
            let amount = safe_mul(farmed_units, self.farm_token_rates[i]);
            self.transfer_farmed_tokens(user, i, amount);
        }
    }

    /** Withdraws harvested `token` to the user, which failed to transfer in a past call,
     *  for example due to missing token registration (some tokens require registration
     *  prior to receiving transfers).
     *  This function doesn't call crop an it doesn't translate outstanding farmed units into
     *  harvested tokens.
     */
    pub fn withdraw_farmed_recovered(&mut self, token: &AccountId) {
        self.assert_is_active();
        let a = env::predecessor_account_id();
        let mut v = self.get_vault(&a);
        let token_i = find_acc_idx(token, &self.farm_tokens);
        let amount = v.farmed_recovered[token_i];
        assert!(amount > 0, "user {} balance is zero", token);
        v.farmed_recovered[token_i] = 0;
        self.transfer_farmed_tokens(&a, token_i, amount);
    }

    // ******************* //
    //     management      //
    // ******************* //

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

    pub fn finalize_setup(&mut self) {
        //self.assert_owner();
        assert!(
            !self.setup_finalized,
            "setup deposits must be done when contract setup is not finalized"
        );
        let now = env::block_timestamp() / SECOND;
        assert!(
            now < self.farming_start - ROUND,
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
        //self.assert_owner();
        let total_rounds = u128::from(round_number(
            self.farming_start,
            self.farming_end,
            self.farming_end,
        ));
        log!("rounds: {}", total_rounds);
        let out = self
            .farm_token_rates
            .iter()
            .map(|rate| safe_mul(total_rounds * self.farm_unit_emission, *rate))
            .collect();
        (to_U128s(&out), to_U128s(&self.farm_deposits))
    }

    /*****************
     * internal methods */

    fn assert_is_active(&self) {
        assert!(self.setup_finalized, "contract is not setup yet");
        assert!(self.is_active, "contract is not active");
    }

    /// Transfers staked(locked) `Cheddar` after successful NFT `unstake` or on `close`.
    /// If account on `close` - withdraw all staked `amount` Cheddar from user `Vault`
    /// Else transfer `amount` equals to `Contract.cheddar_rate` and undeclared in args
    fn transfer_staked_cheddar(&mut self, user: AccountId, amount: Option<Balance>) -> Promise {
        let transfered_amount = if let Some(_amount) = amount {
            // total Cheddar staked amound decreased proportionally current user stake
            U128(_amount)
        } else {
            // total Cheddar staked amound decreased to cheddar rate
            U128(self.cheddar_rate)
        };

        self.total_cheddar_stake -= transfered_amount.0;
        log!(
            "@{} unstake Cheddar locked deposit ( {:?} )",
            user.clone(),
            transfered_amount
        );

        return ext_ft::ext(self.cheddar.clone())
            .with_attached_deposit(ONE_YOCTO)
            .with_static_gas(GAS_FOR_FT_TRANSFER)
            .ft_transfer(
                user.clone(),
                transfered_amount,
                Some("withdraw staked Cheddar".to_string()),
            )
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_CALLBACK)
                    .transfer_staked_cheddar_callback(user.clone(), transfered_amount),
            );
    }

    /// transfers staked NFT tokens (NFT contract identified by an index in
    /// self.stake_tokens) back to the user.
    /// `self.staked_units` must be adjusted in the caller. The callback will fix the
    /// `self.staked_units` if the transfer will fails.
    fn transfer_staked_nft(
        &mut self,
        user: AccountId,
        nft_ctr_idx: usize,
        token_id: TokenId,
    ) -> Promise {
        let nft_contract_id = &self.stake_nft_tokens[nft_ctr_idx];
        log!("unstaking {} token @{}", nft_contract_id, token_id);

        self.total_stake[nft_ctr_idx] -= 1;

        return ext_nft::ext(nft_contract_id.clone())
            .with_attached_deposit(ONE_YOCTO)
            .with_static_gas(GAS_FOR_FT_TRANSFER)
            .nft_transfer(
                user.clone(),
                token_id.clone(),
                None,
                Some("unstaking".to_string()),
            )
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_CALLBACK)
                    .transfer_staked_callback(user, nft_ctr_idx, token_id.clone().into()),
            );
    }

    #[inline]
    fn transfer_farmed_tokens(
        &mut self,
        user: &AccountId,
        token_idx: usize,
        amount: u128,
    ) -> Promise {
        let ft_contract_id = &self.farm_tokens[token_idx];
        log!("transfer farmed token: @{} ", ft_contract_id.clone());
        self.total_harvested[token_idx] += amount;
        self.farm_deposits[token_idx] -= amount;

        if ft_contract_id == &near() {
            return Promise::new(user.clone()).transfer(amount);
        }

        let amount: U128 = amount.into();

        return ext_ft::ext(ft_contract_id.clone())
            .with_attached_deposit(ONE_YOCTO)
            .with_static_gas(GAS_FOR_FT_TRANSFER)
            .ft_transfer(user.clone(), amount, Some("farming".to_string()))
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_CALLBACK)
                    .transfer_farmed_callback(user.clone(), token_idx, amount),
            );
    }

    #[private]
    pub fn transfer_staked_callback(
        &mut self,
        user: AccountId,
        nft_ctr_idx: usize,
        token_id: TokenId,
    ) {
        if promise_result_as_failed() {
            log!(
                "transferring token: {} contract: {}  failed. Recovering account state",
                token_id,
                self.stake_nft_tokens[nft_ctr_idx],
            );

            self.total_stake[nft_ctr_idx] += 1;

            self.recover_state(
                &user,
                true,           // is_staked
                nft_ctr_idx,    // NFT Contract
                Some(token_id), // NFT TokenId
                None,           // no amount - unique token
            );
        }
    }

    // find mistake

    #[private]
    pub fn transfer_farmed_callback(&mut self, user: AccountId, ft_ctr_idx: usize, amount: U128) {
        if promise_result_as_failed() {
            log!(
                "harvesting {} {} token failed. recovering account state",
                amount.0,
                self.farm_tokens[ft_ctr_idx],
            );
            self.recover_state(
                &user,
                false,          // is_staked
                ft_ctr_idx,     // FT Contract
                None,           // no token_ids - FT Contract
                Some(amount.0), // amount of farmed FTs
            );
        }
    }

    #[private]
    pub fn transfer_staked_cheddar_callback(&mut self, user: AccountId, amount: U128) {
        if promise_result_as_failed() {
            log!(
                "transferring Cheddar stake to @{} was failed. Recovering account state",
                user.clone(),
            );
            // recover cheddar
            self.total_cheddar_stake += amount.0;
            let mut v = self.recovered_vault(&user);
            v.cheddar_staked += amount.0;

            self._recompute_stake(&mut v);
            self.vaults.insert(&user, &v);
        }
    }

    #[private]
    pub fn withdraw_boost_nft_callback(
        &mut self,
        user: AccountId,
        contract_and_token_id: ContractNftTokenId,
        nft_ctr_idx: usize,
    ) {
        if promise_result_as_failed() {
            log!(
                "transferring {} boost NFT failed. Recovering account state",
                contract_and_token_id,
            );
            // recover boost NFT
            let mut v = self.recovered_vault(&user);

            self.total_boost[nft_ctr_idx] += 1;

            v.boost_nft = contract_and_token_id;
            self._recompute_stake(&mut v);
            self.vaults.insert(&user, &v);
        }
    }

    fn recovered_vault(&mut self, user: &AccountId) -> Vault {
        match self.vaults.get(user) {
            Some(vault) => vault,
            None => {
                // If the vault was closed before by another TX, then we must recover the state
                self.accounts_registered += 1;
                self.new_vault()
            }
        }
    }

    /// State recovering.
    /// If `is_staked` is `true` - push back NFT token to Vault
    /// Else recover farmed tokens
    fn recover_state(
        &mut self,
        user: &AccountId,
        is_staked: bool,
        contract_i: usize,
        token_id: Option<TokenId>,
        amount: Option<u128>,
    ) {
        let mut v = self.recovered_vault(&user);

        // NFT contract id recovered
        if is_staked {
            v.staked[contract_i].push(token_id.unwrap());
        // FT contract id recovered
        } else {
            let amount = amount.unwrap();
            self.total_harvested[contract_i] -= amount;
            v.farmed_recovered[contract_i] += amount;
        }

        self._recompute_stake(&mut v);
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

    fn new_vault(&self) -> Vault {
        Vault::new(
            self.stake_nft_tokens.len(),
            self.farm_tokens.len(),
            self.reward_acc,
        )
    }

    /// creates new empty account. User must deposit tokens using nft_transfer_call
    fn create_account(&mut self, user: &AccountId) {
        self.vaults.insert(&user, &self.new_vault());
        self.accounts_registered += 1;
    }

    fn assert_owner(&self) {
        assert!(
            env::predecessor_account_id() == self.owner_id,
            "can only be called by the owner"
        );
    }
    /// returns `true` if boost `nft_contract_id` in `Contract.boost_nft_contracts`
    #[allow(unused)]
    fn is_boost_nft_whitelisted(&self, nft_contract_id: &NftContractId) -> bool {
        self.boost_nft_contracts.contains(nft_contract_id)
    }
    #[allow(unused)]
    pub(crate) fn assert_boost_nft(&self, nft_contract_id: &NftContractId) {
        assert!(
            self.boost_nft_contracts.contains(nft_contract_id),
            "NFT contract wasn't whitelisted as stake boost"
        );
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[allow(unused_imports)]
mod tests {
    use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
    use near_contract_standards::non_fungible_token::core::NonFungibleTokenReceiver;
    use near_contract_standards::storage_management::StorageManagement;
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::MockedBlockchain;
    use near_sdk::{testing_env, Balance, Gas};
    use serde::de::IntoDeserializer;
    use std::convert::TryInto;
    use std::vec;

    use super::*;

    fn acc_cheddar() -> AccountId {
        "cheddar".parse().unwrap()
    }

    fn acc_farming2() -> AccountId {
        "farming_token".parse().unwrap()
    }

    fn acc_staking1() -> AccountId {
        "nft1".parse().unwrap()
    }

    fn acc_staking2() -> AccountId {
        "nft2".parse().unwrap()
    }

    fn acc_cheddy_nft() -> AccountId {
        "cheddy_boost".parse().unwrap()
    }

    fn acc_nft_boost() -> AccountId {
        "nft_boost".parse().unwrap()
    }

    fn acc_nft_boost2() -> AccountId {
        "nft_boost2".parse().unwrap()
    }

    fn acc_u1() -> AccountId {
        "user1".parse().unwrap()
    }

    fn acc_u2() -> AccountId {
        "user2".parse().unwrap()
    }

    #[allow(dead_code)]
    fn acc_u3() -> AccountId {
        "user3".parse().unwrap()
    }

    #[allow(dead_code)]
    fn acc_u4() -> AccountId {
        "user4".parse().unwrap()
    }

    fn acc_owner() -> AccountId {
        "user_owner".parse().unwrap()
    }

    /// first and last round
    const END: i64 = 10;
    const RATE: u128 = E24 * 2; // 2 farming_units / round (60s)
    const BOOST: u32 = 250;
    const CHEDDY_BOOST: u32 = 300;
    const CHEDDAR_RATE: u128 = 555 * E24; // Cheddar amount required per 1 token stake

    fn round(r: i64) -> u64 {
        let r: u64 = (10 + r).try_into().unwrap();
        dbg!("current round:{} {} ", r, r * ROUND_NS);
        return r * ROUND_NS;
    }
    /// deposit_dec = size of deposit in e24 to set for the next transacton
    fn setup_contract(
        predecessor: AccountId,
        deposit_dec: u128,
        stake_nft_tokens: Option<Vec<AccountId>>,
        stake_rates: Option<Vec<u128>>,
        farm_unit_emission: u128,
        total_rounds: i64,
    ) -> (VMContextBuilder, Contract) {
        let mut context = VMContextBuilder::new();
        testing_env!(context.build());
        let contract = Contract::new(
            acc_owner(),
            stake_nft_tokens.unwrap_or_else(|| vec![acc_staking1(), acc_staking2()]), // staking nft tokens
            to_U128s(&stake_rates.unwrap_or_else(|| vec![E24, E24 / 10])), // staking rates
            U128(farm_unit_emission),                                      // farm_unit_emission
            vec![acc_cheddar(), acc_farming2()],                           // farming tokens
            to_U128s(&vec![E24, E24 / 2]),                                 // farming rates
            round(0) / SECOND,                                             // farming start
            round(total_rounds) / SECOND,                                  // farming end
            vec![acc_nft_boost(), acc_nft_boost2(), acc_cheddy_nft()],     // boost nft
            acc_cheddy_nft(),                                              // cheddy nft
            BOOST,                                                         // boost rate
            CHEDDY_BOOST,                                                  // cheddy boost rate
            U128(CHEDDAR_RATE), // cheddar charge per 1 staked NFT
            acc_cheddar(),
            accounts(1), // treasury
        );
        contract.check_vectors();
        testing_env!(context
            .predecessor_account_id(predecessor.clone())
            .signer_account_id(predecessor.clone())
            .attached_deposit(deposit_dec.into())
            .block_timestamp(round(-10))
            .build());
        (context, contract)
    }

    fn deposit_cheddar(ctx: &mut VMContextBuilder, ctr: &mut Contract, user: &AccountId) {
        testing_env!(ctx
            .attached_deposit(0)
            .predecessor_account_id(acc_cheddar().clone())
            .signer_account_id(user.clone())
            .build());
        ctr.ft_on_transfer(user.clone(), U128(CHEDDAR_RATE), "cheddar stake".into());
    }

    /// epoch is a timer in rounds (rather than miliseconds)
    fn stake(
        ctx: &mut VMContextBuilder,
        ctr: &mut Contract,
        user: &AccountId,
        nft_token_contract: &AccountId,
        token_id: String,
    ) {
        testing_env!(ctx
            .attached_deposit(0)
            .predecessor_account_id(nft_token_contract.clone())
            .signer_account_id(user.clone())
            .build());
        ctr.nft_on_transfer(user.clone(), user.clone(), token_id, "to farm".to_string());
    }

    /// epoch is a timer in rounds (rather than miliseconds)
    fn unstake(
        ctx: &mut VMContextBuilder,
        ctr: &mut Contract,
        user: &AccountId,
        nft_token_contract: &AccountId,
        token_id: String,
    ) {
        testing_env!(ctx
            .attached_deposit(1)
            .predecessor_account_id(user.clone())
            .prepaid_gas(Gas(300000000000000))
            .build());
        ctr.unstake(nft_token_contract, token_id);
    }

    /// epoch is a timer in rounds (rather than miliseconds)
    fn close(ctx: &mut VMContextBuilder, ctr: &mut Contract, user: &AccountId) {
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
        user: &AccountId,
        nft_contract_id: &AccountId,
        stake_token_id: String,
        r: i64,
    ) {
        testing_env!(ctx
            .attached_deposit(STORAGE_COST)
            .predecessor_account_id(user.clone())
            .signer_account_id(user.clone())
            .block_timestamp(round(r))
            .build());

        ctr.storage_deposit(None, None);

        testing_env!(ctx
            .attached_deposit(ONE_YOCTO)
            .predecessor_account_id(user.clone())
            .signer_account_id(user.clone())
            .block_timestamp(round(r))
            .build());

        deposit_cheddar(ctx, ctr, user);
        stake(ctx, ctr, user, nft_contract_id, stake_token_id);
    }

    #[test]
    fn test_set_active() {
        let (_, mut ctr) = setup_contract(acc_owner(), 5, None, None, RATE, END);
        assert_eq!(ctr.is_active, true);
        ctr.set_active(false);
        assert_eq!(ctr.is_active, false);
    }

    #[test]
    #[should_panic(expected = "can only be called by the owner")]
    fn test_set_active_not_admin() {
        let (_, mut ctr) = setup_contract(accounts(0), 0, None, None, RATE, END);
        ctr.set_active(false);
    }

    fn finalize(ctr: &mut Contract, farm_deposits: Vec<u128>) {
        ctr._setup_deposit(&acc_cheddar().into(), farm_deposits[0]);
        ctr._setup_deposit(&acc_farming2().into(), farm_deposits[1]);
        ctr.finalize_setup();
    }

    #[test]
    fn test_finalize_setup() {
        let (_, mut ctr) = setup_contract(acc_owner(), 0, None, None, RATE, END);
        assert_eq!(
            ctr.setup_finalized, false,
            "at the beginning setup mut not be finalized"
        );
        finalize(&mut ctr, vec![20 * E24, 10 * E24]);
        assert_eq!(ctr.setup_finalized, true)
    }

    #[test]
    #[should_panic(expected = "must be finalized at last before farm start")]
    fn test_finalize_setup_too_late() {
        let (mut ctx, mut ctr) = setup_contract(acc_owner(), 0, None, None, RATE, END);
        ctr._setup_deposit(&acc_cheddar().into(), 20 * E24);
        ctr._setup_deposit(&acc_farming2().into(), 10 * E24);
        testing_env!(ctx.block_timestamp(10 * ROUND_NS).build());
        ctr.finalize_setup();
    }

    #[test]
    #[should_panic(expected = "Expected deposit for token cheddar is 20000000000000000000000000")]
    fn test_finalize_setup_wrong_deposit() {
        let (_, mut ctr) = setup_contract(accounts(1), 0, None, None, RATE, END);
        ctr._setup_deposit(&acc_cheddar().into(), 10 * E24);
    }

    #[test]
    #[should_panic(expected = "Deposit for token farming_token not done")]
    fn test_finalize_setup_not_enough_deposit() {
        let (_, mut ctr) = setup_contract(acc_owner(), 0, None, None, RATE, END);
        ctr._setup_deposit(&acc_cheddar().into(), 20 * E24);
        ctr.finalize_setup();
    }

    #[test]
    fn test_round_number() {
        let (mut ctx, ctr) = setup_contract(acc_u1(), 0, None, None, RATE, END);
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
        let (mut ctx, mut ctr) = setup_contract(acc_u1(), 0, None, None, RATE, END);
        testing_env!(ctx.attached_deposit(STORAGE_COST / 4).build());
        ctr.storage_deposit(None, None);
    }

    #[test]
    fn test_storage_deposit() {
        let user = acc_u1();
        let (mut ctx, mut ctr) = setup_contract(user.clone(), 0, None, None, RATE, END);

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
    fn test_staking_nft_unit() {
        let user_1 = acc_u1();
        let (mut ctx, mut ctr) = setup_contract(
            user_1.clone(),
            0,
            Some(vec![acc_staking1()]),
            Some(vec![E24]),
            RATE,
            END,
        );
        finalize(&mut ctr, vec![20 * E24, 10 * E24]);

        let user_1_stake_token = "some_token_id".to_string();
        let user_1_stake_contract = acc_staking1();
        register_user_and_stake(
            &mut ctx,
            &mut ctr,
            &user_1,
            &user_1_stake_contract,
            user_1_stake_token.clone(),
            2, // round
        );

        let user_1_status = ctr.status(user_1.clone()).unwrap();
        assert_eq!(user_1_status.stake.0, E24, "u1 should have staked units!");
        unstake(
            &mut ctx,
            &mut ctr,
            &user_1,
            &user_1_stake_contract,
            user_1_stake_token.clone(),
        );
        let status = ctr.status(user_1.clone());

        assert!(status.is_none());
        dbg!("{:?}", ctr.get_contract_params());
    }

    #[test]
    #[should_panic]
    fn test_staking_without_cheddar() {
        let user_1 = acc_u1();
        let nft_1 = acc_staking1(); // nft contract 1

        let (mut ctx, mut ctr) = setup_contract(user_1.clone(), 0, None, None, RATE, END);
        finalize(&mut ctr, vec![20 * E24, 10 * E24]);

        assert!(
            ctr.status(user_1.clone()).is_none(),
            "u1 is not registered yet"
        );
        // register user1 account and stake without Cheddar deposited before
        testing_env!(ctx.attached_deposit(STORAGE_COST).build());
        register_user_and_stake(&mut ctx, &mut ctr, &user_1, &nft_1, "token_id_1".into(), -3);

        // panic because not enough deposit
        stake(&mut ctx, &mut ctr, &user_1, &nft_1, "token_id_2".into());
    }

    #[test]
    #[should_panic]
    fn test_staking_not_expected_cheddar() {
        let user_1 = acc_u1();
        let nft_1 = acc_staking1(); // nft contract 1

        let (mut ctx, mut ctr) = setup_contract(user_1.clone(), 0, None, None, RATE, END);
        finalize(&mut ctr, vec![20 * E24, 10 * E24]);

        assert!(
            ctr.status(user_1.clone()).is_none(),
            "u1 is not registered yet"
        );
        // register user1 account and stake without Cheddar deposited before
        testing_env!(ctx.attached_deposit(STORAGE_COST).build());
        register_user_and_stake(&mut ctx, &mut ctr, &user_1, &nft_1, "token_id_1".into(), -3);

        // panic - wrong amount
        testing_env!(ctx
            .attached_deposit(0)
            .predecessor_account_id(acc_cheddar().clone())
            .signer_account_id(user_1.clone())
            .build());
        ctr.ft_on_transfer(
            user_1.clone(),
            U128(CHEDDAR_RATE - 1),
            "cheddar stake".into(),
        );
    }

    #[test]
    #[should_panic(
        expected = "Not enough Cheddar to stake. Required 555000000000000000000000000 of yoctoCheddar for stakeing one more NFT token"
    )]
    fn test_staking_more_cheddar() {
        let user_1 = acc_u1();
        let nft_1 = acc_staking1(); // nft contract 1
        let nft_2 = acc_staking2(); // nft contract 1

        let (mut ctx, mut ctr) = setup_contract(user_1.clone(), 0, None, None, RATE, END);
        finalize(&mut ctr, vec![20 * E24, 10 * E24]);

        assert!(
            ctr.status(user_1.clone()).is_none(),
            "u1 is not registered yet"
        );
        // register user1 account and stake without Cheddar deposited before
        testing_env!(ctx.attached_deposit(STORAGE_COST).build());
        register_user_and_stake(&mut ctx, &mut ctr, &user_1, &nft_1, "token_id_1".into(), -3);

        deposit_cheddar(&mut ctx, &mut ctr, &user_1);
        deposit_cheddar(&mut ctx, &mut ctr, &user_1);
        deposit_cheddar(&mut ctx, &mut ctr, &user_1);
        stake(&mut ctx, &mut ctr, &user_1, &nft_1, "2".into());
        stake(&mut ctx, &mut ctr, &user_1, &nft_2, "3(2)".into());
        stake(&mut ctx, &mut ctr, &user_1, &nft_1, "4".into());
        // too much
        stake(&mut ctx, &mut ctr, &user_1, &nft_1, "5".into());
    }

    #[test]
    fn test_alone_staking() {
        let user_1 = acc_u1();
        let nft_1 = acc_staking1(); // nft contract 1
        let nft_2 = acc_staking2();

        let (mut ctx, mut ctr) = setup_contract(user_1.clone(), 0, None, None, RATE, END);
        finalize(&mut ctr, vec![20 * E24, 10 * E24]);

        assert!(
            ctr.status(user_1.clone()).is_none(),
            "u1 is not registered yet"
        );

        // register user1 account
        testing_env!(ctx.attached_deposit(STORAGE_COST).build());
        ctr.storage_deposit(None, None);
        let mut user_1_status = ctr.status(user_1.clone()).unwrap();

        // NFT contracts as index in staked tokens (contract_i)
        for i in 0..user_1_status.stake_tokens.clone().len() {
            assert!(&user_1_status.stake_tokens[i].is_empty(), "a1 didn't stake");
        }
        assert_eq!(
            user_1_status.farmed_units.0, 0,
            "a1 didn't stake no one NFT"
        );

        // ------------------------------------------------
        // stake before farming_start
        testing_env!(ctx.block_timestamp(round(-3)).build());
        // deposit CHEDDAR RATE cheddar
        deposit_cheddar(&mut ctx, &mut ctr, &user_1);
        stake(
            &mut ctx,
            &mut ctr,
            &user_1,
            &nft_1,
            "some_token_id".to_string(),
        );

        user_1_status = ctr.status(user_1.clone()).unwrap();
        let mut user_1_stake: Vec<Vec<String>> = vec![vec!["some_token_id".to_string()], vec![]];
        assert_eq!(user_1_status.stake_tokens, user_1_stake, "user1 stake");
        assert_eq!(
            user_1_status.total_cheddar_staked.0, CHEDDAR_RATE,
            "user1 cheddar stake"
        );
        assert_eq!(user_1_status.farmed_units.0, 0, "farming didn't start yet");
        assert_eq!(
            ctr.total_stake.len(),
            user_1_stake.len(),
            "total tokens staked should equal to account1 stake."
        );

        // ------------------------------------------------
        // stake one more time before farming_start
        testing_env!(ctx.block_timestamp(round(-2)).build());
        deposit_cheddar(&mut ctx, &mut ctr, &user_1);
        stake(
            &mut ctx,
            &mut ctr,
            &user_1,
            &nft_2,
            "some_token_id_2".to_string(),
        );

        user_1_status = ctr.status(user_1.clone()).unwrap();
        user_1_stake = vec![
            vec!["some_token_id".to_string()],
            vec!["some_token_id_2".to_string()],
        ];
        assert_eq!(user_1_status.stake_tokens, user_1_stake, "user1 stake");
        assert_eq!(
            user_1_status.total_cheddar_staked.0,
            2 * CHEDDAR_RATE,
            "user1 cheddar stake"
        );
        assert_eq!(user_1_status.farmed_units.0, 0, "farming didn't start yet");
        assert_eq!(
            ctr.total_stake.len(),
            user_1_stake.len(),
            "total tokens staked should equal to account1 stake."
        );

        // ------------------------------------------------
        // Staking before the beginning won't yield rewards
        testing_env!(ctx.block_timestamp(round(0) - 1).build());
        user_1_status = ctr.status(user_1.clone()).unwrap();
        assert_eq!(
            user_1_status.stake_tokens, user_1_stake,
            "account1 stake didn't change"
        );
        assert_eq!(
            user_1_status.farmed_units.0, 0,
            "no farmed_units should be rewarded before start"
        );

        // ------------------------------------------------
        // First round - a whole epoch needs to pass first to get first rewards
        testing_env!(ctx.block_timestamp(round(0) + 1).build());
        user_1_status = ctr.status(user_1.clone()).unwrap();
        assert_eq!(
            user_1_status.farmed_units.0, 0,
            "need to stake whole round to farm"
        );

        // ------------------------------------------------
        // 3rd round. We are alone - we should get 100% of emission of first 2 rounds.

        testing_env!(ctx.block_timestamp(round(2)).build());
        user_1_status = ctr.status(user_1.clone()).unwrap();
        assert_eq!(
            user_1_status.stake_tokens, user_1_stake,
            "account1 stake didn't change"
        );
        assert_eq!(
            user_1_status.farmed_units.0,
            2 * RATE,
            "we take all harvest"
        );

        // ------------------------------------------------
        // middle of the 3rd round.
        // second check in same epoch shouldn't change rewards
        testing_env!(ctx.block_timestamp(round(2) + 100).build());
        user_1_status = ctr.status(user_1.clone()).unwrap();
        assert_eq!(
            user_1_status.farmed_units.0,
            2 * RATE,
            "in the same epoch we should harvest only once"
        );
        assert_eq!(
            user_1_status.total_cheddar_staked.0,
            2 * CHEDDAR_RATE,
            "user1 cheddar stake didn't change"
        );

        // ------------------------------------------------
        // last round
        testing_env!(ctx.block_timestamp(round(9)).build());
        let total_rounds: u128 =
            round_number(ctr.farming_start, ctr.farming_end, ctr.farming_end).into();
        user_1_status = ctr.status(user_1.clone()).unwrap();
        assert_eq!(
            user_1_status.farmed_units.0,
            (total_rounds - 1) * RATE,
            "in the last round we should get rewards minus one round"
        );

        // ------------------------------------------------
        // end of farming
        testing_env!(ctx.block_timestamp(round(END) + 100).build());
        user_1_status = ctr.status(user_1.clone()).unwrap();
        assert_eq!(
            user_1_status.farmed_units.0,
            total_rounds * RATE,
            "after end we should get all rewards"
        );

        testing_env!(ctx.block_timestamp(round(END + 1) + 100).build());
        user_1_status = ctr.status(user_1.clone()).unwrap();
        let total_farmed = total_rounds * RATE;
        assert_eq!(
            user_1_status.farmed_units.0, total_farmed,
            "after end there is no more farming"
        );

        // ------------------------------------------------
        // withdraw
        // ------------------------------------------------
        // Before withdraw farm deposits doesn't changed
        assert_eq!(ctr.farm_deposits, vec![20 * E24, 10 * E24]);

        testing_env!(ctx.predecessor_account_id(user_1.clone()).build());
        ctr.withdraw_crop();
        user_1_status = ctr.status(user_1.clone()).unwrap();
        assert_eq!(
            user_1_status.farmed_units.0, 0,
            "after withdrawing we should have 0 farming units"
        );
        // After withdraw there is no farm deposits and full of farmed now harvested
        assert_eq!(ctr.total_harvested, vec![20 * E24, 10 * E24]);
        assert_eq!(ctr.farm_deposits, vec![0, 0]);
        // stake not changed
        assert_eq!(
            user_1_status.stake_tokens, user_1_stake,
            "after withdrawing crop stake not changed"
        );
        assert_eq!(
            user_1_status.total_cheddar_staked.0,
            2 * CHEDDAR_RATE,
            "user1 cheddar stake didn't changed"
        );
    }

    #[test]
    fn test_alone_staking_late() {
        let user_1 = acc_u1();
        let nft1 = acc_staking1(); // nft contract 1
        let nft2 = acc_staking2();

        let (mut ctx, mut ctr) = setup_contract(user_1.clone(), 0, None, None, RATE, END);
        finalize(&mut ctr, vec![20 * E24, 10 * E24]);
        // register user1 account
        testing_env!(ctx.attached_deposit(STORAGE_COST).build());
        ctr.storage_deposit(None, None);

        // ------------------------------------------------
        // stake only one token at round 2
        testing_env!(ctx.block_timestamp(round(1)).build());
        deposit_cheddar(&mut ctx, &mut ctr, &user_1);
        stake(
            &mut ctx,
            &mut ctr,
            &user_1,
            &nft1,
            "some_token_id_1".to_string(),
        );

        // ------------------------------------------------
        // stake second token in the middle of round 4
        // but firstly verify that we didn't farm anything
        testing_env!(ctx.block_timestamp(round(3)).build());
        let mut user_1_status = ctr.status(user_1.clone()).unwrap();

        let mut user_1_stake: Vec<Vec<String>> = vec![vec!["some_token_id_1".to_string()], vec![]];
        assert_eq!(user_1_status.stake_tokens, user_1_stake, "user1 stake");
        assert_eq!(
            user_1_status.farmed_units.0, 0,
            "need to stake all tokens to farm"
        );

        testing_env!(ctx.block_timestamp(round(4) + 500).build());
        deposit_cheddar(&mut ctx, &mut ctr, &user_1);
        stake(
            &mut ctx,
            &mut ctr,
            &user_1,
            &nft2,
            "some_token_id_2".to_string(),
        );
        user_1_status = ctr.status(user_1.clone()).unwrap();
        user_1_stake = vec![
            vec!["some_token_id_1".to_string()],
            vec!["some_token_id_2".to_string()],
        ];
        assert_eq!(user_1_status.stake_tokens, user_1_stake, "user1 stake");
        assert_eq!(
            user_1_status.farmed_units.0, 0,
            "full round needs to pass to farm"
        );
        assert_eq!(
            user_1_status.total_cheddar_staked.0,
            2 * CHEDDAR_RATE,
            "2 tokens staked Cheddar stake equivalent"
        );

        // ------------------------------------------------
        // at round 6th, after full round of staking we farm the first tokens!
        testing_env!(ctx.block_timestamp(round(5)).build());
        user_1_status = ctr.status(user_1.clone()).unwrap();
        assert_eq!(
            user_1_status.farmed_units.0, RATE,
            "full round needs to pass to farm"
        );

        testing_env!(ctx.block_timestamp(round(END)).build());
        user_1_status = ctr.status(user_1.clone()).unwrap();
        assert_eq!(
            user_1_status.farmed_units.0,
            6 * RATE,
            "farming form round 5 (including) to 10"
        );
    }

    #[test]
    fn test_staking_2_users() {
        let user_1: AccountId = acc_u1();
        let user_2: AccountId = acc_u2();
        let nft1 = acc_staking1(); // nft_contract_id 1

        let (mut ctx, mut ctr) = setup_contract(
            acc_owner(),
            0,
            Some(vec![nft1.clone()]),
            Some(vec![E24 / 10]),
            RATE,
            END,
        );
        assert_eq!(
            ctr.total_stake,
            [0],
            "at the beginning there should be 0 total stake"
        );
        finalize(&mut ctr, vec![20 * E24, 10 * E24]);

        // register user1 account and stake before farming_start
        let user_1_stake = vec![vec![
            "some_token_id_1".to_string(),
            "some_token_id_1_1".to_string(),
            "some_token_id_1_2".to_string(),
        ]];
        register_user_and_stake(
            &mut ctx,
            &mut ctr,
            &user_1,
            &nft1,
            user_1_stake.clone()[0].clone()[0].clone(),
            -2,
        );
        deposit_cheddar(&mut ctx, &mut ctr, &user_1);
        stake(
            &mut ctx,
            &mut ctr,
            &user_1,
            &nft1,
            user_1_stake.clone()[0].clone()[1].clone(),
        );
        deposit_cheddar(&mut ctx, &mut ctr, &user_1);
        stake(
            &mut ctx,
            &mut ctr,
            &user_1,
            &nft1,
            user_1_stake.clone()[0].clone()[2].clone(),
        );

        // ------------------------------------------------
        // at round 4, user2 registers and stakes
        // firstly register u2 account (storage_deposit) and then stake.
        let user_2_stake = vec![vec!["some_token_id_2".to_string()]];
        register_user_and_stake(
            &mut ctx,
            &mut ctr,
            &user_2,
            &nft1,
            user_2_stake.clone()[0].clone()[0].clone(),
            3,
        );

        let mut user_1_status = ctr.status(user_1.clone()).unwrap();
        assert_eq!(
            user_1_status.stake_tokens, user_1_stake,
            "account1 stake didn't change"
        );
        assert_eq!(
            user_1_status.farmed_units.0,
            3 * RATE,
            "adding new stake doesn't change current issuance"
        );
        assert_eq!(user_1_status.stake.0, 3 * E24 / 10);
        assert_eq!(user_1_status.total_cheddar_staked.0, 3 * CHEDDAR_RATE);

        let mut user_2_status = ctr.status(user_2.clone()).unwrap();
        assert_eq!(
            user_2_status.stake_tokens, user_2_stake,
            "account2 stake got updated"
        );
        assert_eq!(user_2_status.farmed_units.0, 0, "u2 doesn't farm now");
        assert_eq!(user_2_status.stake.0, E24 / 10);
        assert_eq!(user_2_status.total_cheddar_staked.0, CHEDDAR_RATE);

        // ------------------------------------------------
        // 1 epochs later (5th round) user2 should have farming reward
        testing_env!(ctx.block_timestamp(round(4)).build());
        user_1_status = ctr.status(user_1.clone()).unwrap();
        assert_eq!(
            user_1_status.stake_tokens, user_1_stake,
            "account1 stake didn't change"
        );
        assert_eq!(
            user_1_status.farmed_units.0,
            3 * RATE + RATE * 3 / 4,
            "5th round of account1 farming"
        );
        assert_eq!(
            user_1_status.total_cheddar_staked.0,
            3 * CHEDDAR_RATE,
            "u1 cheddar stake didn't change"
        );

        user_2_status = ctr.status(user_2.clone()).unwrap();
        assert_eq!(
            user_2_status.stake_tokens, user_2_stake,
            "account1 stake didn't change"
        );
        assert_eq!(user_1_status.stake.0, 3 * E24 / 10);
        assert_eq!(
            user_2_status.farmed_units.0,
            RATE / 4,
            "account2 first farming is correct"
        );
        assert_eq!(
            user_2_status.total_cheddar_staked.0, CHEDDAR_RATE,
            "u2 cheddar stake didn't change"
        );

        // ------------------------------------------------
        // go to the last round of farming, and try to stake - it shouldn't change the rewards.
        testing_env!(ctx.block_timestamp(round(END)).build());
        deposit_cheddar(&mut ctx, &mut ctr, &user_2);
        stake(
            &mut ctx,
            &mut ctr,
            &user_2,
            &nft1,
            "some_token_id_3".to_string(),
        );

        user_1_status = ctr.status(user_1.clone()).unwrap();
        assert_eq!(user_1_status.farmed_units.0, 3 * RATE + RATE * 7 * 3 / 4);
        assert_eq!(
            user_1_status.farmed_units.0,
            3 * RATE + 7 * RATE * 3 / 4,
            "last round of account1 farming"
        );
        assert_eq!(
            user_1_status.total_cheddar_staked.0,
            3 * CHEDDAR_RATE,
            "u1 cheddar stake didn't change"
        );

        user_2_status = ctr.status(user_2.clone()).unwrap();
        let user_2_stake: Vec<Vec<String>> = vec![vec![
            "some_token_id_2".to_string(),
            "some_token_id_3".to_string(),
        ]];
        assert_eq!(
            user_2_status.stake_tokens, user_2_stake,
            "account2 stake is updated"
        );
        assert_eq!(
            user_2_status.farmed_units.0,
            7 * RATE / 4,
            "account2 first farming is correct"
        );
        assert_eq!(
            user_2_status.total_cheddar_staked.0,
            2 * CHEDDAR_RATE,
            "u1 cheddar stake didn't change"
        );

        // ------------------------------------------------
        // After farm end farming is disabled
        testing_env!(ctx.block_timestamp(round(END + 2)).build());

        user_1_status = ctr.status(user_1.clone()).unwrap();
        assert_eq!(
            user_1_status.stake.0,
            3 * E24 / 10,
            "account1 stake didn't change"
        );
        assert_eq!(
            user_1_status.farmed_units.0,
            3 * RATE + 7 * RATE * 3 / 4,
            "last round of account1 farming"
        );

        user_2_status = ctr.status(user_2.clone()).unwrap();
        assert_eq!(
            user_2_status.stake.0,
            2 * E24 / 10,
            "account2 min stake have been updated "
        );
        assert_eq!(
            user_1_status.farmed_units.0,
            3 * RATE + 7 * RATE * 3 / 4,
            "but there is no more farming"
        );
    }

    #[test]
    fn test_stake_unstake() {
        let user_1 = acc_u1();
        let user_2 = acc_u2();
        let nft1 = acc_staking1(); // nft contract 1
        let nft2 = acc_staking2();

        let (mut ctx, mut ctr) = setup_contract(user_1.clone(), 0, None, None, RATE, END);
        finalize(&mut ctr, vec![20 * E24, 10 * E24]);

        // -----------------------------------------------
        // register and stake by user1 and user2 - both will stake the same amounts
        let user_1_stake = vec![
            vec!["some_token_id_1".to_string()],
            vec![
                "some_token_id_2".to_string(),
                "some_token_id_2_2".to_string(),
            ],
        ];
        let user_2_stake = vec![
            vec!["some_token_id_3".to_string()],
            vec![
                "some_token_id_4".to_string(),
                "some_token_id_4_2".to_string(),
            ],
        ];

        // user_stake structure explanation

        // [ [token_i, token_i, token_i...], [token_i, token_i, token_i...],... [token_i, token_i, token_i...] ]
        //   ^------nft_contract_j--------^  ^------nft_contract_j--------^     ^------nft_contract_j--------^

        // both users stake same:
        // - one token from nft_contract_1
        // - two tokens from nft_contract_2
        // register users and stake 1 token from nft_contract_1
        // "some_token_id_1" from user1 and "some_token_id_3" from user2
        register_user_and_stake(
            &mut ctx,
            &mut ctr,
            &user_1,
            &nft1,
            user_1_stake.clone()[0].clone()[0].clone(),
            -2,
        );
        register_user_and_stake(
            &mut ctx,
            &mut ctr,
            &user_2,
            &nft1,
            user_2_stake.clone()[0].clone()[0].clone(),
            -2,
        );
        // stake more from both users
        deposit_cheddar(&mut ctx, &mut ctr, &user_1);
        stake(
            &mut ctx,
            &mut ctr,
            &user_1,
            &nft2,
            user_1_stake.clone()[1].clone()[0].clone(),
        );
        deposit_cheddar(&mut ctx, &mut ctr, &user_1);
        stake(
            &mut ctx,
            &mut ctr,
            &user_1,
            &nft2,
            user_1_stake.clone()[1].clone()[1].clone(),
        );

        deposit_cheddar(&mut ctx, &mut ctr, &user_2);
        stake(
            &mut ctx,
            &mut ctr,
            &user_2,
            &nft2,
            user_2_stake.clone()[1].clone()[0].clone(),
        );
        deposit_cheddar(&mut ctx, &mut ctr, &user_2);
        stake(
            &mut ctx,
            &mut ctr,
            &user_2,
            &nft2,
            user_2_stake.clone()[1].clone()[1].clone(),
        );

        assert_eq!(
            ctr.total_stake[0], 2 as u128,
            "token1 stake two NFT tokens for contract nft1 (index = 0"
        );
        assert_eq!(
            ctr.total_stake[1], 4 as u128,
            "token1 stake four NFT tokens for contract nft2 (index = 1"
        );
        assert_eq!(
            ctr.total_cheddar_stake,
            6 * CHEDDAR_RATE,
            "total staked cheddar in contract"
        );

        // user1 unstake at round 5
        testing_env!(ctx.block_timestamp(round(4)).build());
        unstake(
            &mut ctx,
            &mut ctr,
            &user_1,
            &nft1,
            user_1_stake.clone()[0].clone()[0].clone(),
        );
        let user_1_status = ctr.status(user_1.clone()).unwrap();
        let user_2_status = ctr.status(user_2.clone()).unwrap();

        assert_eq!(ctr.total_stake[0], 1 as u128, "token1 stake was reduced");
        assert_eq!(ctr.total_stake[1], 4 as u128, "token2 stake is same");
        assert_eq!(
            ctr.total_cheddar_stake,
            5 * CHEDDAR_RATE,
            "total stake cheddar changed"
        );

        assert_eq!(
            user_1_status.farmed_units.0,
            4 / 2 * RATE,
            "user1 and user2 should farm equally in first 4 rounds"
        );
        assert_eq!(
            user_2_status.farmed_units.0,
            4 / 2 * RATE,
            "user1 and user2 should farm equally in first 4 rounds"
        );

        // check at round 7 - user1 should not farm any more
        testing_env!(ctx.block_timestamp(round(6)).build());
        let user_1_status = ctr.status(user_1.clone()).unwrap();
        let user_2_status = ctr.status(user_2.clone()).unwrap();

        assert_eq!(
            user_1_status.farmed_units.0,
            4 / 2 * RATE,
            "user1 doesn't farm any more"
        );
        assert_eq!(
            user_2_status.farmed_units.0,
            (4 / 2 + 2) * RATE,
            "user2 gets 100% of farming"
        );

        // unstake other tokens
        unstake(
            &mut ctx,
            &mut ctr,
            &user_2,
            &nft1,
            user_2_stake.clone()[0].clone()[0].clone(),
        );
        unstake(
            &mut ctx,
            &mut ctr,
            &user_1,
            &nft2,
            user_1_stake.clone()[1].clone()[0].clone(),
        );
        unstake(
            &mut ctx,
            &mut ctr,
            &user_1,
            &nft2,
            user_1_stake.clone()[1].clone()[1].clone(),
        );

        assert_eq!(ctr.total_stake[0], 0, "token1 stake was reduced");
        assert_eq!(ctr.total_stake[1], 2, "token2 is reduced");
        assert_eq!(
            ctr.total_cheddar_stake,
            2 * CHEDDAR_RATE,
            "total stake cheddar changed"
        );
        assert!(
            ctr.status(user_1.clone()).is_none(),
            "user1 should be removed when unstaking everything"
        );
        assert_eq! {
            ctr.status(user_2.clone()).unwrap().total_cheddar_staked.0,
            ctr.total_cheddar_stake,
            "u2 total staked cheddar equals to all staked and to 2 * CHEDDAR_RATE"
        }

        // close accounts
        testing_env!(ctx.block_timestamp(round(7)).build());
        close(&mut ctx, &mut ctr, &user_2);
        assert_eq!(ctr.total_stake[0], 0, "token1");
        assert_eq!(ctr.total_stake[1], 0, "token2");
        assert!(
            ctr.status(user_2.clone()).is_none(),
            "u1 should be removed when unstaking everything"
        );
    }

    #[test]
    fn test_nft_boost() {
        let user_1: AccountId = acc_u1();
        let user_2: AccountId = acc_u2();
        let nft1: AccountId = acc_staking1();

        let (mut ctx, mut ctr) = setup_contract(
            acc_owner(),
            0,
            Some(vec![nft1.clone()]),
            Some(vec![E24]),
            RATE,
            END,
        );
        finalize(&mut ctr, vec![20 * E24, 10 * E24]);

        // ------------------------------------------------
        // register and stake by user1 and user2 - both will stake the same amounts,
        // but user1 will have nft boost

        let user_1_stake: Vec<Vec<String>> = vec![vec!["some_token".to_string()]];
        register_user_and_stake(
            &mut ctx,
            &mut ctr,
            &user_1,
            &nft1,
            user_1_stake.clone()[0].clone()[0].clone(),
            -2,
        );

        testing_env!(ctx.predecessor_account_id(acc_nft_boost()).build());

        ctr.nft_on_transfer(
            user_1.clone(),
            user_1.clone(),
            "1".into(),
            "to boost".into(),
        );

        register_user_and_stake(
            &mut ctx,
            &mut ctr,
            &user_2,
            &nft1,
            "some_token_2".into(),
            -2,
        );

        // check at round 3
        testing_env!(ctx.block_timestamp(round(2)).build());
        let user_1_status = ctr.status(user_1.clone()).unwrap();
        let user_2_status = ctr.status(user_2.clone()).unwrap();

        assert!(
            user_1_status.farmed_units.0 > 2 / 2 * RATE,
            "user1 should farm more than the 'normal' rate"
        );
        assert!(
            user_2_status.farmed_units.0 < 2 / 2 * RATE,
            "user2 should farm less than the 'normal' rate"
        );
        assert_eq!(
            ctr.status(user_1.clone()).unwrap().boost_nfts,
            "nft_boost@1".to_string(),
            "incorrect boost contract_token_ids"
        );
        assert!(
            ctr.total_boost == vec![1u128, 0u128, 0u128],
            "unexpected boost stake!"
        );

        // withdraw nft during round 3
        testing_env!(ctx
            .predecessor_account_id(user_1.clone())
            .block_timestamp(round(2) + 1000)
            .attached_deposit(1)
            .build());
        ctr.withdraw_boost_nft();

        // check at round 4 - user1 should farm at equal rate as user2
        testing_env!(ctx.block_timestamp(round(3)).build());
        let user_1_status_r4 = ctr.status(user_1.clone()).unwrap();
        let user_2_status_r4 = ctr.status(user_2.clone()).unwrap();

        assert_eq!(
            user_1_status_r4.farmed_units.0 - user_1_status.farmed_units.0,
            RATE / 2,
            "user1 farming rate is equal to user2"
        );
        assert_eq!(
            user_2_status_r4.farmed_units.0 - user_2_status.farmed_units.0,
            RATE / 2,
            "user1 farming rate is equal to user2",
        );
        assert!(
            ctr.status(user_1.clone()).unwrap().boost_nfts.is_empty(),
            "incorrect boost contract_token_ids"
        );
        assert!(
            ctr.total_boost == vec![0u128, 0u128, 0u128],
            "unexpected boost stake!"
        );
    }
    #[test]
    fn test_stake_by_token_id_unstake_all() {
        let user_1: AccountId = acc_u1();
        let nft1: AccountId = acc_staking1();

        let (mut ctx, mut ctr) = setup_contract(
            acc_owner(),
            0,
            Some(vec![nft1.clone()]),
            Some(vec![E24 / 20]),
            RATE,
            END,
        );
        finalize(&mut ctr, vec![20 * E24, 10 * E24]);

        let user_1_stake: Vec<Vec<String>> =
            vec![vec!["some_token".to_string(), "some_token_2".to_string()]];

        register_user_and_stake(
            &mut ctx,
            &mut ctr,
            &user_1,
            &nft1,
            "some_token".to_string(),
            -2,
        );
        deposit_cheddar(&mut ctx, &mut ctr, &user_1);
        stake(
            &mut ctx,
            &mut ctr,
            &user_1,
            &nft1,
            "some_token_2".to_string(),
        );
        let mut user_1_status = ctr.status(user_1.clone()).unwrap();

        assert_eq!(
            user_1_status.stake_tokens, user_1_stake,
            "stake tokens as ids must be equal to vector"
        );
        assert_eq!(
            user_1_status.farmed_units.0, 0,
            "no farmed units before before round 0"
        );
        assert_eq!(
            ctr.total_cheddar_stake,
            2 * CHEDDAR_RATE,
            "staked cheddar on contract"
        );

        // ------------------------------------------------
        // 1 epochs later (5th round) user1 should have farming reward
        testing_env!(ctx.block_timestamp(round(4)).build());
        user_1_status = ctr.status(user_1.clone()).unwrap();
        assert_eq!(
            user_1_status.stake_tokens, user_1_stake,
            "user stake didn't change"
        );
        dbg!("{:?} ", user_1_status.clone());
        assert_eq!(user_1_status.farmed_units.0, 4 * RATE, "farmed units");
        assert_eq!(
            user_1_status.farmed_tokens,
            [U128::from(8 * E24), U128::from(4 * E24)],
            "farmed tokens"
        );
        assert_eq!(user_1_status.stake.0, 2 * E24 / 20, "farmed tokens");

        // unstake all - no token_id declared - go to self.close()
        testing_env!(ctx
            .attached_deposit(1)
            .predecessor_account_id(user_1.clone())
            .build());
        close(&mut ctx, &mut ctr, &user_1);
        assert!(ctr.status(user_1.clone()).is_none(), "account closed");
        assert_eq!(ctr.total_stake[0], 0, "token1 stake was reduced");
        assert_eq!(ctr.total_cheddar_stake, 0, "cheddar stake reduces");
    }
    #[test]
    fn test_stake_longer() {
        let user_1 = acc_u1();
        let user_2 = acc_u2();
        let user_3 = acc_u3();

        let nft_1 = acc_staking1();
        const E23RATE: u128 = RATE / 20;

        // build new contract with 1 staked NFT and 2 farmed tokens
        // for example farm tokens is CHEDDAR and another FARM_TOKEN
        // which costs like `n` and `2n` and have rates for this in `setup_contract` function
        let (mut ctx, mut ctr) = setup_contract(
            acc_owner(),
            0,
            Some(vec![nft_1.clone()]),
            Some(vec![E24]),
            E23RATE,
            20160,
        );

        // finalize with setup deposits
        finalize(&mut ctr, vec![2016 * E24, 2016 / 2 * E24]);

        // user 1 stake will be 2 tokens from nft_1 contract
        let user_1_stake: Vec<Vec<String>> = vec![vec!["1_1".to_string(), "2_1".to_string()]];
        // user 2 stake will be 1 token from nft_1 contract
        let _user_2_stake: Vec<Vec<String>> = vec![vec!["1_2".to_string()]];
        // user 3 stake will be 5 tokens from nft_1 contract
        let user_3_stake: Vec<Vec<String>> = vec![vec![
            "1_3".to_string(),
            "2_3".to_string(),
            "3_3".to_string(),
            "4_3".to_string(),
            "5_3".to_string(),
        ]];

        // all of them staked one token before farming start
        // register storage => deposit 555 Cheddar => stake one token
        register_user_and_stake(&mut ctx, &mut ctr, &user_1, &nft_1, "1_1".to_string(), -2);
        register_user_and_stake(&mut ctx, &mut ctr, &user_2, &nft_1, "1_2".to_string(), -2);
        register_user_and_stake(&mut ctx, &mut ctr, &user_3, &nft_1, "1_3".to_string(), -2);

        let mut user_1_status = ctr.status(user_1.clone()).unwrap();
        let mut user_2_status = ctr.status(user_2.clone()).unwrap();
        let mut user_3_status = ctr.status(user_3.clone()).unwrap();

        assert_eq!(ctr.total_stake[0], 3, "total staked tokens");
        assert_eq!(
            ctr.total_cheddar_stake,
            3 * CHEDDAR_RATE,
            "total staked cheddar on contract"
        );
        assert!(
            user_1_status.farmed_units.0
                + user_2_status.farmed_units.0
                + user_3_status.farmed_units.0
                == 0,
            "zeroed units for all users before farming start"
        );

        // 20160 round ~ 2 weeks (20160 minutes)
        let total_rounds = round_number(ctr.farming_start, ctr.farming_end, ctr.farming_end);
        assert_eq!(total_rounds, 20160);

        // move forward to 1 hour (60 rounds)
        testing_env!(ctx.block_timestamp(round(60)).build());
        // statuses
        user_1_status = ctr.status(user_1.clone()).unwrap();
        user_2_status = ctr.status(user_2.clone()).unwrap();
        user_3_status = ctr.status(user_3.clone()).unwrap();

        assert_eq!(
            ctr.total_stake[0],
            ctr.total_cheddar_stake / CHEDDAR_RATE,
            "staked tokens amount didn't change as like Cheddar stake"
        );
        assert!(
            user_1_status.farmed_units.0
                + user_2_status.farmed_units.0
                + user_3_status.farmed_units.0
                == 60 * E23RATE,
            "for all users its one token and one hour farmed units (60minutes * rate)"
        );
        assert!(
            user_1_status.farmed_tokens[0].0
                + user_2_status.farmed_tokens[0].0
                + user_3_status.farmed_tokens[0].0
                == 60 * E23RATE,
            "farmed token number one have 1/1 rates"
        );
        assert!(
            user_1_status.farmed_tokens[1].0
                + user_2_status.farmed_tokens[1].0
                + user_3_status.farmed_tokens[1].0
                == 60 * E23RATE / 2,
            "farmed token number two have 1/2 rates from token one"
        );
        // ... and stake more for user_1
        // deposit 555 Cheddar
        deposit_cheddar(&mut ctx, &mut ctr, &user_1);
        // stake token
        stake(&mut ctx, &mut ctr, &user_1, &nft_1, "2_1".to_string());
        // check user_1 stake and total_stake in Contract
        user_1_status = ctr.status(user_1.clone()).unwrap();
        assert_eq!(
            user_1_status.stake_tokens, user_1_stake,
            "user_1 now stake all his tokens (2)"
        );
        assert_eq!(
            ctr.total_stake[0], 4,
            "now one more token staked in contract"
        );
        assert_eq!(
            ctr.total_cheddar_stake,
            4 * CHEDDAR_RATE,
            "now one more token staked in contract"
        );

        // move to next round
        testing_env!(ctx.block_timestamp(round(61)).build());

        user_1_status = ctr.status(user_1.clone()).unwrap();
        user_2_status = ctr.status(user_2.clone()).unwrap();
        user_3_status = ctr.status(user_3.clone()).unwrap();

        // Now emission rate per round(minute) which equals to 1 * E24
        // splits to 4 tokens which actually staked in Contract now
        // so, per each staked token we got 1/4 * RATE = 0.25 RATE
        // user 1 have 50% of this - 0.5 RATE per round
        // user 2 and user 3 has 25% both - 0.25 RATE and 0.25 RATE
        // user_1.farmed_units + user_2.farmed_units + user_3.farmed_units per round always equals to RATE
        // Now we have 0.5 + 0.25 + 0.25
        // lets check this out:
        assert_eq!(user_1_status.farmed_units.0 - user_2_status.farmed_units.0, E23RATE / 4, "(50% staked = 2 tokens) user_1 farmed units per round minus (25% staked = 1 token) user_2 farmed units");
        assert_eq!(user_1_status.farmed_units.0 - user_3_status.farmed_units.0, E23RATE / 4, "(50% staked = 2 tokens) user_1 farmed units per round minus (25% staked = 1 token) user_3 farmed units");
        assert_eq!(
            user_1_status.farmed_units.0
                + user_2_status.farmed_units.0
                + user_3_status.farmed_units.0,
            61 * E23RATE,
            "sum of all units is RATE"
        );

        // move to 4 hours from farm was started
        testing_env!(ctx.block_timestamp(round(240)).build());
        assert!(
            ctr.total_boost == vec![0u128, 0u128, 0u128],
            "no boost stake!"
        );

        // uncomment and see cheddar boost work[1]
        // add boost NFT for user 3
        testing_env!(ctx
            .predecessor_account_id(acc_nft_boost())
            .signer_account_id(user_3.clone())
            .build());
        ctr.nft_on_transfer(
            user_3.clone(),
            user_3.clone(),
            "1".into(),
            "to boost".into(),
        );
        // add boost NFT2 for user 2
        testing_env!(ctx
            .predecessor_account_id(acc_nft_boost2())
            .signer_account_id(user_1.clone())
            .build());
        ctr.nft_on_transfer(
            user_1.clone(),
            user_1.clone(),
            "1".into(),
            "to boost".into(),
        );

        // check total stake
        assert_eq!(ctr.total_stake[0], 4);
        assert_eq!(ctr.total_cheddar_stake, 4 * CHEDDAR_RATE);
        assert!(
            ctr.total_boost == vec![1u128, 1u128, 0u128],
            "unexpected boost stake!"
        );
        // move to 25 hours from farm was started
        testing_env!(ctx.block_timestamp(round(1500)).build());
        user_1_status = ctr.status(user_1.clone()).unwrap();
        user_2_status = ctr.status(user_2.clone()).unwrap();
        user_3_status = ctr.status(user_3.clone()).unwrap();

        // uncomment and see cheddar boost work[2]
        assert!(
            user_3_status.farmed_units.0 > user_2_status.farmed_units.0,
            "NFT boost made user_3 more units when both staking same tokens amounts (1)"
        );
        assert_eq!(
            user_3_status.boost_nfts,
            "nft_boost@1".to_string(),
            "incorrect boost contract_token_ids"
        );
        assert_eq!(
            user_1_status.boost_nfts,
            "nft_boost2@1".to_string(),
            "incorrect boost contract_token_ids"
        );
        assert!(
            ctr.total_boost == vec![1u128, 1u128, 0u128],
            "unexpected boost stake!"
        );

        // stake rest 4 tokens from user_3
        deposit_cheddar(&mut ctx, &mut ctr, &user_3);
        stake(&mut ctx, &mut ctr, &user_3, &nft_1, "2_3".to_string());
        deposit_cheddar(&mut ctx, &mut ctr, &user_3);
        stake(&mut ctx, &mut ctr, &user_3, &nft_1, "3_3".to_string());
        deposit_cheddar(&mut ctx, &mut ctr, &user_3);
        stake(&mut ctx, &mut ctr, &user_3, &nft_1, "4_3".to_string());
        deposit_cheddar(&mut ctx, &mut ctr, &user_3);
        stake(&mut ctx, &mut ctr, &user_3, &nft_1, "5_3".to_string());

        //check stake
        assert_eq!(
            ctr.total_stake[0],
            ctr.total_cheddar_stake / CHEDDAR_RATE,
            "+4 tokens and +4 * 555 Cheddar in Contract now"
        );
        //check totals
        assert_eq!(
            user_3_stake,
            ctr.status(user_3.clone()).unwrap().stake_tokens,
            "user1 stake all 5 tokens"
        );

        // second after farm end
        testing_env!(ctx.block_timestamp(round(20160)).build());
        user_1_status = ctr.status(user_1.clone()).unwrap();
        user_2_status = ctr.status(user_2.clone()).unwrap();
        user_3_status = ctr.status(user_3.clone()).unwrap();

        // **** view **** //
        dbg!(
            "units: {:?}\n",
            user_1_status.farmed_units.0
                + user_2_status.farmed_units.0
                + user_3_status.farmed_units.0
        );
        dbg!("status_1 {:?}\n", user_1_status);
        dbg!("status_2 {:?}\n", user_2_status);
        dbg!("status_3 {:?}\n", user_3_status);

        // unstake all - no token_id declared - go to self.close()
        unstake(&mut ctx, &mut ctr, &user_3, &nft_1, "5_3".to_string());
        close(&mut ctx, &mut ctr, &user_3);
        close(&mut ctx, &mut ctr, &user_2);
        close(&mut ctx, &mut ctr, &user_1);

        //check
        let total_emission = 20160 * E23RATE;
        //emission accuracy with boost. F.e :
        //total_harvested_expected(emission) = `2016000000000000000000000000`
        //total_harvested_real = `2015999999320000000000000000`
        let emission_accuracy: u128 = E24 / 1000;
        assert!(
            total_emission - ctr.total_harvested[0] < emission_accuracy,
            "all is harvested"
        );
        assert_eq!(
            ctr.total_cheddar_stake, 0,
            "all cheddar reverts when unstake"
        );
        assert_eq!(ctr.total_stake[0], 0, "all cheddar reverts when unstake");
        assert!(
            ctr.total_boost == vec![0u128, 0u128, 0u128],
            "all boost reverts when unstake!"
        );

        //none statuses for all
        assert!(ctr.status(user_1.clone()).is_none());
        assert!(ctr.status(user_2.clone()).is_none());
        assert!(ctr.status(user_3.clone()).is_none());
    }

    #[test]
    fn test_different_nft_boosts() {
        let user_1: AccountId = acc_u1();
        let user_2: AccountId = acc_u2();
        let user_3: AccountId = acc_u3();
        let user_4: AccountId = acc_u4();
        let nft1: AccountId = acc_staking1();
        // whitelisted nfts for boost with less rates (250)
        let less_rated_boost_1: AccountId = acc_nft_boost();
        let less_rated_boost_2: AccountId = acc_nft_boost2();
        // whitelisted cheddy for boost with more rates (300)
        let cheddy_boost: AccountId = acc_cheddy_nft();

        let (mut ctx, mut ctr) = setup_contract(
            acc_owner(),
            0,
            Some(vec![nft1.clone()]),
            Some(vec![E24]),
            RATE,
            END,
        );
        finalize(&mut ctr, vec![20 * E24, 10 * E24]);

        // ------------------------------------------------
        // register and stake by user1, user2 and use 3 - both will stake the same amounts, but
        // user 1 will have nft cheddy boost
        // user 2 will have another nft boost (less_rated_boost_1)
        // user 3 will have another nft boost (less_rated_boost_2)
        // user 4 will have no boost

        let user_1_stake: Vec<Vec<String>> = vec![vec!["some_token_1".to_string()]];
        let _user_2_stake: Vec<Vec<String>> = vec![vec!["some_token_2".to_string()]];
        let _user_3_stake: Vec<Vec<String>> = vec![vec!["some_token_3".to_string()]];
        let _user_4_stake: Vec<Vec<String>> = vec![vec!["some_token_4".to_string()]];

        register_user_and_stake(
            &mut ctx,
            &mut ctr,
            &user_1,
            &nft1,
            user_1_stake.clone()[0].clone()[0].clone(),
            -2,
        );
        testing_env!(ctx.predecessor_account_id(cheddy_boost).build());
        ctr.nft_on_transfer(
            user_1.clone(),
            user_1.clone(),
            "1".into(),
            "to boost".into(),
        );

        register_user_and_stake(
            &mut ctx,
            &mut ctr,
            &user_2,
            &nft1,
            "some_token_2".into(),
            -2,
        );
        testing_env!(ctx.predecessor_account_id(less_rated_boost_1).build());
        ctr.nft_on_transfer(
            user_2.clone(),
            user_2.clone(),
            "1".into(),
            "to boost".into(),
        );

        register_user_and_stake(
            &mut ctx,
            &mut ctr,
            &user_3,
            &nft1,
            "some_token_3".into(),
            -2,
        );
        testing_env!(ctx.predecessor_account_id(less_rated_boost_2).build());
        ctr.nft_on_transfer(
            user_3.clone(),
            user_3.clone(),
            "1".into(),
            "to boost".into(),
        );

        register_user_and_stake(
            &mut ctx,
            &mut ctr,
            &user_4,
            &nft1,
            "some_token_4".into(),
            -2,
        );

        // check at round 3
        testing_env!(ctx.block_timestamp(round(2)).build());
        let user_1_status = ctr.status(user_1.clone()).unwrap();
        let user_2_status = ctr.status(user_2.clone()).unwrap();
        let user_3_status = ctr.status(user_3.clone()).unwrap();
        let user_4_status = ctr.status(user_4.clone()).unwrap();

        assert!(
            user_1_status.farmed_units.0 > (2 / 4) * RATE,
            "user1 should farm more than the 'normal' rate"
        );
        assert!(
            user_2_status.farmed_units.0 > (2 / 4) * RATE,
            "user2 should farm more than the 'normal' rate"
        );
        assert!(
            user_3_status.farmed_units.0 > (2 / 4) * RATE,
            "user3 should farm more than the 'normal' rate"
        );
        assert!(
            user_4_status.farmed_units.0 < (2 * RATE) / 4, // 2/4 RATE too
            "user4 should farm less than the 'normal' rate"
        );
        assert_eq!(
            user_1_status.boost_nfts,
            "cheddy_boost@1".to_string(),
            "incorrect boost contract_token_ids"
        );
        assert_ne!(
            user_2_status.boost_nfts, user_3_status.boost_nfts,
            "incorrect boost contract_token_ids"
        );
        assert!(
            user_2_status.farmed_units.0 == user_3_status.farmed_units.0,
            "same collection boost nfts - same boost"
        );
        assert!(
            user_1_status.farmed_units.0 > user_2_status.farmed_units.0,
            "cheddy boost provides more rewards than other boost nfts"
        );

        assert!(
            ctr.total_boost == vec![1u128, 1u128, 1u128],
            "unexpected boost stake!"
        );

        // withdraw nft from user_2 during round 3
        testing_env!(ctx
            .predecessor_account_id(user_2.clone())
            .block_timestamp(round(2) + 1000)
            .attached_deposit(1)
            .build());
        ctr.withdraw_boost_nft();

        // check at round 4 - user2 should farm at equal rate as user4
        testing_env!(ctx.block_timestamp(round(3)).build());
        let _user_1_status_r4 = ctr.status(user_1.clone()).unwrap();
        let user_2_status_r4 = ctr.status(user_2.clone()).unwrap();
        let user_3_status_r4 = ctr.status(user_3.clone()).unwrap();
        let user_4_status_r4 = ctr.status(user_4.clone()).unwrap();

        assert_eq!(
            user_2_status_r4.farmed_units.0 - user_2_status.farmed_units.0,
            user_4_status_r4.farmed_units.0 - user_4_status.farmed_units.0,
            "user2 farming rate is equal to user4"
        );
        assert!(
            user_3_status_r4.farmed_units.0 > user_2_status_r4.farmed_units.0,
            "user3 farming rate more then user2 has now"
        );

        assert!(
            ctr.status(user_2.clone()).unwrap().boost_nfts.is_empty(),
            "incorrect boost contract_token_ids"
        );
        dbg!("{:?}", ctr.total_boost.clone());
        assert!(
            ctr.total_boost == vec![0u128, 1u128, 1u128],
            "unexpected boost stake!"
        );
    }
    #[test]
    fn test_unstake_all() {
        let user_1 = acc_u1();

        let nft_1 = acc_staking1();
        let nft_2 = acc_staking2();

        const E23RATE: u128 = RATE / 20;

        // build new contract with 1 staked NFT and 2 farmed tokens
        // for example farm tokens is CHEDDAR and another FARM_TOKEN
        // which costs like `n` and `2n` and have rates for this in `setup_contract` function
        let (mut ctx, mut ctr) = setup_contract(
            acc_owner(),
            0,
            Some(vec![nft_1.clone(), nft_2.clone()]),
            Some(vec![E24, E24 / 2]),
            E23RATE,
            20160,
        );

        // finalize with setup deposits
        finalize(&mut ctr, vec![2016 * E24, 2016 / 2 * E24]);

        register_user_and_stake(&mut ctx, &mut ctr, &user_1, &nft_1, "1".into(), -2);
        assert!(ctr.total_cheddar_stake == 1 * CHEDDAR_RATE, "staked 1 NFT");

        // stake 4 more
        deposit_cheddar(&mut ctx, &mut ctr, &user_1);
        stake(&mut ctx, &mut ctr, &user_1, &nft_2, "2".into());
        deposit_cheddar(&mut ctx, &mut ctr, &user_1);
        stake(&mut ctx, &mut ctr, &user_1, &nft_1, "3".into());
        deposit_cheddar(&mut ctx, &mut ctr, &user_1);
        stake(&mut ctx, &mut ctr, &user_1, &nft_2, "4".into());
        deposit_cheddar(&mut ctx, &mut ctr, &user_1);
        stake(&mut ctx, &mut ctr, &user_1, &nft_2, "5".into());
        deposit_cheddar(&mut ctx, &mut ctr, &user_1);
        stake(&mut ctx, &mut ctr, &user_1, &nft_2, "6".into());
        deposit_cheddar(&mut ctx, &mut ctr, &user_1);
        stake(&mut ctx, &mut ctr, &user_1, &nft_2, "7".into());
        // trying to unstake all from contract 1
        unstake(&mut ctx, &mut ctr, &user_1, &nft_1, "1".into());
        unstake(&mut ctx, &mut ctr, &user_1, &nft_1, "3".into());
        // status check
        let user_1_status = ctr.status(user_1.clone()).unwrap();
        assert!(user_1_status.stake_tokens[0].is_empty());
        assert!(
            user_1_status.stake_tokens[1]
                == Vec::<String>::from([
                    "2".into(),
                    "4".into(),
                    "5".into(),
                    "6".into(),
                    "7".into()
                ]),
            "nft_2 staked should keeped"
        );
        assert!(
            user_1_status.total_cheddar_staked.0 == 5 * CHEDDAR_RATE,
            "2 unstaked, now we have only 5 tokens from nft_2 contract"
        );
        assert_eq!(ctr.total_stake[0], 0, "no tokens for nft_1 contract");
        assert_eq!(ctr.total_stake[1], 5, "5 tokens for nft_2 contract");

        // stake contract 1 (nft_1) again
        deposit_cheddar(&mut ctx, &mut ctr, &user_1);
        stake(&mut ctx, &mut ctr, &user_1, &nft_1, "3".into());

        // trying to unstake all (contract 2)
        unstake(&mut ctx, &mut ctr, &user_1, &nft_2, "2".into());
        unstake(&mut ctx, &mut ctr, &user_1, &nft_2, "4".into());
        unstake(&mut ctx, &mut ctr, &user_1, &nft_2, "5".into());
        unstake(&mut ctx, &mut ctr, &user_1, &nft_2, "6".into());
        unstake(&mut ctx, &mut ctr, &user_1, &nft_2, "7".into());

        // status check
        let user_1_status = ctr.status(user_1.clone()).unwrap();
        dbg!("{:?}", ctr.status(user_1.clone()));

        assert!(user_1_status.stake_tokens[1].is_empty());
        assert!(
            user_1_status.stake_tokens[0] == Vec::<String>::from(["3".into(),]),
            "nft_1 staked should keeped"
        );
        assert!(
            user_1_status.total_cheddar_staked.0 == 1 * CHEDDAR_RATE,
            "5 unstaked, now we have only 1 token from nft_2 contract"
        );
        assert_eq!(ctr.total_stake[0], 1, "1 token for nft_1 contract");
        assert_eq!(ctr.total_stake[1], 0, "no tokens for nft_2 contract");
        close(&mut ctx, &mut ctr, &user_1);
        assert_eq!(ctr.total_stake[0], 0, "1 token for nft_1 contract");
        assert_eq!(ctr.total_stake[1], 0, "no tokens for nft_2 contract");
        assert!(ctr.status(user_1.clone()).is_none());
    }
    #[test]
    fn test_close_max() {
        let user_1 = acc_u1();

        let nft_1 = acc_staking1();
        let nft_2 = acc_staking2();
        const E23RATE: u128 = RATE / 20;

        // build new contract with 1 staked NFT and 2 farmed tokens
        // for example farm tokens is CHEDDAR and another FARM_TOKEN
        // which costs like `n` and `2n` and have rates for this in `setup_contract` function
        let (mut ctx, mut ctr) = setup_contract(
            acc_owner(),
            0,
            Some(vec![nft_1.clone(), nft_2.clone()]),
            Some(vec![E24, E24 / 2]),
            E23RATE,
            20160,
        );

        // finalize with setup deposits
        finalize(&mut ctr, vec![2016 * E24, 2016 / 2 * E24]);

        register_user_and_stake(&mut ctx, &mut ctr, &user_1, &nft_1, "1".into(), -2);
        assert!(ctr.total_cheddar_stake == 1 * CHEDDAR_RATE, "staked 1 NFT");

        // stake 4 more
        deposit_cheddar(&mut ctx, &mut ctr, &user_1);
        stake(&mut ctx, &mut ctr, &user_1, &nft_1, "2".into());
        deposit_cheddar(&mut ctx, &mut ctr, &user_1);
        stake(&mut ctx, &mut ctr, &user_1, &nft_1, "3".into());
        deposit_cheddar(&mut ctx, &mut ctr, &user_1);
        stake(&mut ctx, &mut ctr, &user_1, &nft_1, "4".into());
        deposit_cheddar(&mut ctx, &mut ctr, &user_1);
        stake(&mut ctx, &mut ctr, &user_1, &nft_1, "5".into());
        deposit_cheddar(&mut ctx, &mut ctr, &user_1);
        stake(&mut ctx, &mut ctr, &user_1, &nft_1, "6".into());
        deposit_cheddar(&mut ctx, &mut ctr, &user_1);
        stake(&mut ctx, &mut ctr, &user_1, &nft_2, "7".into());

        assert!(ctr.total_cheddar_stake == 7 * CHEDDAR_RATE, "staked 8 NFT");
        assert!(ctr.total_stake[0] == 6, "staked 4 NFT from nft_1");
        assert!(ctr.total_stake[1] == 1, "staked 3 NFT from nft_2");
        assert!(
            ctr.status(user_1.clone()).unwrap().stake_tokens[0].len()
                + ctr.status(user_1.clone()).unwrap().stake_tokens[1].len()
                == 7,
            "staked 7 NFT"
        );

        // trying to unstake all by close() func call (MAX=7)
        close(&mut ctx, &mut ctr, &user_1);
        assert!(ctr.status(user_1.clone()).is_none());
    }
}

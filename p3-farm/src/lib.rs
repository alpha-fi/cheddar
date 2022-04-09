use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::LookupMap;
use near_sdk::json_types::{ValidAccountId, U128};
use near_sdk::{
    assert_one_yocto, env, log, near_bindgen, AccountId, Balance, PanicOnDefault, Promise,
    PromiseResult,
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
        fee_rate: u32,
        treasury: ValidAccountId,
    ) -> Self {
        assert!(farming_end > farming_start, "End must be after start");
        assert!(stake_rates[0].0 == E24, "stake_rate[0] must be 1e24");
        assert!(
            farm_token_rates[0].0 == E24,
            "farm_token_rates[0] must be 1e24"
        );
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
            stake_rates: to_u128_vec(&self.stake_rates),
            farm_unit_emission: self.farm_unit_emission.into(),
            farm_tokens: self.farm_tokens.clone(),
            farm_token_rates: to_u128_vec(&self.farm_token_rates),
            is_active: self.is_active,
            farming_start: self.farming_start,
            farming_end: self.farming_end,
            total_staked: to_u128_vec(&self.total_stake),
            total_farmed: to_u128_vec(&self.total_harvested),
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
                    .map(|ra| U128::from(farmed_tokens(v.farmed, *ra)))
                    .collect();
                return Some(Status {
                    stake_tokens: to_u128_vec(&v.staked),
                    farmed_units: v.farmed.into(),
                    farmed_tokens: farmed,
                    timestamp: self.farming_start + r0 * ROUND,
                });
            }
            None => None,
        };
    }

    // ******************* //
    // transaction methods //

    pub(crate) fn _setup_deposit(&mut self, token: &AccountId, amount: u128) {
        assert!(
            !self.setup_finalized,
            "setup deposits must be done when contract setup is not finalized"
        );
        let token_i = find_acc_idx(token, &self.farm_tokens);
        let total_rounds = round_number(self.farming_start, self.farming_end, self.farming_end);
        let expected = u128::from(total_rounds) * self.farm_unit_emission;
        assert_eq!(
            self.farm_deposits[token_i], 0,
            "deposit already done for the given token"
        );
        assert_eq!(
            amount, expected,
            "Expected deposit for token {} is {}, got {}",
            self.farm_tokens[token_i], expected, amount
        );
        self.farm_deposits[token_i] += amount;
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
        if all_zeros(&v.staked) && v.farmed == 0 {
            self.vaults.remove(&a);
            Promise::new(a.clone()).transfer(STORAGE_COST);
            return;
        }

        let s = min_stake(&v.staked, &self.stake_rates);
        self.staked_units -= s;
        for i in 0..self.total_stake.len() {
            let amount = v.staked[i];
            self.transfer_staked_tokens(a.clone(), i, amount);
        }
        self._withdraw_crop(&a, v.farmed);
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
        to_u128_vec(&self.fee_collected)
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
    pub fn set_end(&mut self, end: u64) {
        self.assert_owner();
        assert!(end > self.farming_start, "End must be after start");
        self.farming_end = end;
    }

    pub fn finalize_setup(&mut self) {
        assert!(
            !self.setup_finalized,
            "setup deposits must be done when contract setup is not finalized"
        );
        let now = env::block_timestamp() / SECOND;
        assert!(
            now < self.farming_start - 12 * 3600,
            "must be finalized at last 12h before farm start"
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
        self.total_stake[token_i] -= amount;
        let fee = amount * self.fee_rate / 10_000;
        let amount = amount - fee;
        let token = &self.stake_tokens[token_i];
        log!("unstaking {} {}", amount, token);
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
                log!("tokens withdrawn {}", amount.0);
                // we can't remove the vault here, because we don't know if `mint` succeded
                //  if it didn't succed, the the mint_callback will try to recover the vault
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
                self.fee_collected[token_i] += fee.0;
                let full_amount = amount.0 + fee.0;
                self.total_stake[token_i] -= full_amount;
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
    use std::convert::TryInto;
    use std::vec;

    use super::*;

    fn acc_cheddar() -> ValidAccountId {
        "cheddar".to_string().try_into().unwrap()
    }

    fn acc_farming2() -> ValidAccountId {
        "cheddar2".to_string().try_into().unwrap()
    }

    fn acc_staking() -> ValidAccountId {
        "atom".try_into().unwrap()
    }

    fn acc_staking2() -> ValidAccountId {
        "atom2".try_into().unwrap()
    }

    fn acc_user1() -> ValidAccountId {
        "user1".try_into().unwrap()
    }

    fn acc_user2() -> ValidAccountId {
        "user2".try_into().unwrap()
    }

    fn acc_user3() -> ValidAccountId {
        "user3".try_into().unwrap()
    }

    /// block round length in nanoseconds.
    const B_ROUND: u64 = ROUND * SECOND;
    /// half of the block round
    const B_ROUND_H: u64 = B_ROUND / 2;
    const RATE: u128 = E24 / 10;

    /// deposit_dec = size of deposit in e24 to set for the next transacton
    fn setup_contract(
        predecessor: ValidAccountId,
        deposit_dec: u128,
        round: u64,
        fee_rate: u32,
    ) -> (VMContextBuilder, Contract) {
        let mut context = VMContextBuilder::new();
        testing_env!(context.build());
        let contract = Contract::new(
            accounts(0), // owner
            vec![acc_staking(), acc_staking2()],
            to_u128_vec(&vec![E24, E24 / 10]),    // staking rates
            RATE.into(),                          // farm_unit_emission
            vec![acc_cheddar(), acc_farming2()],  // farming tokens
            to_u128_vec(&vec![2 * E24, E24 / 2]), // farming rates
            10 * ROUND,                           // farming_start
            20 * ROUND,                           // farming end
            fee_rate,
            accounts(1), // treasury
        );
        contract.check_vectors();
        testing_env!(context
            .predecessor_account_id(predecessor)
            .attached_deposit((deposit_dec).into())
            .block_timestamp(round * B_ROUND)
            .build());
        (context, contract)
    }

    /// epoch is a timer in rounds (rather than miliseconds)
    fn stake(
        ctx: &mut VMContextBuilder,
        ctr: &mut Contract,
        a: &ValidAccountId,
        amount: u128,
        epoch: u64,
    ) {
        testing_env!(ctx
            .attached_deposit(0)
            .predecessor_account_id(acc_staking())
            .block_timestamp(epoch * B_ROUND)
            .build());
        ctr.ft_on_transfer(a.clone(), amount.into(), "transfer to farm".to_string());
    }

    #[test]
    fn test_set_active() {
        let (_, mut ctr) = setup_contract(accounts(0), 5, 1, 0);
        assert_eq!(ctr.is_active, true);
        ctr.set_active(false);
        assert_eq!(ctr.is_active, false);
    }

    #[test]
    #[should_panic(expected = "can only be called by the owner")]
    fn test_set_active_not_admin() {
        let (_, mut ctr) = setup_contract(accounts(1), 0, 1, 0);
        ctr.set_active(false);
    }

    #[test]
    #[should_panic(
        expected = "The attached deposit is less than the minimum storage balance (50000000000000000000000)"
    )]
    fn test_min_storage_deposit() {
        let (mut ctx, mut ctr) = setup_contract(acc_user1(), 0, 1, 0);
        testing_env!(ctx.attached_deposit(STORAGE_COST / 4).build());
        ctr.storage_deposit(None, None);
    }

    #[test]
    fn test_storage_deposit() {
        let user = acc_user1();
        let (mut ctx, mut ctr) = setup_contract(user.clone(), 0, 1, 0);

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

    /*

       #[test]
       fn test_staking() {
           let user = acc_user1();
           let user_a: AccountId = user.clone().into();
           let (mut ctx, mut ctr) = setup_contract(user.clone(), 0, 1, 0);
           assert_eq!(
               ctr.total_stake, 0,
               "at the beginning there should be 0 total stake"
           );

           // register an account
           testing_env!(ctx.attached_deposit(STORAGE_COST).build());
           ctr.storage_deposit(None, None);

           // ------------------------------------------------
           // stake before farming_start
           stake(&mut ctx, &mut ctr, &user, E24, 1);
           let (a1_s, a1_r, _) = ctr.status(get_acc(2)); // returns (stake, rewards, round)
           assert_eq!(a1_s.0, 0, "account0 didn't stake");
           assert_eq!(a1_r.0, 0, "account0 didn't stake so no cheddar");

           let (a1_s, a1_r, _) = ctr.status(user_a.clone());
           assert_eq!(a1_s.0, E24, "user stake");
           assert_eq!(
               ctr.total_stake, a1_s.0,
               "total stake should equal to account1 stake"
           );
           assert_eq!(a1_r.0, 0, "no cheddar should be rewarded before start");

           // ------------------------------------------------
           // stake one more time before farming_start
           stake(&mut ctx, &mut ctr, &user, 3 * E24, 2);

           let (a1_s, a1_r, _) = ctr.status(user_a.clone());
           assert_eq!(a1_s.0, 4 * E24, "user stake increased");
           assert_eq!(a1_r.0, 0, "no cheddar should be rewarded before start");
           assert_eq!(
               ctr.total_stake, a1_s.0,
               "total stake should equal to the user stake"
           );

           // ------------------------------------------------
           // Staking before the beginning won't yield rewards
           testing_env!(ctx.block_timestamp(10 * B_ROUND - 1).build());
           let (a1_s, a1_r, _) = ctr.status(user_a.clone());
           assert_eq!(a1_s.0, 4 * E24, "account1 stake didn't change");
           assert_eq!(a1_r.0, 0, "no cheddar should be rewarded before start");

           // ------------------------------------------------
           // The first round already reward - a whole epoch needs to pass first
           testing_env!(ctx.block_timestamp(10 * B_ROUND + 1).build());
           let (a1_s, a1_r, _) = ctr.status(user_a.clone());
           assert_eq!(a1_s.0, 4 * E24, "account1 stake didn't change");
           assert_eq!(
               a1_r.0, 0,
               "no cheddar should be rewarded during the first round"
           );

           // ------------------------------------------------
           // WE are alone - we should get 100% of emission.

           testing_env!(ctx.block_timestamp(12 * B_ROUND).build());
           let (a1_s, a1_r, _) = ctr.status(user_a.clone());
           assert_eq!(a1_s.0, 4 * E24, "account1 stake didn't change");
           assert_eq!(a1_r.0, 2 * RATE, "we take all harvest");

           // ------------------------------------------------
           // second check in same epoch shouldn't change rewards
           testing_env!(ctx.block_timestamp(12 * B_ROUND + 100).build());
           let (a1_s, a1_r, _) = ctr.status(user_a.clone());
           assert_eq!(a1_s.0, 4 * E24, "account1 stake didn't change");
           assert_eq!(
               a1_r.0,
               2 * RATE,
               "in the same epoch we should harvest only once"
           );

           // ------------------------------------------------
           // 2 epochs later user1 stake
           stake(&mut ctx, &mut ctr, &user, a1_s.0, 13);
           testing_env!(ctx.block_timestamp(13 * B_ROUND + 100).build());
           let (a1_s, a1_r, _) = ctr.status(user_a.clone());
           assert_eq!(a1_s.0, 8 * E24, "account1 stake didn't change");
           assert_eq!(
               a1_r.0,
               3 * RATE,
               "adding new stake shouldn't change issuance in 'self farmin' scenario"
           );

           // ------------------------------------------------
           // User who didn't stake should have zero rewards
           let user2 = acc_user2();
           let user2_a: AccountId = user2.clone().into();
           let (a2_s, a2_r, _) = ctr.status(user2_a.clone());
           assert_eq!(a2_s.0, 0, "account2 stake should be zero");
           assert_eq!(a2_r.0, 0, "account2 rewards should be zero");

           // ------------------------------------------------
           // User2 joins, but his stake will only be taken into account for the next round.
           // register an account
           testing_env!(ctx
               .attached_deposit(STORAGE_COST)
               .predecessor_account_id(user2.clone())
               .block_timestamp(14 * B_ROUND)
               .build());
           ctr.storage_deposit(None, None);

           stake(&mut ctx, &mut ctr, &user2, 4 * E24, 14);
           let (a2_s, a2_r, _) = ctr.status(user2_a.clone());
           assert_eq!(a2_s.0, 4 * E24, "account2 stake should be updated");
           assert_eq!(a2_r.0, 0, "account2 rewards should be still zero");

           let (a1_s, a1_r, _) = ctr.status(user_a.clone());
           assert_eq!(a1_s.0, 8 * E24, "account1 stake didn't change");
           assert_eq!(a1_r.0, 4 * RATE, "all rewards should still go to user1");

           // ------------------------------------------------
           // 1 epochs later account 2 should have farming reward
           testing_env!(ctx.block_timestamp(15 * B_ROUND).build());

           let (a1_s, a1_r, _) = ctr.status(user_a.clone());
           assert_eq!(a1_s.0, 8 * E24, "account1 stake didn't change");
           assert_eq!(
               a1_r.0,
               4 * RATE + RATE * 2 / 3,
               "5th round of account1 farming"
           );

           let (a2_s, a2_r, _) = ctr.status(user2_a.clone());
           assert_eq!(a2_s.0, 4 * E24, "account2 didn't change");
           assert_eq!(a2_r.0, RATE / 3, "account2 first farming is correct");

           // ------------------------------------------------
           // go to the last round of farming, and try to stake - it shouldn't change the rewards.
           stake(&mut ctx, &mut ctr, &user2, 4 * E24, 20);

           let (a1_s, a1_r, _) = ctr.status(user_a.clone());
           assert_eq!(a1_s.0, 8 * E24, "account1 stake didn't change");
           assert_eq!(
               a1_r.0,
               4 * RATE + 6 * RATE * 2 / 3,
               "last round of account1 farming"
           );

           let (a2_s, a2_r, _) = ctr.status(user2_a.clone());
           assert_eq!(a2_s.0, 8 * E24, "account2 stake is updated");
           assert_eq!(a2_r.0, 6 * RATE / 3, "account2 first farming is correct");

           assert_eq!(
               ctr.total_stake,
               a1_s.0 + a2_s.0,
               "total stake should equal to sum of  user stake"
           );

           // ------------------------------------------------
           // After farm end farming is disabled
           testing_env!(ctx.block_timestamp(21 * B_ROUND + 100).build());
           let (a1_s, a1_r, _) = ctr.status(user_a.clone());
           assert_eq!(a1_s.0, 8 * E24, "account1 stake didn't change");
           assert_eq!(
               a1_r.0,
               4 * RATE + 6 * RATE * 2 / 3,
               "last round of account1 farming"
           );

           let (a2_s, a2_r, _) = ctr.status(user2_a.clone());
           assert_eq!(a2_s.0, 8 * E24, "account2 stake is updated");
           assert_eq!(a2_r.0, 6 * RATE / 3, "account2 first farming is correct");
       }

    */

    /*
    #[test]
    fn test_staking_late() {
        let user = acc_user1();
        let user_a: AccountId = user.clone().into();
        let (mut ctx, mut ctr) = setup_contract(user.clone(), 0, 1, 0);
        assert_eq!(
            ctr.total_stake, 0,
            "at the beginning there should be 0 total stake"
        );

        // register an account
        testing_env!(ctx
            .attached_deposit(STORAGE_COST)
            .block_timestamp(15 * ROUND)
            .build());
        ctr.storage_deposit(None, None);

        stake(&mut ctx, &mut ctr, &user, E24, 15);
        let (a1_s, a1_r, _) = ctr.status(user_a.clone());
        assert_eq!(a1_s.0, E24, "user stake");
        assert_eq!(
            ctr.total_stake, a1_s.0,
            "total stake should equal to account1 stake"
        );
        assert_eq!(
            a1_r.0, 0,
            "no cheddar should be rewarded in a round when user joins the pool"
        );

        // in a subsequent round we should farm!
        testing_env!(ctx.block_timestamp(16 * B_ROUND + 100).build());
        let (a1_s, a1_r, _) = ctr.status(user_a.clone());
        assert_eq!(a1_s.0, E24, "account1 stake didn't change");
        assert_eq!(a1_r.0, RATE, "account1 farming");

        assert_eq!(
            ctr.total_stake, a1_s.0,
            "total stake should equal to the user  user stake"
        );
    }

    #[test]
    fn test_staking_late_join() {
        let user = acc_user1();
        let user_a: AccountId = user.clone().into();
        let (mut ctx, mut ctr) =
            setup_contract(user.clone(), STORAGE_COST.try_into().unwrap(), 14, 0);

        // register an account
        ctr.storage_deposit(None, None);
        assert_eq!(
            ctr.total_stake, 0,
            "at the beginning there should be 0 total stake"
        );

        // ------------------------------------------------
        // user joins after the farm started
        stake(&mut ctx, &mut ctr, &user, 2 * E24, 15);

        testing_env!(ctx.block_timestamp(15 * B_ROUND + B_ROUND_H).build());
        let (a1_s, a1_r, _) = ctr.status(user_a.clone());
        assert_eq!(a1_s.0, 2 * E24, "user stake is correct");
        assert_eq!(
            a1_r.0, 0,
            "no cheddar should be rewarded during the first round of staking"
        );
        assert_eq!(
            ctr.total_stake, a1_s.0,
            "total stake should equal to the user stake"
        );

        testing_env!(ctx.block_timestamp(16 * B_ROUND).build());
        let (_, a1_r, _) = ctr.status(user_a.clone());
        assert_eq!(a1_r.0, RATE, "One round farming should be allocated");
    }

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

        testing_env!(ctx.block_timestamp(10 * B_ROUND + B_ROUND_H).build());
        let (a1_s, a1_r, _) = ctr.status(user_a.clone());
        assert_eq!(a1_s.0, 8 * E24, "user stake is correct");
        assert_eq!(a1_r.0, 0, "no cheddar should be rewarded");
        assert_eq!(ctr.total_stake, 24 * E24, "total stake should be correct");

        // ------------------------------------------------
        // After second round all users should have correct rewards
        testing_env!(ctx.block_timestamp(12 * B_ROUND).build());
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
            .block_timestamp(15 * B_ROUND + B_ROUND_H)
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
        testing_env!(ctx.block_timestamp(22 * B_ROUND).build());
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

    fn get_acc(idx: usize) -> AccountId {
        accounts(idx).as_ref().to_string()
    }

    fn assert_close(a: u128, b: u128, msg: &'static str) {
        assert!(a * 99999 / 100_000 < b, "{}, {} <> {}", msg, a, b);
        assert!(a * 100001 / 100_000 > b, "{}, {} <> {}", msg, a, b);
    }
}

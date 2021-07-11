use std::convert::TryInto;

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::LookupMap;
use near_sdk::json_types::{ValidAccountId, U128};
use near_sdk::{
    assert_one_yocto, env, near_bindgen, AccountId, PanicOnDefault, Promise, PromiseResult,
};

pub mod constants;
pub mod errors;
pub mod interfaces;
pub mod util;
pub mod vault;

use crate::interfaces::*;
use crate::{constants::*, errors::*, util::*, vault::*};

near_sdk::setup_alloc!();

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    pub owner_id: AccountId,
    /// farm token
    pub cheddar_id: AccountId,
    /// staked token
    pub token_id: AccountId,
    // if farming is opened
    pub is_active: bool,
    //user vaults
    pub vaults: LookupMap<AccountId, Vault>,
    /// amount of $CHEDDAR farmed during each round. Round duration is defined in constants.rs
    /// Farmed $CHEDDAR are distributed to all users proportionally to their NEAR stake.
    pub rate: u128, //cheddar per round per near (round = 1 second)
    /// farming starts in unix timestamp (seconds).
    pub farming_start: u64,
    /// farming ends (first time with no farming) in unix timestamp (seconds).
    pub farming_end: u64,
    /// total number of farmed and withdrawn rewards
    pub total_rewards: u128,
    pub rounds: Vec<u128>,
}

#[near_bindgen]
impl Contract {
    /// Initializes the contract with the account where the NEP-141 token contract resides, start block-timestamp & rewards_per_year.
    /// Parameters:
    /// * `farming_start` & `farming_end` are unix timestamps
    /// * `reward_rate` is amount of yoctoCheddars per 1 NEAR (1e24 yNEAR)
    #[init]
    pub fn new(
        owner_id: ValidAccountId,
        cheddar_id: ValidAccountId,
        token_id: ValidAccountId,
        reward_rate: U128,
        farming_start: u64,
        farming_end: u64,
    ) -> Self {
        assert!(
            farming_end > farming_start,
            "Start must be after end, end at there must be at least one round difference"
        );
        let num_rounds: usize = (farming_end - farming_start).try_into().unwrap();
        Self {
            owner_id: owner_id.into(),
            cheddar_id: cheddar_id.into(),
            token_id: token_id.into(),
            is_active: true,
            vaults: LookupMap::new(b"v".to_vec()),
            rate: reward_rate.0, //cheddar per round per near (round = 1 second)
            total_rewards: 0,
            farming_start,
            farming_end,
            rounds: vec![0u128; num_rounds],
        }
    }

    // ************ //
    // view methods //

    /// Returns amount of staked NEAR and farmed CHEDDAR of given account.
    pub fn get_contract_params(&self) -> ContractParams {
        let r = self.current_round();
        let staked = self.rounds.get(r).unwrap_or(&0);
        ContractParams {
            owner_id: self.owner_id.clone(),
            farming_token: self.cheddar_id.clone(),
            staked_token: self.token_id.clone(),
            farming_rate: self.rate.into(),
            round_len: ROUND,
            is_open: self.is_active,
            farming_start: self.farming_start,
            farming_end: self.farming_end,
            staked_in_round: (*staked).into(),
        }
    }

    /// Returns amount of staked NEAR, farmed CHEDDAR and the current round.
    pub fn status(&self, account_id: AccountId) -> (U128, U128, usize) {
        return match self.vaults.get(&account_id) {
            Some(mut v) => (
                v.staked.into(),
                self.ping(&mut v).into(),
                self.current_round(),
            ),
            None => {
                let zero = U128::from(0);
                return (zero, zero, 0);
            }
        };
    }

    // ******************* //
    // transaction methods //

    /// Unstakes given amount of tokens and transfers it back to the user.
    /// If amount is >= then the amount staked then we close the account. NOTE: account once
    /// closed must re-register to stake again.
    /// Returns amount of staked tokens left after the call.
    /// Panics if the caller doesn't stake anything or if he doesn't have enough staked tokens.
    /// Requires 1 yNEAR payment for wallet 2FA.
    #[payable]
    pub fn unstake(&mut self, amount: U128) -> Promise {
        assert_one_yocto();
        let amount_u = u128::from(amount);
        let (a, mut vault) = self.get_vault();
        assert!(
            amount_u <= vault.staked + MIN_BALANCE,
            "Invalid amount, you have not that much staked"
        );
        if amount_u >= vault.staked {
            //unstake all => close -- simplify UI
            return self.close();
        }
        self._unstake(amount_u, &mut vault);
        self.vaults.insert(&a, &vault);
        self.return_tokens(a.clone(), amount)
            .then(ext_self::return_tokens_callback(
                a,
                amount,
                &env::current_account_id(),
                0,
                GAS_FOR_MINT_CALLBACK,
            ))
    }

    /// Unstakes everything and close the account. Sends all farmed CHEDDAR using a ft_transfer
    /// and all NEAR to the caller.
    /// Returns amount of farmed CHEDDAR.
    /// Panics if the caller doesn't stake anything.
    /// Requires 1 yNEAR payment for wallet validation.
    #[payable]
    pub fn close(&mut self) -> Promise {
        assert_one_yocto();
        let (aid, mut vault) = self.get_vault();
        self.ping(&mut vault);
        env_log!(
            "Closing {} account, farmed CHEDDAR: {}",
            &aid,
            vault.rewards
        );
        // if user doesn't stake anything and has no rewards then we can make a shortcut
        // and remove the account and return storage deposit.
        if vault.staked == 0 && vault.rewards == 0 {
            self.vaults.remove(&aid);
            return Promise::new(aid.clone()).transfer(MIN_BALANCE);
        }

        let rewards_str: U128 = vault.rewards.into();
        let staked = vault.staked;
        // note: r==1 at start
        let r = self.current_round();
        self.rounds[r] -= staked;

        // We update the values here, and recover in a callback if minting fails
        vault.staked = 0;
        vault.rewards = 0;
        self.vaults.insert(&aid.clone(), &vault);
        return self.mint_cheddar(&aid, rewards_str, staked.into(), true);
    }

    /// Withdraws all farmed CHEDDAR to the user. It doesn't close the account.
    /// Call `close` to remove the account and return all NEAR deposit.
    /// Return amount of farmed CHEDDAR.
    /// Panics if user has not staked anything.
    #[payable]
    pub fn withdraw_crop(&mut self) -> Promise {
        let (aid, mut vault) = self.get_vault();
        self.ping(&mut vault);
        let rewards = vault.rewards;
        // zero the rewards to block double-withdraw-cheddar
        vault.rewards = 0;
        self.vaults.insert(&aid.clone(), &vault);
        return self.mint_cheddar(&aid, rewards.into(), 0.into(), false);
    }

    // ******************* //
    // management          //

    /// Opens or closes the farming. For admin use only. Smart contract has `epoch_start` and
    /// `epoch_end` attributes which controls start and end of the farming.
    pub fn set_active(&mut self, is_open: bool) {
        self.assert_owner_calling();
        self.is_active = is_open;
    }

    /*****************
     * internal methods */
    #[inline]
    fn return_tokens(&self, user: AccountId, amount: U128) -> Promise {
        return ext_ft::ft_transfer(
            user,
            amount,
            Some("unstaking".to_string()),
            &self.cheddar_id,
            1,
            GAS_FOR_FT_TRANSFER,
        );
    }

    #[private]
    pub fn return_tokens_callback(&mut self, user: AccountId, amount: U128) {
        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),

            PromiseResult::Successful(_) => {
                env_log!("tokens returned {}", amount.0);

                // TODO
                // if close {
                //     // AUDIT: TODO: Should check that the vault doesn't have `.staked > 0`, because
                //     //    in case of weird async race conditions, the vault might get new staked balance.
                //     self.vaults.remove(&user);
                //     env::log(b"account closed");
                // }
            }

            PromiseResult::Failed => {
                env_log!(
                    "token transfer failed {}. recovering account state",
                    amount.0
                );
                // returning tokens failed, restore account state
                match self.vaults.get(&user) {
                    Some(mut v) => {
                        v.staked += amount.0;
                        self.vaults.insert(&user, &v);
                    }
                    None => {
                        // weird case - account was deleted in the meantime.
                        env_log!(
                            "Account deleted, user {} funds ({} tokens) not recovered and account recreated.",
                            &user,
                            amount.0
                        );
                        self.create_account(&user, amount.0);
                    }
                }
            }
        }
    }

    /// mint cheddar rewards for the user, maybe closes the account
    /// NOTE: the destination account must be registered on CHEDDAR first!
    fn mint_cheddar(&mut self, a: &AccountId, cheddar: U128, tokens: U128, close: bool) -> Promise {
        // AUDIT: TODO: Assert or check that the amount is positive.

        // TODO verify callback and close parameter
        let mut p;
        if cheddar.0 != 0 {
            p = ext_ft::mint(
                a.clone().try_into().unwrap(),
                cheddar,
                &self.cheddar_id,
                ONE_YOCTO,
                GAS_FOR_FT_TRANSFER,
            );
            if tokens.0 != 0 {
                p = p.and(self.return_tokens(a.clone(), tokens));
            } else {
                p = p.then(ext_self::mint_callback(
                    a.clone(),
                    cheddar,
                    close,
                    &env::current_account_id(),
                    0,
                    GAS_FOR_MINT_CALLBACK,
                ));
            }
        } else {
            // NOTE: both can't be empty - we handle that in the caller
            p = self.return_tokens(a.clone(), tokens.clone()).then(
                ext_self::return_tokens_callback(
                    a.clone(),
                    tokens,
                    &env::current_account_id(),
                    0,
                    GAS_FOR_MINT_CALLBACK,
                ),
            );
        }

        p.then(ext_self::mint_callback_finally(
            a.clone(),
            &env::current_account_id(),
            0,
            GAS_FOR_MINT_CALLBACK_FINALLY,
        ))
    }

    #[private]
    pub fn mint_callback(&mut self, user: AccountId, amount: U128, close: bool) {
        env_log!(
            "mint_callback, env::promise_results_count()={}",
            env::promise_results_count()
        );
        // after the async call to mint rewards for the user
        assert_eq!(
            env::promise_results_count(),
            1,
            "{}",
            ERR25_WITHDRAW_CALLBACK
        );
        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),

            PromiseResult::Successful(_) => {
                env_log!("cheddar rewards withdrew {}", amount.0);
                self.total_rewards += amount.0;
                if close {
                    // AUDIT: TODO: Should check that the vault doesn't have `.staked > 0`, because
                    //    in case of weird async race conditions, the vault might get new staked balance.
                    self.vaults.remove(&user);
                    env::log(b"account closed");
                }
            }

            PromiseResult::Failed => {
                // mint failed, restore cheddar rewards
                // AUDIT: TODO: If the vault was closed before by another TX, then the contract
                //     should recover the vault here.
                let mut vault = self.vaults.get(&user).expect(ERR10_NO_ACCOUNT);
                // AUDIT: TODO: You may also want to ping the vault.
                vault.rewards = amount.0;
                self.vaults.insert(&user, &vault);
            }
        }
    }

    #[private]
    pub fn mint_callback_finally(&mut self, user: &AccountId) {
        //Check if rewards were withdrew
        if let Some(vault) = self.vaults.get(&user) {
            if vault.rewards != 0 {
                //if there are cheddar rewards, means the cheddar transfer failed
                panic!("{}", "cheddar transfer failed");
            }
        }
    }

    /// Returns the round number since `start`.
    /// If now < start  return 0.
    /// If now == start return 1.
    /// if now == start + ROUND return 2...
    pub fn current_round(&self) -> usize {
        let mut now = env::block_timestamp() / SECOND;
        if now > self.farming_start {
            return 0;
        }
        // we start rounds from 1
        let mut adjust = 1;
        if now >= self.farming_end {
            now = self.farming_end;
            // if at the end of farming we don't start a new round then we need to force a new round
            if now % ROUND != 0 {
                adjust = 2
            };
        }
        let r: usize = ((now - self.farming_start) / ROUND).try_into().unwrap();
        r + adjust
    }

    fn create_account(&mut self, user: &AccountId, staked: u128) {
        self.vaults.insert(
            &user,
            &Vault {
                // warning: previous can be set in the future
                previous: self.current_round(),
                staked,
                rewards: 0,
            },
        );
    }

    fn assert_owner_calling(&self) {
        assert!(
            env::predecessor_account_id() == self.owner_id,
            "can only be called by the owner"
        );
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[allow(unused_imports)]
mod tests {
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::MockedBlockchain;
    use near_sdk::{testing_env, Balance};
    use std::convert::TryInto;

    use super::*;

    /// deposit_dec = size of deposit in e24
    fn setup_contract(
        account_id: usize,
        deposit_dec: u128,
        round: u64,
    ) -> (VMContextBuilder, Contract) {
        let mut context = VMContextBuilder::new();
        testing_env!(context.build());
        let contract = Contract::new(
            accounts(0),
            "cheddar".to_string().try_into().unwrap(),
            b"atom".try_into().unwrap(),
            120.into(),
            10,
            20,
        );
        testing_env!(context
            .predecessor_account_id(accounts(account_id))
            .attached_deposit((deposit_dec * E24).into())
            .block_timestamp(round * ROUND)
            .build());
        (context, contract)
    }

    #[test]
    fn test_set_active() {
        let (_, mut ctr) = setup_contract(0, 5, 1);
        assert_eq!(ctr.is_active, true);
        ctr.set_active(false);
        assert_eq!(ctr.is_active, false);
    }

    #[test]
    #[should_panic(expected = "can only be called by the owner")]
    fn test_set_active_not_admin() {
        let (_, mut ctr) = setup_contract(1, 0, 1);
        ctr.set_active(false);
    }

    #[test]
    #[should_panic(expected = "E01: min stake amount is 0.01 NEAR")]
    fn test_min_staking() {
        let (mut ctx, mut ctr) = setup_contract(1, 0, 1);
        testing_env!(ctx.attached_deposit(MIN_BALANCE / 10).build());
        ctr.stake();
    }

    #[test]
    fn test_staking() {
        let (mut ctx, mut ctr) = setup_contract(1, 10, 1);
        assert_eq!(
            ctr.total_stake, 0,
            "at the beginning there should be 0 total stake"
        );

        ctr.stake();

        // status returns (account_stake, account_rewards)
        let (a1_s, a1_r, _) = ctr.status(get_acc(0));
        assert_eq!(a1_s.0, 0, "account0 didn't stake");
        assert_eq!(a1_r.0, 0, "account0 didn't stake so no cheddar");

        let (a1_s, a1_r, _) = ctr.status(get_acc(1));
        assert_eq!(a1_s.0, 10 * E24, "account1 staked");
        assert_eq!(
            ctr.total_stake, a1_s.0,
            "total stake should equal to account1 stake"
        );
        assert_eq!(a1_r.0, 0, "no cheddar should be rewarded before start");

        // ------------------------------------------------
        // stake before the farming_start

        testing_env!(ctx
            .attached_deposit(10 * E24)
            .block_timestamp(2 * ROUND)
            .build());
        ctr.stake();

        let (a1_s, a1_r, _) = ctr.status(get_acc(1));
        assert_eq!(a1_s.0, 20 * E24, "account1 stake increased");
        assert_eq!(a1_r.0, 0, "no cheddar should be rewarded before start");
        assert_eq!(
            ctr.total_stake, a1_s.0,
            "total stake should equal to account1 stake"
        );

        // ------------------------------------------------
        // at the start we still shouldn't get any reward.

        testing_env!(ctx.block_timestamp(10 * ROUND + 1).build());
        let (a1_s, a1_r, _) = ctr.status(get_acc(1));
        assert_eq!(a1_s.0, 20 * E24, "account1 stake increased");
        assert_eq!(a1_r.0, 0, "no cheddar should be rewarded before start");

        // ------------------------------------------------
        // Staking at the very beginning wont yield rewards - a whole epoch needs to pass first

        testing_env!(ctx.block_timestamp(10 * ROUND).build());
        let (a1_s, a1_r, _) = ctr.status(get_acc(1));
        assert_eq!(a1_s.0, 20 * E24, "account1 stake didn't change");
        assert_eq!(a1_r.0, 0, "no cheddar should be rewarded before start");

        // ------------------------------------------------
        // WE are alone - we should get 100% of emission.

        testing_env!(ctx.block_timestamp(11 * ROUND).build());
        let (a1_s, a1_r, _) = ctr.status(get_acc(1));
        assert_eq!(a1_s.0, 20 * E24, "account1 stake didn't change");
        assert_eq!(a1_r.0, 2400, "we take all harvest");

        // ------------------------------------------------
        // second check in same epoch shouldn't change rewards

        testing_env!(ctx.block_timestamp(11 * ROUND + 100).build());
        let (a1_s, a1_r, _) = ctr.status(get_acc(1));
        assert_eq!(a1_s.0, 20 * E24, "account1 stake didn't change");
        assert_eq!(
            a1_r.0, 2400,
            "in the same epoch we should harvest only once"
        );

        // ------------------------------------------------
        // 2 epochs later we add another account to stake

        testing_env!(ctx
            .predecessor_account_id(accounts(2))
            .attached_deposit(a1_s.0) // let's stake the same amount.
            .block_timestamp(13 * ROUND + 100)
            .build());
        ctr.stake();

        let (a1_s, a1_r, _) = ctr.status(get_acc(1));
        assert_eq!(a1_s.0, 20 * E24, "account1 stake didn't change");
        assert_eq!(a1_r.0, 120 * 20 * 3, "3rd round of account1 farming");

        let (a2_s, a2_r, _) = ctr.status(get_acc(2));
        assert_eq!(a2_s.0, 20 * E24, "account2 stake was set correctly");
        assert_eq!(
            a2_r.0, 0,
            "account2 can only start farming in the next round"
        );

        assert_eq!(a1_s.0 + a2_s.0, ctr.total_stake);

        // ------------------------------------------------
        // 1 epochs later account 2 should have farming reward

        testing_env!(ctx.block_timestamp(14 * ROUND).build());

        let (a1_s, a1_r, _) = ctr.status(get_acc(1));
        assert_eq!(a1_s.0, 20 * E24, "account1 stake didn't change");
        assert_eq!(a1_r.0, 120 * 20 * 4, "4th round of account1 farming");

        let (a2_s, a2_r, _) = ctr.status(get_acc(2));
        assert_eq!(a2_s.0, 20 * E24, "account2 didn't change");
        assert_eq!(a2_r.0, 120 * 20, "account2 first farming is correct");

        // ------------------------------------------------
        // go to end of farming, and try to stake - it shouldn't change anything.

        testing_env!(ctx
            .predecessor_account_id(accounts(2))
            .block_timestamp(20 * ROUND)
            .build());
        ctr.stake();

        let (_, a1_r, _) = ctr.status(get_acc(1));
        assert_eq!(a1_r.0, 120 * 20 * 10, "10th round of account1 farming");

        let (_, a2_r, _) = ctr.status(get_acc(2));
        assert_eq!(a2_r.0, 120 * 20 * 7, "6th round of account1 farming");

        // ------------------------------------------------
        // try to stake - it shouldn't after the end of farming

        testing_env!(ctx
            .predecessor_account_id(accounts(2))
            .block_timestamp(20 * ROUND)
            .build());
        ctr.stake();
        let (_, a1_r, _) = ctr.status(get_acc(1));
        assert_eq!(a1_r.0, 120 * 20 * 10, "after end of framing - no change");

        let (_, a2_r, _) = ctr.status(get_acc(2));
        assert_eq!(a2_r.0, 120 * 20 * 7, "after end of framing - no change");
    }

    #[test]
    fn test_staking_accumulate() {
        let (mut ctx, mut ctr) = setup_contract(1, 100, 5);
        assert_eq!(
            ctr.total_stake, 0,
            "at the beginning there should be 0 total stake"
        );
        ctr.stake();

        // ------------------------------------------------
        // at round 12 stake same amount with 2 other accounts
        // NOTE: we start farming at round 10

        testing_env!(ctx
            .predecessor_account_id(accounts(2))
            .block_timestamp(12 * ROUND)
            .build());
        ctr.stake();
        testing_env!(ctx.predecessor_account_id(accounts(2)).build());
        ctr.stake();

        // ------------------------------------------------
        // check the rewards at round 14

        testing_env!(ctx
            .predecessor_account_id(accounts(2))
            .block_timestamp(14 * ROUND)
            .build());

        assert_eq!(
            ctr.total_stake,
            300 * E24,
            "each account staked the same amount"
        );

        let (a1_s, a1_r, _) = ctr.status(get_acc(1));
        assert_eq!(a1_s.0, 100 * E24, "account1 stake is correct");
        assert_eq!(a1_r.0, 4 * 120 * 100, "account1 reward");
    }

    fn get_acc(idx: usize) -> AccountId {
        accounts(idx).as_ref().to_string()
    }
}

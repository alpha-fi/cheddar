use std::convert::TryInto;

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::LookupMap;
use near_sdk::json_types::{ValidAccountId, U128};
use near_sdk::{
    assert_one_yocto, env, log, near_bindgen, AccountId, PanicOnDefault, Promise, PromiseResult,
};

pub mod constants;
pub mod errors;
pub mod interfaces;
// pub mod util;
pub mod vault;

use crate::interfaces::*;
use crate::{constants::*, errors::*, vault::*};

near_sdk::setup_alloc!();

/// P2 rewards distribution contract implementing the "Scalable Reward Distribution on the Ethereum Blockchain"
/// algorithm:
/// https://uploads-ssl.webflow.com/5ad71ffeb79acc67c8bcdaba/5ad8d1193a40977462982470_scalable-reward-distribution-paper.pdf
#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    pub owner_id: AccountId,
    /// farm token
    pub cheddar: AccountId,
    /// NEP-141 token for staking
    pub staking_token: AccountId,
    /// if farming is opened
    pub is_active: bool,
    /// user vaults
    pub vaults: LookupMap<AccountId, Vault>,
    /// amount of $CHEDDAR farmed during each round. Round duration is defined in constants.rs
    /// Farmed $CHEDDAR are distributed to all users proportionally to their stake.
    pub rate: u128,
    /// unix timestamp (seconds) when the farming starts.
    pub farming_start: u64,
    /// unix timestamp (seconds) when the farming ends (first time with no farming).
    pub farming_end: u64,
    /// total number of harvested $CHEDDAR
    pub total_harvested: u128,
    /// rewards accumulator: running sum of staked rewards per token (equals to the total
    /// number of farmed tokens).
    s: u128,
    /// round number when the s was previously updated.
    s_round: u64,
    /// total amount of currently staked tokens.
    t: u128,
    /// total number of accounts currently registered.
    pub accounts_registered: u64,
    /// Free rate in basis points. The fee is charged from the user staked tokens
    /// on withdraw. Example: if fee=2 and user withdraws 10000e24 staking tokens
    /// then the protocol will charge 2e24 staking tokens.
    pub fee_rate: u128,
    /// amount of fee collected (in staking token).
    pub fee_collected: u128,
    /// Treasury address - a destination for the collected fees.
    pub treasury: AccountId,
}

#[near_bindgen]
impl Contract {
    /// Initializes the contract with the account where the NEP-141 token contract resides, start block-timestamp & rewards_per_year.
    /// Parameters:
    /// * `farming_start` & `farming_end` are unix timestamps (in seconds).
    /// * `reward_rate` is amount of yoctoCheddars per 1e24 staked tokens (usually tokens are
    ///    denominated in 1e24 on NEAR).
    /// * `fee_rate`: the Contract.fee parameter (in basis points)
    #[init]
    pub fn new(
        owner_id: ValidAccountId,
        cheddar: ValidAccountId,
        staked_token: ValidAccountId,
        farming_start: u64,
        farming_end: u64,
        reward_rate: U128,
        fee_rate: u32,
        treasury: AccountId,
    ) -> Self {
        assert!(
            farming_end > farming_start,
            "Start must be after end, end at there must be at least one round difference"
        );
        Self {
            owner_id: owner_id.into(),
            cheddar: cheddar.into(),
            staking_token: staked_token.into(),
            is_active: true,
            vaults: LookupMap::new(b"v".to_vec()),
            rate: reward_rate.0, //cheddar per round per near (round = 1 second)
            total_harvested: 0,
            farming_start,
            farming_end,
            s: 0,
            s_round: 0,
            t: 0,
            accounts_registered: 0,
            fee_rate: fee_rate.into(),
            fee_collected: 0,
            treasury,
        }
    }

    // ************ //
    // view methods //

    /// Returns amount of staked NEAR and farmed CHEDDAR of given account.
    pub fn get_contract_params(&self) -> ContractParams {
        let r = self.current_round();
        ContractParams {
            owner_id: self.owner_id.clone(),
            farming_token: self.cheddar.clone(),
            staked_token: self.staking_token.clone(),
            farming_rate: self.rate.into(),
            is_active: self.is_active,
            farming_start: self.farming_start,
            farming_end: self.farming_end,
            total_staked: self.t.into(),
            total_farmed: (u128::from(r) * self.rate).into(),
            fee_rate: self.fee_rate.into(),
            accounts_registered: self.accounts_registered,
        }
    }

    /// Returns amount of staked tokens, farmed CHEDDAR and the timestamp of the current round.
    pub fn status(&self, account_id: AccountId) -> (U128, U128, u64) {
        return match self.vaults.get(&account_id) {
            Some(mut v) => {
                let r = self.current_round();
                let farmed = v.ping(self.compute_s(r), r).into();
                // round starts from 1 when now >= farming_start
                let r0 = if r > 1 { r - 1 } else { 0 };
                (v.staked.into(), farmed, self.farming_start + r0 * ROUND)
            }
            None => {
                let zero = U128::from(0);
                return (zero, zero, 0);
            }
        };
    }

    // ******************* //
    // transaction methods //

    /// Unstakes given amount of tokens and transfers it back to the user.
    /// If amount equals to the amount staked then we close the account.
    /// NOTE: account once closed must re-register to stake again.
    /// Returns amount of staked tokens left after the call.
    /// Panics if the caller doesn't stake anything or if he doesn't have enough staked tokens.
    /// Requires 1 yNEAR payment for wallet 2FA.
    #[payable]
    pub fn unstake(&mut self, amount: U128) -> Promise {
        self.assert_is_active();
        assert_one_yocto();
        let amount_u = amount.0;
        let a = env::predecessor_account_id();
        let mut v = self.get_vault(&a);
        assert!(amount_u <= v.staked, "{}", ERR30_NOT_ENOUGH_STAKE);
        if amount_u == v.staked {
            //unstake all => close -- simplify UI
            return self.close();
        }
        self.ping_all(&mut v);
        v.staked -= amount_u;
        self.t -= amount_u;

        self.vaults.insert(&a, &v);
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
    /// and all staked tokens back to the caller.
    /// Returns amount of farmed CHEDDAR.
    /// Panics if the caller doesn't stake anything.
    /// Requires 1 yNEAR payment for wallet validation.
    #[payable]
    pub fn close(&mut self) -> Promise {
        self.assert_is_active();
        assert_one_yocto();
        let a = env::predecessor_account_id();
        let mut v = self.get_vault(&a);
        self.ping_all(&mut v);
        log!("Closing {} account, farmed CHEDDAR: {}", &a, v.rewards);
        // if user doesn't stake anything and has no rewards then we can make a shortcut
        // and remove the account and return storage deposit.
        if v.staked == 0 && v.rewards == 0 {
            self.vaults.remove(&a);
            return Promise::new(a.clone()).transfer(NEAR_BALANCE);
        }

        self.t -= v.staked;

        // We remove the vault but we will try to recover in a callback if a minting will fail.
        self.vaults.remove(&a);
        self.accounts_registered -= 1;
        return self.mint_cheddar(&a, v.rewards.into(), v.staked.into());
    }

    /// Withdraws all farmed CHEDDAR to the user. It doesn't close the account.
    /// Return amount of farmed CHEDDAR.
    /// Panics if user has not staked anything.
    pub fn withdraw_crop(&mut self) -> Promise {
        self.assert_is_active();
        let a = env::predecessor_account_id();
        let mut v = self.get_vault(&a);
        self.ping_all(&mut v);
        let rewards = v.rewards;
        // zero the rewards to block double-withdraw-cheddar
        v.rewards = 0;
        self.vaults.insert(&a, &v);
        return self.mint_cheddar(&a, rewards.into(), 0.into());
    }

    /// Returns the amount of collected fees which are not withdrawn yet.
    pub fn get_collected_fee(&self) -> U128 {
        self.fee_collected.into()
    }

    /// Withdraws all collected fee to the treasury.
    /// Must make sure treasury is registered
    /// Panics if the collected fees == 0.
    pub fn withdraw_fee(&mut self) -> Promise {
        assert!(self.fee_collected > 0, "zero collected fees");
        log!("Withdrawing collected fee: {} tokens", self.fee_collected);
        let fee = U128::from(self.fee_collected);
        self.fee_collected = 0;
        return ext_ft::ft_transfer(
            self.treasury.clone(),
            fee,
            Some("fee withdraw".to_string()),
            &self.staking_token,
            1,
            GAS_FOR_FT_TRANSFER,
        );
    }

    // ******************* //
    // management          //

    /// Opens or closes smart contract operations. When the contract is not active, it won't
    /// reject every user call, until it will be open back again.
    pub fn set_active(&mut self, is_open: bool) {
        self.assert_owner();
        self.is_active = is_open;
    }

    // TODO: remove before mainnet
    /// Opens or closes smart contract operations. When the contract is not active, it won't
    /// reject every user call, until it will be open back again.
    pub fn set_farming_end(&mut self, farming_end: u64) {
        self.assert_owner();
        self.farming_end = farming_end;
    }

    // TODO: remove before mainnet
    pub fn set_reward_rate(&mut self, rate: U128) {
        self.assert_owner();
        self.rate = rate.into();
    }

    /*****************
     * internal methods */

    fn assert_is_active(&self) {
        assert!(self.is_active, "contract is not active");
    }

    /// transfers staked tokens back to the user
    #[inline]
    fn return_tokens(&mut self, user: AccountId, amount: U128) -> Promise {
        let fee = amount.0 * self.fee_rate / 10_000;
        self.fee_collected += fee;
        return ext_ft::ft_transfer(
            user,
            (amount.0 - fee).into(),
            Some("unstaking".to_string()),
            &self.staking_token,
            1,
            GAS_FOR_FT_TRANSFER,
        );
    }

    #[private]
    pub fn return_tokens_callback(&mut self, user: AccountId, amount: U128) {
        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),

            PromiseResult::Successful(_) => {
                log!("tokens returned {}", amount.0);
            }

            PromiseResult::Failed => {
                log!(
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
                        // recovering from `unstake`
                        // or a weird case after closing - account deleted in the meantime.
                        // TODO: check if no attack possible here (user could drain the account if recovering it in aloop of transactions)
                        log!(
                            "Recovering deleted {} account. {} tokens restored.",
                            &user,
                            amount.0
                        );
                        self.create_account(&user, amount.0);
                    }
                }
            }
        }
    }

    /// mint `cheddar` rewards for the user and returns `tokens` staked back to the user.
    /// NOTE: the destination account must be registered on CHEDDAR first!
    fn mint_cheddar(&mut self, a: &AccountId, cheddar_amount: U128, tokens: U128) -> Promise {
        // TODO: verify callback
        let mut p;
        if cheddar_amount.0 != 0 {
            p = ext_ft::ft_mint(
                a.clone(),
                cheddar_amount,
                Some("farming".to_string()),
                &self.cheddar,
                ONE_YOCTO,
                GAS_FOR_FT_TRANSFER,
            );
            if tokens.0 != 0 {
                p = p.and(self.return_tokens(a.clone(), tokens));
            } else {
                p = p.then(ext_self::mint_callback(
                    a.clone(),
                    cheddar_amount,
                    &env::current_account_id(),
                    0,
                    GAS_FOR_MINT_CALLBACK,
                ));
            }
        } else if tokens.0 != 0 {
            p = self.return_tokens(a.clone(), tokens.clone()).then(
                ext_self::return_tokens_callback(
                    a.clone(),
                    tokens,
                    &env::current_account_id(),
                    0,
                    GAS_FOR_MINT_CALLBACK,
                ),
            );
        } else {
            // nothing to mint nor return.
            return Promise::new(a.clone());
        }

        p.then(ext_self::mint_callback_finally(
            a.clone(),
            &env::current_account_id(),
            0,
            GAS_FOR_MINT_CALLBACK_FINALLY,
        ))
    }

    #[private]
    pub fn mint_callback(&mut self, user: AccountId, amount: U128) {
        assert!(
            env::promise_results_count() <= 2,
            "{}",
            ERR25_WITHDRAW_CALLBACK
        );
        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),

            PromiseResult::Successful(_) => {
                log!("cheddar rewards withdrew {}", amount.0);
                self.total_harvested += amount.0;
            }

            PromiseResult::Failed => {
                // mint failed, restore cheddar rewards
                // If the vault was closed before by another TX, then we must recover the state
                let mut v;
                if let Some(v2) = self.vaults.get(&user) {
                    v = v2;
                } else {
                    self.accounts_registered += 1;
                    v = Vault {
                        s: 0,
                        staked: 0,
                        rewards: 0,
                    }
                }
                v.rewards += amount.0;
                self.vaults.insert(&user, &v);
            }
        }
    }

    #[private]
    pub fn mint_callback_finally(&mut self, user: &AccountId) {
        //Check if rewards were withdrew
        if let Some(v) = self.vaults.get(&user) {
            if v.rewards != 0 {
                //if there are cheddar rewards, means the cheddar transfer failed
                panic!("{}", "cheddar transfer failed");
            }
        }
    }

    /// Returns the round number since `start`.
    /// If now < start  return 0.
    /// If now == start return 1.
    /// if now == start + ROUND return 2...
    fn current_round(&self) -> u64 {
        let mut now = env::block_timestamp() / SECOND;
        if now < self.farming_start {
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
        let r: u64 = ((now - self.farming_start) / ROUND).try_into().unwrap();
        r + adjust
    }

    fn create_account(&mut self, user: &AccountId, staked: u128) {
        self.vaults.insert(
            &user,
            &Vault {
                // warning: previous can be set in the future
                s: self.s,
                staked,
                rewards: 0,
            },
        );
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
        testing_env!(ctx.attached_deposit(NEAR_BALANCE / 10).build());
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

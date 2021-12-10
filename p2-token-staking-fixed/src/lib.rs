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
    reward_acc: u128,
    /// round number when the s was previously updated.
    reward_acc_round: u64,
    /// total amount of currently staked tokens.
    total_stake: u128,
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
        treasury: ValidAccountId,
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
            reward_acc: 0,
            reward_acc_round: 0,
            total_stake: 0,
            accounts_registered: 0,
            fee_rate: fee_rate.into(),
            fee_collected: 0,
            treasury: treasury.into(),
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
            total_staked: self.total_stake.into(),
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
                let farmed = v.ping(self.compute_reward_acc(r), r).into();
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
    /// Returns amount of staked tokens left (still staked) after the call.
    /// Panics if the caller doesn't stake anything or if he doesn't have enough staked tokens.
    /// Requires 1 yNEAR payment for wallet 2FA.
    #[payable]
    pub fn unstake(&mut self, amount: U128) -> U128 {
        self.assert_is_active();
        assert_one_yocto();
        let amount_u = amount.0;
        let a = env::predecessor_account_id();
        let mut v = self.get_vault(&a);
        assert!(amount_u <= v.staked, "{}", ERR30_NOT_ENOUGH_STAKE);
        if amount_u == v.staked {
            //unstake all => close -- simplify UX
            self.close();
            return v.staked.into();
        }
        self.ping_all(&mut v);
        v.staked -= amount_u;
        self.total_stake -= amount_u;

        self.vaults.insert(&a, &v);
        self.return_tokens(a, amount);
        return v.staked.into();
    }

    /// Unstakes everything and close the account. Sends all farmed CHEDDAR using a ft_transfer
    /// and all staked tokens back to the caller.
    /// Returns amount of farmed CHEDDAR.
    /// Panics if the caller doesn't stake anything.
    /// Requires 1 yNEAR payment for wallet validation.
    #[payable]
    pub fn close(&mut self) {
        self.assert_is_active();
        assert_one_yocto();
        let a = env::predecessor_account_id();
        let mut v = self.get_vault(&a);
        self.ping_all(&mut v);
        log!("Closing {} account, farmed CHEDDAR: {}", &a, v.farmed);

        // if user doesn't stake anything and has no rewards then we can make a shortcut
        // and remove the account and return storage deposit.
        if v.staked == 0 && v.farmed == 0 {
            self.vaults.remove(&a);
            Promise::new(a.clone()).transfer(STORAGE_COST);
            return;
        }

        self.total_stake -= v.staked;

        // We remove the vault but we will try to recover in a callback if a minting will fail.
        self.vaults.remove(&a);
        self.accounts_registered -= 1;
        self.mint_cheddar(&a, v.farmed.into(), v.staked.into());
    }

    /// Withdraws all farmed CHEDDAR to the user. It doesn't close the account.
    /// Return amount of farmed CHEDDAR.
    /// Panics if user has not staked anything.
    pub fn withdraw_crop(&mut self) {
        self.assert_is_active();
        let a = env::predecessor_account_id();
        let mut v = self.get_vault(&a);
        self.ping_all(&mut v);
        let rewards = v.farmed;
        // zero the rewards to block double-withdraw-cheddar
        v.farmed = 0;
        self.vaults.insert(&a, &v);
        self.mint_cheddar(&a, rewards.into(), 0.into());
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
            user.clone(),
            (amount.0 - fee).into(),
            Some("unstaking".to_string()),
            &self.staking_token,
            1,
            GAS_FOR_FT_TRANSFER,
        )
        .then(ext_self::return_tokens_callback(
            user,
            amount,
            &env::current_account_id(),
            0,
            GAS_FOR_MINT_CALLBACK,
        ));
    }

    #[private]
    pub fn return_tokens_callback(&mut self, user: AccountId, amount: U128) {
        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),

            PromiseResult::Successful(_) => {
                log!("tokens returned {}", amount.0);
                // we can't remove the vault here, because we don't know if `mint` succeded
                //  if it didn't succed, the the mint_callback will try to recover the vault
                //  and recreate it - so potentially we will send back to the user NEAR deposit
                //  multiple times. User should call `close` second time to get back
                //  his NEAR deposit.
            }

            PromiseResult::Failed => {
                log!(
                    "token transfer failed {}. recovering account state",
                    amount.0
                );
                self.recover_state(&user, 0, amount.0);
            }
        }
    }

    /** mint `cheddar` rewards for the user and returns `tokens` staked back to the user.
    / NOTE: the destination account must be registered on CHEDDAR first!
    / NOTE: callers of fn mint_cheddar MUST set rewards to zero in the vault prior to the
    /       call, because in case of failure the callbacks will re-add rewards to the vault */
    fn mint_cheddar(&mut self, a: &AccountId, cheddar_amount: U128, tokens: U128) {
        if cheddar_amount.0 == 0 && tokens.0 == 0 {
            // nothing to mint nor return.
            return;
        }
        let mut p: Option<Promise> = None;
        if cheddar_amount.0 != 0 {
            p = Some(
                ext_ft::ft_mint(
                    a.clone(),
                    cheddar_amount,
                    Some("farming".to_string()),
                    &self.cheddar,
                    ONE_YOCTO,
                    GAS_FOR_FT_TRANSFER,
                )
                .then(ext_self::mint_callback(
                    a.clone(),
                    cheddar_amount,
                    &env::current_account_id(),
                    0,
                    GAS_FOR_MINT_CALLBACK,
                )),
            );
        }
        if tokens.0 != 0 {
            let p_return = self.return_tokens(a.clone(), tokens.clone());
            if let Some(p_mint) = p {
                p = Some(p_mint.and(p_return));
            } else {
                p = Some(p_return);
            }
        }
        let _p = p.unwrap();
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
                log!("cheddar mint failed {}. recovering account state", amount.0);
                self.recover_state(&user, amount.0, 0);
            }
        }
    }

    fn recover_state(&mut self, user: &AccountId, cheddar: u128, staked: u128) {
        let mut v;
        if let Some(v2) = self.vaults.get(&user) {
            v = v2;
            v.staked += staked;
            v.farmed += cheddar;
        } else {
            // If the vault was closed before by another TX, then we must recover the state
            self.accounts_registered += 1;
            v = Vault {
                reward_acc: self.reward_acc,
                staked,
                farmed: cheddar,
            }
        }

        self.vaults.insert(user, &v);
    }

    /// Returns the round number since `start`.
    /// If now < start  return 0.
    /// If now == start return 0.
    /// if now == start + ROUND return 1...
    fn current_round(&self) -> u64 {
        let mut now = env::block_timestamp() / SECOND;
        if now < self.farming_start {
            return 0;
        }
        // we start rounds from 0
        let mut adjust = 0;
        if now >= self.farming_end {
            now = self.farming_end;
            // if at the end of farming we don't start a new round then we need to force a new round
            if now % ROUND != 0 {
                adjust = 1
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
                reward_acc: self.reward_acc,
                staked,
                farmed: 0,
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
    use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
    use near_contract_standards::storage_management::StorageManagement;
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::{testing_env, Balance};
    use near_sdk::{MockedBlockchain, ValidatorId};
    use std::convert::TryInto;

    use super::*;

    fn acc_cheddar() -> ValidAccountId {
        "cheddar".to_string().try_into().unwrap()
    }

    fn acc_staking() -> ValidAccountId {
        "atom".try_into().unwrap()
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
    const RATE: u128 = 12 * E24;

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
            acc_cheddar(),
            acc_staking(),
            10 * ROUND, // farming_start
            20 * ROUND,
            RATE.into(), // reward rate
            fee_rate,
            accounts(1),
        );
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

    fn get_acc(idx: usize) -> AccountId {
        accounts(idx).as_ref().to_string()
    }

    fn assert_close(a: u128, b: u128, msg: &'static str) {
        assert!(a * 99999 / 100_000 < b, "{}, {} <> {}", msg, a, b);
        assert!(a * 100001 / 100_000 > b, "{}, {} <> {}", msg, a, b);
    }
}

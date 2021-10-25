use std::{cmp, convert::TryInto};

// use near_contract_standards::storage_management::{
//     StorageBalance, StorageBalanceBounds, StorageManagement,
// };
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::LookupMap;
use near_sdk::json_types::{ValidAccountId, U128};
use near_sdk::{
    assert_one_yocto, env, log, near_bindgen, AccountId, PanicOnDefault, Promise, PromiseResult,
};

pub mod constants;
pub mod errors;
pub mod interfaces;
pub mod util;
pub mod vault;

use crate::errors::*;
use crate::interfaces::*;
use crate::vault::*;
use crate::{constants::*, util::*};

near_sdk::setup_alloc!();

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    pub owner_id: AccountId,
    /// farm token
    pub cheddar_id: AccountId,
    // if farming is opened
    pub is_active: bool,
    //user vaults
    pub vaults: LookupMap<AccountId, Vault>,
    /// amount of $CHEDDAR farmed each per each epoch. Epoch is defined in constants (`ROUND`)
    /// Farmed $CHEDDAR are distributed to all users proportionally to their NEAR stake.
    pub rate: u128, // cheddar per round per near (round = 1 second)
    pub total_stake: u128,
    /// round number when the farming starts
    pub farming_start: Round,
    /// round number when the farming ends (first round round with no farming)
    pub farming_end: Round,
    /// total number of farmed and withdrawn rewards
    pub total_rewards: u128,
}

#[near_bindgen]
impl Contract {
    /// Initializes the contract with the account where the NEP-141 token contract resides, start block-timestamp & rewards_per_year.
    /// Parameters:
    /// * farming_start & farming_end are unix timestamps
    /// * reward_rate is amount of yoctoCheddars per 1 NEAR (1e24 yNEAR)
    #[init]
    pub fn new(
        owner_id: ValidAccountId,
        cheddar_id: ValidAccountId,
        reward_rate: U128,
        farming_start: u64,
        farming_end: u64,
    ) -> Self {
        Self {
            owner_id: owner_id.into(),
            cheddar_id: cheddar_id.into(),
            is_active: true,
            vaults: LookupMap::new(b"v".to_vec()),
            rate: reward_rate.0, // cheddar per round per near (round = 1 second)
            total_stake: 0,
            total_rewards: 0,
            farming_start: round_from_unix(farming_start),
            farming_end: round_from_unix(farming_end),
        }
    }

    // ************ //
    // view methods //

    /// Returns amount of staked NEAR and farmed CHEDDAR of given account.
    pub fn get_contract_params(&self) -> ContractParams {
        ContractParams {
            owner_id: self.owner_id.clone(),
            token_contract: self.cheddar_id.clone(),
            rewards_per_day: (self.rate * 60 * 60 * 24).into(),
            is_open: self.is_active,
            farming_start: round_to_unix(self.farming_start),
            farming_end: round_to_unix(self.farming_end),
            total_rewards: self.total_rewards.into(),
            total_stake: self.total_stake.into(),
        }
    }

    /// Returns amount of user staked NEAR, farmed CHEDDAR
    /// and the current round (time when the round started).
    pub fn status(&self, account_id: AccountId) -> (U128, U128, u64) {
        let mut v = self.get_vault_or_default(&account_id);
        return (
            v.staked.into(),
            self.ping(&mut v).into(),
            round_to_unix(current_round()),
        );
    }

    /// Return amount of currently staked NEAR (in total).
    pub fn total_staked(&self) -> U128 {
        self.total_stake.into()
    }

    // ******************* //
    // transaction methods //

    /// Stake attached &NEAR and returns total amount of stake.
    #[payable]
    pub fn stake(&mut self) -> U128 {
        self.assert_open();
        let amount = env::attached_deposit();
        assert!(
            amount == STAKE,
            "Exactly 1 NEAR per account stake is expected"
        );
        let aid = env::predecessor_account_id();
        self.total_stake += amount;
        if self.vaults.contains_key(&aid) {
            panic!("You can't stake 2 times");
        }
        let vault = Vault {
            staked: amount,
            previous: cmp::max(current_round(), self.farming_start),
            rewards: 0,
        };
        self.save_vault(&aid, &vault);
        return vault.staked.into();
    }

    /// Unstakes everything and close the account. Sends all farmed CHEDDAR using a ft_transfer
    /// and all NEAR to the caller.
    /// Returns amount of farmed CHEDDAR.
    /// Panics if the caller doesn't stake anything.
    /// Requires 1 yNEAR payment for wallet validation.
    #[payable]
    pub fn close(&mut self) -> Promise {
        assert_one_yocto();
        let aid = env::predecessor_account_id();
        let mut vault = self.get_vault(&aid);
        self.ping(&mut vault);
        log!(
            "Closing {} account, farmed CHEDDAR: {}",
            &aid,
            vault.rewards
        );

        let rewards = vault.rewards;
        let to_transfer = vault.staked;
        assert!(
            self.total_stake >= to_transfer,
            "total_staked {} < to_transfer {}",
            self.total_stake,
            to_transfer
        );

        // since NEAR transfer never fails, we must remove the vault here to clean up the
        // storage and protect against double-unstake or double harvest (which would be
        // possible  other promise will fail),
        self.vaults.remove(&aid);
        self.total_stake -= to_transfer;

        // 1st promise is to transfer back all their NEAR, we assume near transfer does not fail
        assert!(to_transfer > 0);
        let p = Promise::new(aid.clone()).transfer(to_transfer);

        // if there are no rewards, no need to call self.mint_cheddar_promise
        if rewards == 0 {
            return p;
        }

        // 2nd promise is to mint cheddar rewards for the user & close the account
        // NOTE: returning a merged (using `and`) promise is forbidden now.
        // Joining the 2 promises with "then" causes the "transfer"
        // to become the main promise for the callback,
        // so the success/failure of the mint-call can not be evaluated.
        // Creating 2 separate promises works correctly.
        return self.mint_cheddar_promise(&aid, rewards.into());
    }

    /// Withdraws all farmed CHEDDAR to the user. It doesn't close the account.
    /// Call `close` to remove the account and return all NEAR deposit.
    /// Return amount of farmed CHEDDAR.
    /// Panics if user has not staked anything.
    #[payable]
    pub fn withdraw_crop(&mut self) -> Promise {
        let aid = env::predecessor_account_id();
        let mut vault = self.get_vault(&aid);
        self.ping(&mut vault);
        assert!(vault.rewards > 0);
        let rewards = vault.rewards;
        // zero the rewards to avoid double-withdraw-cheddar
        vault.rewards = 0;
        self.save_vault(&aid, &vault);
        return self.mint_cheddar_promise(&aid, rewards.into());
    }

    // ******************* //
    // management          //

    /// Opens or closes the farming. For admin use only. Smart contract has `epoch_start` and
    /// `epoch_end` attributes which controls start and end of the farming.
    pub fn set_active(&mut self, is_open: bool) {
        self.assert_owner_calling();
        self.is_active = is_open;
    }

    /// changes farming start-end. For admin use only
    pub fn set_start_end(&mut self, farming_start: u64, farming_end: u64) {
        self.assert_owner_calling();
        self.farming_start = round_from_unix(farming_start);
        self.farming_end = round_from_unix(farming_end);
    }

    pub fn farm_yocto(&mut self) {
        self.assert_owner_calling();
        let a = env::predecessor_account_id();
        ext_ft::ft_mint(
            a.clone().try_into().unwrap(),
            1.into(),
            Some("admin farming 1 yocto".to_string()),
            &self.cheddar_id,
            ONE_YOCTO,
            GAS_FOR_FT_TRANSFER,
        );
        self.total_rewards += 1;
    }

    /*****************
     * internal methods */

    /// mint `amount` cheddar rewards for the user, maybe closes the account
    /// NOTE: the destination account must be registered on CHEDDAR first!
    fn mint_cheddar_promise(&mut self, a: &AccountId, amount: U128) -> Promise {
        assert!(amount.0 > 0, "amount of Cheddar to mint must be positive");
        // launch async callback to mint rewards for the user
        ext_ft::ft_mint(
            a.clone().try_into().unwrap(),
            amount,
            Some("farming".to_string()),
            &self.cheddar_id,
            ONE_YOCTO,
            GAS_FOR_FT_TRANSFER,
        )
        .then(ext_self::mint_callback(
            a.clone(),
            amount,
            &env::current_account_id(),
            NO_DEPOSIT,
            GAS_FOR_MINT_CALLBACK,
        ))
        .then(ext_self::mint_callback_finally(
            a.clone(),
            amount,
            &env::current_account_id(),
            NO_DEPOSIT,
            GAS_FOR_MINT_CALLBACK_FINALLY,
        ))
    }

    /// Callback after the cheddar minting. The functions is not publicly exported
    /// to the user. It updates the global `total_rewards` when the minting succeeded.
    /// If the minting failed, it recovers the user and global state.
    #[private]
    pub fn mint_callback(&mut self, user: AccountId, amount: U128) {
        assert_eq!(
            env::promise_results_count(),
            1,
            "{}",
            ERR25_WITHDRAW_CALLBACK
        );
        // check prev promise result
        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),

            PromiseResult::Successful(_) => {
                log!("Cheddar rewards withdrew {}", amount.0);
                self.total_rewards += amount.0;
            }

            PromiseResult::Failed => {
                // mint failed, restore cheddar rewards
                log!("Cheddar minting failed, recovering the account");
                let mut vault = self.get_vault_or_default(&user);
                vault.rewards += amount.0;
                self.ping(&mut vault);
                self.save_vault(&user, &vault);
            }
        }
    }

    /// The final callback to inform the UI if the transaction succeeded or not.
    /// It will remove (unregister) user account if the user closes it.
    /// HACK: we are assuming `vault.rewards` is zero when the farming succeeded (because
    /// user can't partially claim rewards).
    #[private]
    pub fn mint_callback_finally(&mut self, user: &AccountId, amount: U128) -> U128 {
        // Check if rewards were withdrew
        let vault = self.get_vault_or_default(&user);
        if vault.rewards != 0 {
            //if there are cheddar rewards, means the cheddar transfer failed
            panic!("{}", "Cheddar minting failed");
        }
        amount
    }

    fn assert_owner_calling(&self) {
        assert!(
            env::predecessor_account_id() == self.owner_id,
            "can only be called by the owner"
        );
    }

    fn assert_open(&self) {
        assert!(self.is_active, "Farming is not open");
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
    #[should_panic(expected = "Exactly 1 NEAR per account stake is expected")]
    fn test_staking_too_small() {
        let (mut ctx, mut ctr) = setup_contract(1, 1, 1);
        testing_env!(ctx.attached_deposit(STAKE / 10).build());
        ctr.stake();
    }

    #[test]
    #[should_panic(expected = "Exactly 1 NEAR per account stake is expected")]
    fn test_staking_too_big() {
        let (_, mut ctr) = setup_contract(1, 2, 1);
        ctr.stake();
    }

    #[test]
    fn test_staking() {
        let (mut ctx, mut ctr) = setup_contract(1, 1, 1);
        assert_eq!(
            ctr.total_stake, 0,
            "at the beginning there should be 0 total stake"
        );

        // ------------------------------------------------
        // stake before the farming_start
        ctr.stake();

        // status returns (account_stake, account_rewards)
        let (a1_s, a1_r, _) = ctr.status(get_acc(0));
        assert_eq!(a1_s.0, 0, "account0 didn't stake");
        assert_eq!(a1_r.0, 0, "account0 didn't stake so no cheddar");

        let (a1_s, a1_r, _) = ctr.status(get_acc(1));
        assert_eq!(a1_s.0, STAKE, "account1 staked");
        assert_eq!(
            ctr.total_stake, a1_s.0,
            "total stake should equal to account1 stake"
        );
        assert_eq!(a1_r.0, 0, "no cheddar should be rewarded before start");

        // ------------------------------------------------
        // during the first round we still shouldn't get any reward.

        testing_env!(ctx.block_timestamp(10 * ROUND + ROUND / 2).build());
        let (a1_s, a1_r, _) = ctr.status(get_acc(1));
        assert_eq!(a1_s.0, STAKE, "account1 stake didn't change");
        assert_eq!(a1_r.0, 0, "no cheddar should be rewarded before start");

        // ------------------------------------------------
        // WE are alone - we should get 100% of emission.

        testing_env!(ctx.block_timestamp(11 * ROUND).build());
        let (a1_s, a1_r, _) = ctr.status(get_acc(1));
        assert_eq!(a1_s.0, STAKE, "account1 stake didn't change");
        assert_eq!(a1_r.0, 120, "we take all harvest");

        // ------------------------------------------------
        // second check in same epoch shouldn't change rewards

        testing_env!(ctx.block_timestamp(11 * ROUND + 100).build());
        let (a1_s, a1_r, _) = ctr.status(get_acc(1));
        assert_eq!(a1_s.0, STAKE, "account1 stake didn't change");
        assert_eq!(a1_r.0, 120, "in the same epoch we should harvest only once");

        // ------------------------------------------------
        // 2 epochs later we add another account to stake

        testing_env!(ctx
            .predecessor_account_id(accounts(2))
            .block_timestamp(13 * ROUND + 100)
            .build());
        ctr.stake();

        let (a1_s, a1_r, _) = ctr.status(get_acc(1));
        assert_eq!(a1_s.0, STAKE, "account1 stake didn't change");
        assert_eq!(a1_r.0, 120 * 3, "3rd round of account1 farming");

        let (a2_s, a2_r, _) = ctr.status(get_acc(2));
        assert_eq!(a2_s.0, STAKE, "account2 stake was set correctly");
        assert_eq!(
            a2_r.0, 0,
            "account2 can only start farming in the next round"
        );

        assert_eq!(a1_s.0 + a2_s.0, ctr.total_stake);

        // ------------------------------------------------
        // 1 epochs later account 2 should have farming reward

        testing_env!(ctx.block_timestamp(14 * ROUND).build());

        let (a1_s, a1_r, _) = ctr.status(get_acc(1));
        assert_eq!(a1_s.0, STAKE, "account1 stake didn't change");
        assert_eq!(a1_r.0, 120 * 4, "4th round of account1 farming");

        let (a2_s, a2_r, _) = ctr.status(get_acc(2));
        assert_eq!(a2_s.0, STAKE, "account2 didn't change");
        assert_eq!(a2_r.0, 120, "account2 first farming is correct");

        // ------------------------------------------------
        // go to end of farming, and try to stake with other account
        // - it shouldn't change anything.

        testing_env!(ctx
            .predecessor_account_id(accounts(3))
            .block_timestamp(20 * ROUND)
            .build());
        ctr.stake();

        let (_, a1_r, _) = ctr.status(get_acc(1));
        assert_eq!(a1_r.0, 120 * 10, "10th round of account1 farming");

        let (_, a2_r, _) = ctr.status(get_acc(2));
        assert_eq!(a2_r.0, 120 * 7, "6th round of account1 farming");

        // ------------------------------------------------
        // Move few blocks after the end of the farming, it shouldn't change anything

        testing_env!(ctx.block_timestamp(30 * ROUND).build());

        let (_, a1_r, _) = ctr.status(get_acc(1));
        assert_eq!(a1_r.0, 120 * 10, "no more farming");

        let (_, a2_r, _) = ctr.status(get_acc(2));
        assert_eq!(a2_r.0, 120 * 7, "no more farming");

        let (a3_s, a3_r, _) = ctr.status(get_acc(3));
        assert_eq!(a3_s.0, STAKE, "account3 stake should be correct");
        assert_eq!(a3_r.0, 0, "account3 should not farm anyting");
    }

    #[test]
    fn test_staking_accumulate() {
        let (mut ctx, mut ctr) = setup_contract(1, 100, 5);
        assert_eq!(
            ctr.total_stake, 0,
            "at the beginning there should be 0 total stake"
        );
        // this should work
        testing_env!(ctx.attached_deposit(STAKE).build());
        ctr.stake();

        // ------------------------------------------------
        // at round 12 stake same amount with 2 other accounts
        // NOTE: we start farming at round 10

        testing_env!(ctx
            .predecessor_account_id(accounts(2))
            .block_timestamp(12 * ROUND)
            .build());
        ctr.stake();
        testing_env!(ctx.predecessor_account_id(accounts(3)).build());
        ctr.stake();

        // ------------------------------------------------
        // check the rewards at round 14

        testing_env!(ctx
            .predecessor_account_id(accounts(2))
            .block_timestamp(14 * ROUND)
            .build());

        assert_eq!(
            ctr.total_stake,
            3 * E24,
            "each account staked the same amount"
        );

        let (a1_s, a1_r, _) = ctr.status(get_acc(1));
        assert_eq!(a1_s.0, E24, "account1 stake is correct");
        assert_eq!(a1_r.0, 4 * 120, "account1 reward");
    }

    fn get_acc(idx: usize) -> AccountId {
        accounts(idx).as_ref().to_string()
    }
}

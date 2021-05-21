use std::{cmp, convert::TryInto};

// use near_contract_standards::storage_management::{
//     StorageBalance, StorageBalanceBounds, StorageManagement,
// };
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
    /// amount of $CHEDDAR farmed each per each epoch. Epoch is defined in constants (`EPOCH`)
    /// Farmed $CHEDDAR are distributed to all users proportionally to their NEAR stake.
    pub emission_rate: u128,
    pub total_stake: u128,
    pub farming_start_round: u64,
    pub farming_end_round: u64,
}

#[near_bindgen]
impl Contract {
    /// Initializes the contract with the account where the NEP-141 token contract resides, start block-timestamp & rewards_per_year
    /// farming_start & farming_end are unix timestamps
    /// emission_rate is yoctoCheddars per minute
    #[init]
    pub fn new(
        owner_id: ValidAccountId,
        cheddar_id: ValidAccountId,
        emission_rate: U128,
        farming_start: u64,
        farming_end: u64,
    ) -> Self {
        Self {
            owner_id: owner_id.into(),
            cheddar_id: cheddar_id.into(),
            is_active: true,
            vaults: LookupMap::new(b"v".to_vec()),
            emission_rate: (emission_rate.0 / ROUNDS_PER_MINUTE as u128).into(),
            total_stake: 0,
            farming_start_round: round_from(farming_start),
            farming_end_round: round_from(farming_end),
        }
    }

    // ************ //
    // view methods //

    /// Opens or closes the farming. For admin use only. Smart contract has `epoch_start` and
    /// `epoch_end` attributes which controls start and end of the farming.
    pub fn set_active(&mut self, is_open: bool) {
        self.assert_owner_calling();
        self.is_active = is_open;
    }

    /// Returns amount of staked NEAR and farmed CHEDDAR of given account.
    pub fn get_contract_params(&self) -> ContractParams {
        ContractParams {
            owner_id: self.owner_id.clone(),
            token_contract: self.cheddar_id.clone(),
            emission_rate: self.emission_rate.into(),
            is_open: self.is_active,
            farming_start: unix_timestamp_from(self.farming_start_round), //convert round to unix timestamp
            farming_end: unix_timestamp_from(self.farming_end_round), //convert round to unix timestamp
        }
    }

    /// Returns amount of staked NEAR and farmed CHEDDAR of given account.
    pub fn status(&self, account_id: AccountId) -> (U128, U128, u64) {
        return match self.vaults.get(&account_id) {
            Some(mut v) => (v.staked.into(), self.ping(&mut v).into(), unix_timestamp_from(current_round())),
            None => {
                let zero = U128::from(0);
                return (zero, zero, 0);
            }
        };
    }

    // ******************* //
    // transaction methods //

    /// Stake attached &NEAR and returns total amount of stake.
    #[payable]
    pub fn stake(&mut self) -> U128 {
        self.assert_open();
        let amount = env::attached_deposit();
        assert!(amount >= MIN_STAKE, "{}", ERR01_MIN_STAKE);
        self.total_stake += amount;
        let aid = env::predecessor_account_id();
        match self.vaults.get(&aid) {
            Some(mut vault) => {
                self._stake(amount, &mut vault);
                self.vaults.insert(&aid, &vault);
                return vault.staked.into();
            }
            None => {
                self.vaults.insert(
                    &aid,
                    &Vault {
                        previous: cmp::max(current_round(), self.farming_start_round), //warning: previous can be set in the future
                        staked: amount,
                        rewards: 0,
                    },
                );
                return amount.into();
            }
        };
    }

    /// Unstakes given amount of $NEAR and transfers it back to the user.
    /// Returns amount of staked tokens left after the call.
    /// Panics if the caller doesn't stake anything or if he doesn't have enough staked tokens.
    /// Requires 1 yNEAR payment for wallet validation.
    #[payable]
    pub fn unstake(&mut self, amount: U128) -> U128 {
        assert_one_yocto();
        let amount = u128::from(amount);
        let (aid, mut vault) = self.get_vault();
        if vault.staked >= MIN_STAKE && amount >= vault.staked - MIN_STAKE {
            //unstake all => close -- simplify UI
            return self.close();
        }
        self._unstake(amount, &mut vault);

        self.total_stake -= amount;
        self.vaults.insert(&aid, &vault);
        Promise::new(aid).transfer(amount);
        vault.staked.into()
    }

    /// Unstakes everything and close the account. Sends all farmed CHEDDAR using a ft_transfer
    /// and all NEAR to the caller.
    /// Returns amount of farmed CHEDDAR.
    /// Panics if the caller doesn't stake anything.
    /// Requires 1 yNEAR payment for wallet validation.
    #[payable]
    pub fn close(&mut self) -> U128 {
        assert_one_yocto();
        let (aid, mut vault) = self.get_vault();
        self.ping(&mut vault);
        env_log!(
            "Closing {} account, farmed CHEDDAR: {}",
            &aid,
            vault.rewards
        );
        self.vaults.remove(&aid);

        let rewards_str: U128 = vault.rewards.into();
        //assert!(self.total_stake >= vault.staked,"total_staked {} < vault.staked {}", self.total_stake, vault.staked);
        self.total_stake = self.total_stake.saturating_sub(vault.staked);
        let callback = self.withdraw_cheddar(&aid, rewards_str);
        Promise::new(aid).transfer(vault.staked).and(callback);

        // TODO: recover the account - it's not deleted!

        return vault.staked.into();
    }

    /// Withdraws all farmed CHEDDAR to the user.
    /// Return amount of farmed CHEDDAR.
    /// Panics if user has not staked anything.
    #[payable]
    pub fn withdraw_crop(&mut self) -> U128 {
        let (aid, mut vault) = self.get_vault();
        self.ping(&mut vault);
        let rewards = vault.rewards;
        vault.rewards = 0;
        self.vaults.insert(&aid, &vault);
        self.withdraw_cheddar(&aid, rewards.into());
        return rewards.into();
    }

    /*****************
     * internal methods */

    fn withdraw_cheddar(&mut self, a: &AccountId, amount: U128) -> Promise {
        ext_ft::ft_transfer(
            a.clone().try_into().unwrap(),
            amount,
            None,
            &self.cheddar_id,
            ONE_YOCTO,
            GAS_FOR_FT_TRANSFER,
        )
        .then(ext_self::withdraw_callback(
            a.clone(),
            amount,
            &env::current_account_id(),
            NO_DEPOSIT,
            GAS_FOR_FT_TRANSFER,
        ))
    }

    #[private]
    pub fn withdraw_callback(&mut self, sender_id: AccountId, amount: U128) {
        assert_eq!(
            env::promise_results_count(),
            1,
            "{}",
            ERR25_WITHDRAW_CALLBACK
        );
        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Successful(_) => {}
            PromiseResult::Failed => {
                let mut v = self.vaults.get(&sender_id).expect(ERR10_NO_ACCOUNT);
                v.rewards += amount.0;
                self.vaults.insert(&sender_id, &v);
                env_log!("cheddar transfer failed")
            }
        };
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

#[macro_export]
macro_rules! env_log {
    ($($arg:tt)*) => {{
        let msg = format!($($arg)*);
        // io::_print(msg);
        println!("{}", msg);
        env::log(msg.as_bytes())
    }}
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[allow(unused_imports)]
mod tests {
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::MockedBlockchain;
    use near_sdk::{testing_env, Balance};
    use std::convert::TryInto;

    use super::*;

    /// deposit_dec =
    fn setup_contract(
        account_id: usize,
        deposit_dec: u128,
        time: u64,
    ) -> (VMContextBuilder, Contract) {
        let mut context = VMContextBuilder::new();
        testing_env!(context.build());
        let contract = Contract::new(
            accounts(0),
            "cheddar".to_string().try_into().unwrap(),
            120000.into(),
            10,
            20,
        );
        testing_env!(context
            .predecessor_account_id(accounts(account_id))
            .attached_deposit((deposit_dec * MIN_STAKE / 10).into())
            .block_timestamp(time * ROUND)
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
        let (_, mut ctr) = setup_contract(1, 5, 1);
        ctr.stake();
    }

    #[test]
    fn test_staking() {
        let (mut ctx, mut ctr) = setup_contract(1, 100, 1);
        ctr.stake();

        let mut s = ctr.status(get_acc(0));
        assert_eq!(s.0 .0, 0, "account(0) didn't stake");
        assert_eq!(s.1 .0, 0, "account(0) didn't stake so no cheddar");

        s = ctr.status(get_acc(1));
        assert_eq!(s.0 .0, 10 * MIN_STAKE, "account(1) staked");
        assert_eq!(s.1 .0, 0, "no cheddar should be rewarded before start");

        // stake before the farming_start

        testing_env!(ctx
            .attached_deposit((100 * MIN_STAKE / 10).into())
            .block_timestamp(2 * ROUND)
            .build());
        ctr.stake();
        /*        s = ctr.status(get_acc(1));
                assert_eq!(s.0 .0, 20 * MIN_STAKE, "account(1) stake increased");
                assert_eq!(s.1 .0, 0, "no cheddar should be rewarded before start");

                // at the start we still shouldn't get any reward.

                testing_env!(ctx.block_timestamp(10 * EPOCH + 1).build());
                s = ctr.status(get_acc(1));
                assert_eq!(s.0 .0, 20 * MIN_STAKE, "account(1) stake increased");
                assert_eq!(s.1 .0, 0, "no cheddar should be rewarded before start");
        */
        // Staking at the very beginning wont yield rewards - a whole epoch needs to pass first
        testing_env!(ctx.block_timestamp(10 * ROUND).build());
        s = ctr.status(get_acc(1));
        assert_eq!(s.0 .0, 20 * MIN_STAKE, "account(1) stake didn't change");
        assert_eq!(s.1 .0, 0, "no cheddar should be rewarded before start");

        // WE are alone - we should get 100% of emission.

        testing_env!(ctx.block_timestamp(11 * ROUND).build());
        s = ctr.status(get_acc(1));
        assert_eq!(s.0 .0, 20 * MIN_STAKE, "account(1) stake didn't change");
        assert_eq!(s.1 .0, 120, "no cheddar should be rewarded before start");

        // second check in same epoch shouldn't change rewards:

        testing_env!(ctx.block_timestamp(11 * ROUND + 100).build());
        s = ctr.status(get_acc(1));
        assert_eq!(s.0 .0, 20 * MIN_STAKE, "account(1) stake didn't change");
        assert_eq!(s.1 .0, 120, "no cheddar should be rewarded before start");

        // TODO: add more tests
    }

    fn get_acc(idx: usize) -> AccountId {
        accounts(idx).as_ref().to_string()
    }
}

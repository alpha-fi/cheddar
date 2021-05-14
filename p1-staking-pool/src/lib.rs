use std::convert::TryInto;

// use near_contract_standards::storage_management::{
//     StorageBalance, StorageBalanceBounds, StorageManagement,
// };
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::LookupMap;
use near_sdk::json_types::{ValidAccountId, U128};
use near_sdk::{
    assert_one_yocto, env, near_bindgen, AccountId, Balance, PanicOnDefault, Promise, PromiseResult,
};

use crate::constants::*;
use crate::errors::*;
use crate::interfaces::*;
use crate::vault::*;

mod constants;
mod errors;
mod interfaces;
mod vault;

near_sdk::setup_alloc!();

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    pub cheddar_id: AccountId,
    pub vaults: LookupMap<AccountId, Vault>,
    pub total_stake: Vault,

    /// amount of $CHEDDAR farmed each epoch per 1 staked $NEAR.
    pub reward_rate: Balance,

    /// block timestamp when we start farming
    pub start: u64,
}

#[near_bindgen]
impl Contract {
    /// Initializes the contract with the given total supply owned by the given `owner_id`.
    #[init]
    pub fn new(cheddar_id: ValidAccountId, start: u64, reward_rate: Balance) -> Self {
        Self {
            cheddar_id: cheddar_id.into(),
            vaults: LookupMap::new(b"v".to_vec()),
            total_stake: Vault::default(),

            reward_rate,
            start,
        }
    }

    // ************ //
    // view methods //

    /// Returns amount of staked NEAR and farmed CHEDDAR of given account.
    pub fn status(&self, account_id: AccountId) -> (U128, U128) {
        match self.vaults.get(&account_id) {
            None => {
                let zero = U128::from(0);
                return (zero, zero);
            }
            Some(mut v) => {
                v.ping(current_epoch(), self.reward_rate);
                return (v.total().into(), v.rewards.into());
            }
        }
    }

    // ******************* //
    // transaction methods //

    /// Stake attached &NEAR and returns total amount of stake.
    #[payable]
    pub fn stake(&mut self) -> U128 {
        let amount = env::attached_deposit();
        assert!(amount >= MIN_STAKE, "{}", ERR01_MIN_STAKE);
        let aid = env::predecessor_account_id();
        let epoch = current_epoch();
        match self.vaults.get(&aid) {
            None => {
                self.vaults.insert(
                    &aid,
                    &Vault {
                        epoch,
                        locked: amount,
                        locked_new: 0,
                        rewards: 0,
                    },
                );
                return amount.into();
            }
            Some(mut v) => {
                v.stake(amount, epoch, self.reward_rate);
                self.vaults.insert(&aid, &v);
                return v.total().into();
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
        let (aid, mut v) = self.get_vault();
        v.unstake(amount, current_epoch(), self.reward_rate);
        let left = v.total();
        assert!(left <= MIN_STAKE || v.rewards != 0, "{}", ERR02_MIN_BALANCE);

        self.vaults.insert(&aid, &v);
        Promise::new(aid).transfer(amount);
        left.into()
    }

    /// Unstakes everything and close the account. Sends all farmed CHEDDAR using a ft_tranfer
    /// and all NEAR to the caller.
    /// Returns amount of farmed CHEDDAR.
    /// Panics if the caller doesn't stake anything.
    /// Requires 1 yNEAR payment for wallet validation.
    #[payable]
    pub fn close(&mut self) -> U128 {
        assert_one_yocto();
        let (aid, mut v) = self.get_vault();
        env_log!("Closing {} account, farmed CHEDDAR: {}", &aid, v.rewards);
        v.ping(current_epoch(), self.reward_rate);
        self.vaults.remove(&aid);

        let rewards_str: U128 = v.rewards.into();
        let callback = self.withdraw_cheddar(&aid, rewards_str);
        Promise::new(aid).transfer(v.total()).and(callback);

        // TODO: recover the account - it's not deleted!

        return rewards_str;
    }

    #[payable]
    pub fn withdraw_crop(&mut self, amount: U128) {
        let amount_n = u128::from(amount);
        assert!(amount_n > 0, "can't withdraw 0 tokens");
        let (aid, mut v) = self.get_vault();
        v.ping(current_epoch(), self.reward_rate);
        assert!(
            amount_n > v.rewards,
            "E20: not enough cheddar farmed, available: {}",
            v.rewards
        );
        v.rewards -= amount_n;
        self.vaults.insert(&aid, &v);
        self.withdraw_cheddar(&aid, amount);
    }

    /*****************
     * internal methods */

    fn withdraw_cheddar(&mut self, a: &AccountId, amount: U128) -> Promise {
        ext_ft::ft_transfer(
            a.clone().try_into().unwrap(),
            amount,
            None,
            &self.cheddar_id,
            1,
            GAS_FOR_FT_TRANSFER,
        )
        .then(ext_self::withdraw_callback(
            a.clone(),
            amount,
            &env::current_account_id(),
            0,
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
            }
        };
    }
}

fn current_epoch() -> u64 {
    return env::block_timestamp();
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
mod tests {
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::MockedBlockchain;
    use near_sdk::{testing_env, Balance};

    use super::*;

    const OWNER_SUPPLY: Balance = 1_000_000_000_000_000;

    fn get_context(predecessor_account_id: ValidAccountId) -> VMContextBuilder {
        let mut builder = VMContextBuilder::new();
        builder
            .current_account_id(accounts(0))
            .signer_account_id(predecessor_account_id.clone())
            .predecessor_account_id(predecessor_account_id);
        builder
    }

    // #[test]
    // fn test_new() {
    //     let mut context = get_context(accounts(1));
    //     testing_env!(context.build());
    //     let contract = Contract::new(accounts(1).into(), OWNER_SUPPLY.into());
    //     testing_env!(context.is_view(true).build());
    //     assert_eq!(contract.ft_total_supply().0, OWNER_SUPPLY);
    //     assert_eq!(contract.ft_balance_of(accounts(1)).0, OWNER_SUPPLY);
    // }

    // #[test]
    // #[should_panic(expected = "The contract is not initialized")]
    // fn test_default() {
    //     let context = get_context(accounts(1));
    //     testing_env!(context.build());
    //     let _contract = Contract::default();
    // }

    // #[test]
    // fn test_transfer() {
    //     let mut context = get_context(accounts(2));
    //     testing_env!(context.build());
    //     let mut contract = Contract::new(accounts(2).into(), OWNER_SUPPLY.into());
    //     testing_env!(context
    //         .storage_usage(env::storage_usage())
    //         .attached_deposit(contract.storage_balance_bounds().min.into())
    //         .predecessor_account_id(accounts(1))
    //         .build());
    //     // Paying for account registration, aka storage deposit
    //     contract.storage_deposit(None, None);

    //     testing_env!(context
    //         .storage_usage(env::storage_usage())
    //         .attached_deposit(1)
    //         .predecessor_account_id(accounts(2))
    //         .build());
    //     let transfer_amount = OWNER_SUPPLY / 3;
    //     contract.ft_transfer(accounts(1), transfer_amount.into(), None);

    //     testing_env!(context
    //         .storage_usage(env::storage_usage())
    //         .account_balance(env::account_balance())
    //         .is_view(true)
    //         .attached_deposit(0)
    //         .build());
    //     assert_eq!(
    //         contract.ft_balance_of(accounts(2)).0,
    //         (OWNER_SUPPLY - transfer_amount)
    //     );
    //     assert_eq!(contract.ft_balance_of(accounts(1)).0, transfer_amount);
    // }
}

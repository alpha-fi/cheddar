use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{ext_contract, AccountId};

// #[ext_contract(ext_staking_pool)]
pub trait StakingPool {
    // #[payable]
    fn stake(&mut self, amount: U128);

    // #[payable]
    fn unstake(&mut self, amount: U128) -> U128;

    fn withdraw_crop(&mut self, amount: U128);

    /****************/
    /* View methods */
    /****************/

    /// Returns amount of staked NEAR and farmed CHEDDAR of given account & the unix-timestamp for the calculation.
    fn status(&self, account_id: AccountId) -> (U128, U128, u64);
}

#[ext_contract(ext_self)]
pub trait ExtSelf {
    fn mint_callback(&mut self, user: AccountId, amount: U128);
    fn mint_callback_finally(&mut self, user: AccountId, amount: U128);
}

#[ext_contract(ext_ft)]
pub trait FungibleToken {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
    fn mint(&mut self, account_id: AccountId, amount: U128);
}

#[derive(Deserialize, Serialize)]
pub struct ContractParams {
    pub owner_id: AccountId,
    pub token_contract: AccountId,
    pub rewards_per_day: U128,
    pub is_open: bool,
    pub farming_start: u64,
    pub farming_end: u64,
    pub total_rewards: U128,
    pub total_stake: U128,
}

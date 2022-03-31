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
    fn transfer_staked_callback(
        &mut self,
        user: AccountId,
        token_i: usize,
        amount: U128,
        fee: U128,
    );
    fn transfer_farmed_callback(&mut self, user: AccountId, token_i: usize, amount: U128);
    fn mint_callback(&mut self, user: AccountId, amount: U128);
    fn mint_callback_finally(&mut self);
    fn close_account(&mut self, user: AccountId);
}

#[ext_contract(ext_ft)]
pub trait FungibleToken {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
    fn ft_mint(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
}

#[derive(Deserialize, Serialize)]
pub struct ContractParams {
    pub is_active: bool,
    pub owner_id: AccountId,
    pub stake_tokens: Vec<AccountId>,
    pub stake_rates: Vec<U128>,
    pub farm_unit_emission: U128,
    pub farm_tokens: Vec<AccountId>,
    pub farm_token_rates: Vec<U128>,
    pub farming_start: u64,
    pub farming_end: u64,
    pub total_staked: Vec<U128>,
    /// total farmed is total amount of tokens farmed (not necessary minted - which would be
    /// total_harvested).
    pub total_farmed: Vec<U128>,
    pub fee_rate: U128,
    /// Number of accounts currently registered.
    pub accounts_registered: u64,
}

#[derive(Deserialize, Serialize)]
pub struct Status {
    pub stake_tokens: Vec<U128>,
    pub farmed_units: U128,
    pub farmed_tokens: Vec<U128>,
    /// timestamp of the current round.
    pub timestamp: u64,
}

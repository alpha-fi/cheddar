use near_sdk::json_types::U128;
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

    /// Returns amount of staked NEAR and farmed CHEDDAR of given account.
    fn status(&self, account_id: AccountId) -> (U128, U128);
}

#[ext_contract(ext_self)]
pub trait ExtStakingPool {
    fn withdraw_callback(&mut self, sender_id: AccountId, amount: U128);
}

#[ext_contract(ext_ft)]
pub trait FungibleToken {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
}

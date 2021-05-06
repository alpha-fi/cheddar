use near_sdk::json_types::{ValidAccountId, U128};
use near_sdk::PromiseOrValue;

#[ext_contract(ext_staking_pool)]
pub trait ExtStakingPool {
    /// Stakes the attached amount of $NEAR. Minimum 0.01 NEAR is required.
    #[payable]
    fn stake(&mut self, amount: U128);
    // fn stake_all(&mut self);

    /// Unstakes the given amount of $NEAR and sends it back to the predecessor
    #[payable]
    fn unstake(&mut self, amount: U128);
    // fn unstake_all(&mut self);

    fn withdraw_crop(&mut self, amount: U128);

    /****************/
    /* View methods */
    /****************/

    /// Returns amount of staked tokens of given account.
    fn get_stake(&self, account_id: Account_id) -> U128;

    /// Returns amount of farmed tokens for the given account which can be withdrawn.
    fn get_crop(&self, account_id: Account_id) -> U128;
}

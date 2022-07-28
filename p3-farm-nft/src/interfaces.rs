use crate::*;
use near_sdk::serde::{Deserialize, Serialize};

// #[ext_contract(ext_staking_pool)]
pub trait StakingPool {
    // #[payable]
    // [internal] comes from NonFungibleTokenReceiver
    // fn stake(&mut self, previous_owner_id: &AccountId, nft_contract_id: &NftContractId, token_id: Option<TokenId>) -> bool;

    // #[payable]
    fn unstake(&mut self, nft_contract_id: &NftContractId, token_id: Option<TokenId>) -> Vec<TokenId>;

    fn withdraw_crop(&mut self);

    /****************/
    /* View methods */
    /****************/

    /// Returns info about registered Account
    fn status(&self, account_id: AccountId) -> Option<Status>;
}

#[ext_contract(ext_self)]
pub trait ExtSelf {
    fn transfer_staked_callback(
        &mut self,
        user: AccountId,
        nft_contract_i: usize,
        token_id: TokenId,
        //fee: U128
    );
    fn transfer_farmed_callback(&mut self, user: AccountId, token_i: usize, amount: U128);
    fn transfer_staked_cheddar_callback(&mut self, user: AccountId, amount: U128);
    fn withdraw_nft_callback(&mut self, user: AccountId, contract_and_token_id: ContractNftTokenId);
    fn withdraw_fees_callback(&mut self, token_i: usize, amount: U128);
}

#[ext_contract(ext_ft)]
pub trait FungibleToken {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
}

#[ext_contract(ext_nft)]
pub trait NonFungibleToken {
    fn nft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        token_id: String,
        approval_id: Option<u64>,
        memo: Option<String>,
        msg: String
    );
    fn nft_transfer(
        &mut self,
        receiver_id: AccountId,
        token_id: String,
        approval_id: Option<u64>,
        memo: Option<String>,
    );
}
#[derive(Debug, Deserialize, Serialize)]
pub struct ContractParams {
    pub is_active: bool,
    pub owner_id: AccountId,
    pub stake_tokens: Vec<NftContractId>,
    pub stake_rates: Vec<U128>,
    pub farm_unit_emission: U128,
    pub farm_tokens: Vec<AccountId>,
    pub farm_token_rates: Vec<U128>,
    pub farm_deposits: Vec<U128>,
    pub farming_start: u64,
    pub farming_end: u64,
    /// NFT token used for boost
    pub boost_nft_contracts: Vec<NftContractId>,
    /// total staked is total amount of NFT tokens staked to farm
    pub total_staked: Vec<U128>,
    /// total farmed is total amount of tokens farmed (not necessary minted - which would be
    /// total_harvested).
    pub total_farmed: Vec<U128>,
    /// total boost is total amount of NFT tokens staked as a boost
    pub total_boost: Vec<U128>,
    pub fee_rate: U128,
    /// Number of accounts currently registered.
    pub accounts_registered: u64,
    /// Cheddar deposits
    pub cheddar_rate: U128,
    pub cheddar: AccountId
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Status {
    pub stake_tokens: Vec<TokenIds>,
    /// the min stake
    pub stake: U128,
    /// Amount of accumulated, not withdrawn farmed units. This is the base farming unit which
    /// is translated into `farmed_tokens`.
    pub farmed_units: U128,
    /// Amount of accumulated, not withdrawn farmed tokens in the same order as
    /// contract `farm_tokens`. Computed based on `farmed_units` and the contarct
    /// `farmed_token_rates.`
    pub farmed_tokens: Vec<U128>,
    /// token ID of a staked NFT boost. Empty if user doesn't stake any required boost NFT.
    pub boost_nfts: TokenId,
    /// timestamp (in seconds) of the current round.
    pub timestamp: u64,
    /// Cheddar stake
    pub total_cheddar_staked: U128
}

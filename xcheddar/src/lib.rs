use near_contract_standards::fungible_token::metadata::{
    FungibleTokenMetadata, FungibleTokenMetadataProvider, FT_METADATA_SPEC,
};
use near_contract_standards::fungible_token::FungibleToken;

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::U128;
use near_sdk::{env, log, ext_contract, near_bindgen, AccountId, Balance, PanicOnDefault, Promise, PromiseOrValue};

use crate::utils::*;
pub use crate::views::ContractMetadata;

mod xcheddar;
mod utils;
mod owner;
mod views;
mod storage_impl;

#[ext_contract(ext_cheddar)]
pub trait ExtCheddar {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    pub ft: FungibleToken,
    pub owner_id: AccountId,
    pub locked_token: AccountId,
    /// deposit reward that does not distribute to locked Cheddar yet
    pub undistributed_reward: Balance,
    /// locked amount
    pub locked_token_amount: Balance,
    /// the previous distribution time in seconds
    pub prev_distribution_time_in_sec: u32,
    /// when would the reward starts to distribute
    pub reward_genesis_time_in_sec: u32,
    /// 30-days period reward
    pub monthly_reward: Balance,
    /// current account number in contract
    pub account_number: u64,
}

#[near_bindgen]
impl Contract {
    #[init]
    //Initialize with setting the reward genesis time into 30 days from init time

    pub fn new(owner_id: AccountId, locked_token: AccountId) -> Self {
        let initial_reward_genisis_time = DURATION_30DAYS_IN_SEC + nano_to_sec(env::block_timestamp());
        Contract {
            ft: FungibleToken::new(b"a".to_vec()),
            owner_id: owner_id.into(),
            locked_token: locked_token.into(),
            undistributed_reward: 0,
            locked_token_amount: 0,
            prev_distribution_time_in_sec: initial_reward_genisis_time,
            reward_genesis_time_in_sec: initial_reward_genisis_time,
            monthly_reward: 0,
            account_number: 0,
        }
    }
}

near_contract_standards::impl_fungible_token_core!(Contract, ft);

#[near_bindgen]
impl FungibleTokenMetadataProvider for Contract {
    fn ft_metadata(&self) -> FungibleTokenMetadata {
        //XCHEDDAR icon
        let data_url = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 56 56'>
        <style>.a{fill:#F4C647;}.b{fill:#EEAF4B;}</style>
        <path d='M45 19.5v5.5l4.8 0.6 0-11.4c-0.1-3.2-11.2-6.7-24.9-6.7 -13.7 0-24.8 3.6-24.9 6.7L0 32.5c0 3.2 10.7 7.1 24.5 7.1 0.2 0 0.3 0 0.5 0V21.5l-4.7-7.2L45 19.5z' class='a'/>
        <path d='M25 31.5v-10l-4.7-7.2L45 19.5v5.5l-14-1.5v10C31 33.5 25 31.5 25 31.5z' fill='#F9E295'/>
        <path d='M24.9 7.5C11.1 7.5 0 11.1 0 14.3s10.7 7.2 24.5 7.2c0.2 0 0.3 0 0.5 0l-4.7-7.2 25 5.2c2.8-0.9 4.4-4 4.4-5.2C49.8 11.1 38.6 7.5 24.9 7.5z' class='b'/>
        <path d='M36 29v19.6c8.3 0 15.6-1 20-2.5V26.5L31 23.2 36 29z' class='a'/>
        <path d='M31 23.2l5 5.8c8.2 0 15.6-1 19.9-2.5L31 23.2z' class='b'/>
        <polygon points='36 29 36 48.5 31 42.5 31 23.2 ' fill='#FCDF76'/></svg>";

        FungibleTokenMetadata {
            spec: FT_METADATA_SPEC.to_string(),
            name: String::from("xCheddar Token"),
            symbol: String::from("xCHEDDAR"),
            icon: Some(String::from(data_url)),
            reference: None,
            reference_hash: None,
            decimals: 24,
        }
    }
}

use std::convert::TryInto;

use near_contract_standards::non_fungible_token::TokenId;
use near_sdk::json_types::U128;
use near_sdk::{AccountId, Balance};

use crate::constants::*;
use crate::vault::TokenIds;

use uint::construct_uint;
construct_uint! {
    /// 256-bit unsigned integer.
    pub struct U256(4);
}

pub fn farmed_tokens(units: u128, rate: Balance) -> Balance {
    println!("token_units(staked nft for this contract): {} ", units);
    println!("rate for them {} ", rate);
    (U256::from(units) * U256::from(rate) / big_e24()).as_u128()
}

#[allow(non_snake_case)]
pub fn to_U128s(v: &Vec<Balance>) -> Vec<U128> {
    v.iter().map(|x| U128::from(*x)).collect()
}

pub fn find_acc_idx(acc: &AccountId, acc_v: &Vec<AccountId>) -> Option<usize> {
    Some(acc_v.iter().position(|x| x == acc).expect("invalid nft contract"))
}
pub fn find_token_idx(token: &TokenId, token_v: &Vec<TokenId>) -> Option<usize> {
    Some(token_v.iter().position(|x| x == token).expect("invalid token"))
}

pub fn min_stake(staked: &Vec<TokenIds>, stake_rates: &Vec<u128>) -> Balance {
    let mut min = std::u128::MAX;
    for (i, rate) in stake_rates.iter().enumerate() {
        println!("staked tokens for nft_contract[i]: {:?}", staked[i]);
        let staked_tokens:u128 = staked[i].len() as u128 * E24; // Number of NFT tokens for nft_contract[i] as e24
        let s = farmed_tokens(staked_tokens, *rate);
        if s < min {
            min = s;
        }
    }
    return min;
}

pub fn all_zeros(v: &Vec<TokenIds>) -> bool {
    for x in v {
        if !x.is_empty() {
            return false;
        }
    }
    return true;
}

/// computes round number based on timestamp in seconds
pub fn round_number(start: u64, end: u64, mut now: u64) -> u64 {
    if now < start {
        return 0;
    }
    // we start rounds from 0
    let mut adjust = 0;
    if now >= end {
        now = end;
        // if at the end of farming we don't start a new round then we need to force a new round
        if now % ROUND != 0 {
            adjust = 1
        };
    }
    let r: u64 = ((now - start) / ROUND).try_into().unwrap();
    r + adjust
}

pub fn near() -> AccountId {
    NEAR_TOKEN.parse::<AccountId>().unwrap()
}

pub fn big_e24() -> U256 {
    U256::from(E24)
}

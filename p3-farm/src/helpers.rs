use near_sdk::json_types::U128;
use near_sdk::{AccountId, Balance};

use crate::constants::*;

use uint::construct_uint;
construct_uint! {
    /// 256-bit unsigned integer.
    struct U256(4);
}

pub fn farmed_tokens(units: Balance, rate: Balance) -> Balance {
    let e24_big: U256 = U256::from(E24);
    (U256::from(units) * U256::from(rate) / e24_big).as_u128()
}

pub fn to_u128_vec(v: &Vec<Balance>) -> Vec<U128> {
    v.iter().map(|x| U128::from(*x)).collect()
}

pub fn find_acc_idx(acc: &AccountId, acc_v: &Vec<AccountId>) -> usize {
    acc_v.iter().position(|x| x == acc).expect("invalid token")
}

pub fn min_stake(staked: &Vec<u128>, stake_rates: &Vec<u128>) -> Balance {
    let mut min: u128 = 1 << 128 - 1;
    for (i, rate) in stake_rates.iter().enumerate() {
        let s = farmed_tokens(staked[i], *rate);
        if s < min {
            min = s;
        }
    }
    return min;
}

pub fn all_zeros(v: &Vec<Balance>) -> bool {
    for x in v {
        if *x != 0 {
            return false;
        }
    }
    return true;
}

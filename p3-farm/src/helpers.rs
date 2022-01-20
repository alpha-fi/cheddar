use near_sdk::json_types::{ValidAccountId, U128};
use near_sdk::{AccountId, Balance};

pub fn to_u128_vec(v: &Vec<u128>) -> Vec<U128> {
    v.map(|x| x.into()).collect()
}

fn find_acc_idx(acc: &AccountId, acc_v: &Vec<AccountId>) -> Option<usize> {
    acc_v.iter().position(|x| x == acc)
}

use near_sdk::json_types::{ValidAccountId, U128};
use near_sdk::{AccountId, Balance};

pub fn to_u128_vec(v: &Vec<Balance>) -> Vec<U128> {
    v.iter().map(|x| U128::from(*x)).collect()
}

pub fn find_acc_idx(acc: &AccountId, acc_v: &Vec<AccountId>) -> usize {
    acc_v
        .iter()
        .position(|x| x == acc)
        .expect("token not registered")
}

pub fn all_zeros(v: &Vec<Balance>) -> bool {
    for x in v {
        if *x != 0 {
            return false;
        }
    }
    return true;
}

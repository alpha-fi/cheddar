use near_sdk::json_types::{ValidAccountId, U128};
use near_sdk::{AccountId, Balance};

const E24_BIG: U256 = U256::from(E24);

use uint::construct_uint;
construct_uint! {
    /// 256-bit unsigned integer.
    struct U256(4);
}

pub fn to_u128_vec(v: &Vec<Balance>) -> Vec<U128> {
    v.iter().map(|x| U128::from(*x)).collect()
}

pub fn find_acc_idx(acc: &AccountId, acc_v: &Vec<AccountId>) -> usize {
    acc_v
        .iter()
        .position(|x| x == acc)
        .expect("token not registered")
}

pub fn min_stake(staked: &Vec<u128>, stake_rates: &Vec<u128>) -> Balance {
    let mut min: u128 = 1 << 128 - 1;
    for (i, rate) in stake_rates.iter().enumerate() {
        let s = (U256::from(staked[i]) * U256::from(*rate) / E24_BIG).as_u128();
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

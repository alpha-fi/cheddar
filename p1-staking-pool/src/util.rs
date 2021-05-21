//use near_sdk::json_types::{U128, U64};
use near_sdk::env;
use uint::construct_uint;

use crate::constants::*;

// pub type U128String = U128;
// pub type U64String = U64;

construct_uint! {
    /// 256-bit unsigned integer.
    pub struct U256(4);
}

pub fn current_round() -> u64 {
    env::block_timestamp() / ROUND
}

pub fn round_from(unix_timestamp:u64) -> u64 {
    (unix_timestamp as u128 * NANOSECONDS as u128 / ROUND as u128) as u64
}

pub fn unix_timestamp_from(round:u64) -> u64 {
    (round as u128 * ROUND as u128 / NANOSECONDS as u128) as u64
}

// returns amount * numerator/denominator
// pub fn proportional(amount: u128, numerator: u128, denominator: u128) -> u128 {
//     return (U256::from(amount) * U256::from(numerator) / U256::from(denominator)).as_u128();
// }

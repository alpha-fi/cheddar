//use near_sdk::json_types::{U128, U64};
use near_sdk::env;
use uint::construct_uint;

use crate::constants::*;

pub type Round = u64;

const SEC_PER_ROUND: u64 = ROUND / NANOSECONDS;

construct_uint! {
    /// 256-bit unsigned integer.
    pub struct U256(4);
}

pub fn current_round() -> Round {
    env::block_timestamp() / ROUND
}

pub fn round_from_unix(unix_timestamp: u64) -> Round {
    unix_timestamp * SEC_PER_ROUND
}

pub fn round_to_unix(round: Round) -> u64 {
    round / SEC_PER_ROUND
}

// returns amount * numerator/denominator
// pub fn proportional(amount: u128, numerator: u128, denominator: u128) -> u128 {
//     return (U256::from(amount) * U256::from(numerator) / U256::from(denominator)).as_u128();
// }

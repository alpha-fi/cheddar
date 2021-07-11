//use near_sdk::json_types::{U128, U64};
use near_sdk::env;
use uint::construct_uint;

use crate::constants::*;

pub type Round = u64;

const SEC_PER_ROUND: u64 = ROUND / SECOND;

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

#[macro_export]
macro_rules! env_log {
    ($($arg:tt)*) => {{
        let msg = format!($($arg)*);
        // io::_print(msg);
        println!("{}", msg);
        env::log(msg.as_bytes())
    }}
}

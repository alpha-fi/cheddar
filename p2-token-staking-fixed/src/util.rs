//use near_sdk::json_types::{U128, U64};
// use near_sdk::env;
// use crate::constants::*;

use uint::construct_uint;
construct_uint! {
    /// 256-bit unsigned integer.
    pub struct U256(4);
}

pub fn big(x: u128) -> U256 {
    x.into()
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

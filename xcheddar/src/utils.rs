use chrono::prelude::*;
use near_sdk::{Balance, Gas, Timestamp};

use uint::construct_uint;

pub const CHEDDAR_DECIMALS: u8 = 24;
pub const ONE_YOCTO: u128 = 1;
pub const GAS_FOR_RESOLVE_TRANSFER: Gas = Gas(10_000_000_000_000);
pub const GAS_FOR_FT_TRANSFER: Gas = Gas(20_000_000_000_000);

pub const DURATION_30DAYS_IN_SEC: u32 = 60 * 60 * 24 * 30;

pub const ERR_RESET_TIME_IS_PAST_TIME: &str = "Used reward_genesis_time_in_sec must be less than current time!";
pub const ERR_REWARD_GENESIS_TIME_PASSED: &str = "Setting in contract Genesis time must be less than current time!";
pub const ERR_NOT_ALLOWED: &str = "Owner's method";
pub const ERR_NOT_INITIALIZED: &str = "State was not initialized!"; 
pub const ERR_INTERNAL: &str = "Amount of locked token must be greater than 0";
pub const ERR_STAKE_TOO_SMALL: &str = "Stake more than 0 tokens";
pub const ERR_EMPTY_TOTAL_SUPPLY: &str = "Total supply cannot be empty!";
pub const ERR_KEEP_AT_LEAST_ONE_XCHEDDAR: &str = "At least 1 Cheddar must be on lockup contract account";
pub const ERR_MISMATCH_TOKEN: &str = "Only Cheddar tokrn contract may calls this lockup contract";
pub const ERR_PROMISE_RESULT: &str = "Expected 1 promise result";
pub const ERR_WRONG_TRANSFER_MSG: &str = "Use empty msg for deposit to stake Cheddar or msg = 'reward' for add some reward to contract";

construct_uint! {
    // 256-bit unsigned integer.
    pub struct U256(4);
}

pub fn nano_to_sec(nano: Timestamp) -> u32 {
    (nano / 1_000_000_000) as u32
}

pub fn convert_from_yocto_cheddar(yocto_amount: Balance) -> u128 {
    (yocto_amount + (5 * 10u128.pow((CHEDDAR_DECIMALS - 1u8).into()))) / 10u128.pow(CHEDDAR_DECIMALS.into())
}
pub fn convert_timestamp_to_datetime(timestamp: u32) -> DateTime<Utc> {
    let naive_datetime = NaiveDateTime::from_timestamp(timestamp.into(), 0);
    DateTime::from_utc(naive_datetime, Utc)
}


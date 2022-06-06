#![allow(unused)] 
use xcheddar_token::ContractMetadata;
use near_sdk_sim::ExecutionResult;
use uint::construct_uint;

pub const DURATION_30DAYS_IN_SEC: u32 = 60 * 60 * 24 * 30;

construct_uint! {
    /// 256-bit unsigned integer.
    pub struct U256(4);
}

pub fn assert_xcheddar(
    current_xcheddar: &ContractMetadata,
    undistributed_reward: u128,
    locked_token_amount: u128,
    supply: u128,
    
) {
    assert_eq!(current_xcheddar.undistributed_reward.0, undistributed_reward);
    assert_eq!(current_xcheddar.locked_token_amount.0, locked_token_amount);
    assert_eq!(current_xcheddar.supply.0, supply);
}

pub fn get_error_count(r: &ExecutionResult) -> u32 {
    r.promise_errors().len() as u32
}

pub fn get_error_status(r: &ExecutionResult) -> String {
    format!("{:?}", r.promise_errors()[0].as_ref().unwrap().status())
}

pub fn nano_to_sec(nano: u64) -> u32 {
    (nano / 1_000_000_000) as u32
}

pub fn sec_to_nano(sec: u32) -> u64 {
    sec as u64 * 1_000_000_000 as u64
}

use near_sdk::{Balance, Gas};

/// Amount of gas for fungible token transfers.
pub const TGAS: Gas = 1_000_000_000_000;
pub const GAS_FOR_FT_TRANSFER: Gas = 10 * TGAS;
pub const GAS_FOR_MINT_CALLBACK: Gas = 20 * TGAS;
pub const GAS_FOR_MINT_CALLBACK_FINALLY: Gas = 8 * TGAS;

pub const ONE_YOCTO: Balance = 1;

/// one second in nanoseconds
pub const SECOND: u64 = 1_000_000_000;
/// round duration in seconds
pub const ROUND: u64 = 60; // 1 minute
pub const ROUND_NS: u64 = 60 * 1_000_000_000; // round duration in nanoseconds

const MILLI_NEAR: Balance = 1000_000000_000000_000000; // 1e21
pub const STORAGE_COST: Balance = MILLI_NEAR * 60; // 0.06 NEAR
/// E24 is 1 in yocto
pub const E24: Balance = MILLI_NEAR * 1_000;

pub const MAX_STAKE: Balance = E24 * 100_000;

/// accumulator overflow, used to correctly update the self.s accumulator.
// TODO: need to addjust it accordingly to the reward rate and the staked token.
// Eg: if
pub const ACC_OVERFLOW: Balance = 10_000_000; // 1e7

// TOKENS
pub const NEAR_TOKEN: &str = "near";

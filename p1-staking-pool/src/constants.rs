use near_sdk::{Balance, Gas};

// pub const GAS_FOR_RESOLVE_TRANSFER: Gas = 5_000_000_000_000;
// pub const GAS_FOR_FT_TRANSFER_CALL: Gas = 25_000_000_000_000 + GAS_FOR_RESOLVE_TRANSFER;

/// Amount of gas for fungible token transfers.
pub const GAS_FOR_FT_TRANSFER: Gas = 10_000_000_000_000;

pub const NO_DEPOSIT: Balance = 0;
pub const ONE_YOCTO: Balance = 1;

/// one second in nanoseconds
pub const SECOND: u64 = 1_000_000_000;
/// epoch = 1 minute
pub const EPOCH: u64 = 60 * SECOND;

// 1/10_000 of a NEAR
const MILLI_NEAR: Balance = 1000_000000_000000_000000;
pub const MIN_STAKE: Balance = MILLI_NEAR * 10;

pub const SECONDS_PER_YEAR: u32 = 3600 * 24 * 365;

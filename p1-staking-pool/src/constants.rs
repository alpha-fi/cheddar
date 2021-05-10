use near_sdk::{Balance, Gas};

// pub const GAS_FOR_RESOLVE_TRANSFER: Gas = 5_000_000_000_000;
// pub const GAS_FOR_FT_TRANSFER_CALL: Gas = 25_000_000_000_000 + GAS_FOR_RESOLVE_TRANSFER;

/// Amount of gas for fungible token transfers.
pub const GAS_FOR_FT_TRANSFER: Gas = 10_000_000_000_000;

// pub const NO_DEPOSIT: Balance = 0;

/// one day in nanoseconds (NEAR is using nanoseconds)
pub const ONE_MINUTE: u64 = 60 * 1_000_000_000;
pub const EPOCH_DURATION: u64 = ONE_MINUTE;

const MILI_NEAR: Balance = 1000_000000_000000_000000;
// pub const ONE_NEAR: Balance = mNEAR * 1000;
pub const MIN_STAKE: Balance = MILI_NEAR * 10;

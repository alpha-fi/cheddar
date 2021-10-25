use near_sdk::{Balance, Gas};

// pub const GAS_FOR_RESOLVE_TRANSFER: Gas = 5_000_000_000_000;
// pub const GAS_FOR_FT_TRANSFER_CALL: Gas = 25_000_000_000_000 + GAS_FOR_RESOLVE_TRANSFER;

/// Amount of gas for fungible token transfers.
pub const TGAS: Gas = 1_000_000_000_000;
pub const GAS_FOR_FT_TRANSFER: Gas = 10 * TGAS;
pub const GAS_FOR_MINT_CALLBACK: Gas = 20 * TGAS;
pub const GAS_FOR_MINT_CALLBACK_FINALLY: Gas = 8 * TGAS;

pub const NO_DEPOSIT: Balance = 0;
pub const ONE_YOCTO: Balance = 1;

/// one second in nanoseconds
pub const NANOSECONDS: u64 = 1_000_000_000;
/// round duration in nanoseconds
pub const ROUND: u64 = NANOSECONDS; // 1 sec
pub const ROUNDS_PER_MINUTE: u64 = 60; // 1 minute of rounds

// AUDIT: fixed comment
// 1/1_000 of a NEAR
const MILLI_NEAR: Balance = 1000_000000_000000_000000;
pub const E24: Balance = MILLI_NEAR * 1_000;
pub const STAKE: Balance = E24;

pub const MAX_STAKE: Balance = E24 * 100_000;

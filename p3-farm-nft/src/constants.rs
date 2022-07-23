use near_sdk::{Balance, Gas, AccountId};
/// Gas constants
/// Amount of gas for fungible token transfers.
pub const TGAS: u64 = Gas::ONE_TERA.0;

pub const GAS_FOR_FT_TRANSFER: Gas = Gas(10 * TGAS);
pub const GAS_FOR_NFT_TRANSFER: Gas = Gas(20 * TGAS);
pub const GAS_FOR_CALLBACK: Gas = Gas(20 * TGAS);

/// Contract constants ( Stake & Farm )
/// Cheddar contract
pub const CHEDDAR_CONTRACT: &str = "token.v3.cheddar.testnet";
/// one second in nanoseconds
pub const SECOND: u64 = 1_000_000_000;
/// round duration in seconds
pub const ROUND: u64 = 60; // 1 minute
pub const ROUND_NS: u64 = 60 * 1_000_000_000; // round duration in nanoseconds
pub const MAX_STAKE: Balance = E24 * 100_000;
/// accumulator overflow, used to correctly update the self.s accumulator.
// TODO: need to addjust it accordingly to the reward rate and the staked token.
// Eg: if
pub const ACC_OVERFLOW: Balance = 10_000_000; // 1e7

/// NEAR Constants
pub const NEAR_TOKEN:&str = "near";
const MILLI_NEAR: Balance = 1000_000000_000000_000000; // 1e21 yoctoNear
pub const STORAGE_COST: Balance = MILLI_NEAR * 60; // 0.06 NEAR
/// E24 is 1 in yocto (1e24 yoctoNear)
pub const E24: Balance = MILLI_NEAR * 1_000;
pub const ONE_YOCTO: Balance = 1;


/// NFT constants
pub(crate) type NftContractId = AccountId;
/// NFT Delimeter
// pub const NFT_DELIMETER: &str = "@";
/// Cheddy boost constant
pub const BASIS_P: Balance = 10_000;
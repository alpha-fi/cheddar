use near_sdk::json_types::{U128, U64};
use near_sdk::{Balance, Gas};
use uint::construct_uint;

pub type U128String = U128;
pub type U64String = U64;

/// One Tera gas (Tgas), which is 10^12 gas units.
#[allow(dead_code)]
pub const ONE_TERA: Gas = Gas(1_000_000_000_000);
/// 5 Tgas
pub const GAS_FOR_RESOLVE_TRANSFER: Gas = Gas(5_000_000_000_000);
/// 30 Tgas (25 Tgas + GAS_FOR_RESOLVE_TRANSFER)
pub const GAS_FOR_FT_TRANSFER_CALL: Gas = Gas(30_000_000_000_000);
pub const NO_DEPOSIT: Balance = 0;

construct_uint! {
    /// 256-bit unsigned integer.
    pub struct U256(4);
}

/// returns amount * numerator/denominator
pub fn fraction_of(amount: u128, numerator: u128, denominator: u128) -> u128 {
    return (U256::from(amount) * U256::from(numerator) / U256::from(denominator)).as_u128();
}

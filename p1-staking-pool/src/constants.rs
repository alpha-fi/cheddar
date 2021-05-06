const GAS_FOR_RESOLVE_TRANSFER: Gas = 5_000_000_000_000;
const GAS_FOR_FT_TRANSFER_CALL: Gas = 25_000_000_000_000 + GAS_FOR_RESOLVE_TRANSFER;
const NO_DEPOSIT: Balance = 0;

/// one day in nanoseconds (NEAR is using nanoseconds)
const ONE_DAY: u64 = 24 * 3600 * 1_000_000_000;
/// one week in nanoseconds (NEAR is using nanoseconds)
const EPOCH_DURATION: u64 = ONE_DAY * 7;

pub const ERR01_MIN_STAKE: &str = "E01: min stake amount is 0.01 NEAR";
pub const ERR02_MIN_BALANCE: &str =
    "E02: can not unstake everything without closing the account. Call close() instead";
pub const ERR03_STORAGE_DEP: &str = "E03: min storage deposit is 0.05 NEAR";

// Token registration

pub const ERR10_NO_ACCOUNT: &str = "E10: account not found. Register the account.";

// Token Deposit errors //

// pub const ERR21_TOKEN_NOT_REG: &str = "E21: token not registered";
pub const ERR22_NOT_ENOUGH_TOKENS: &str = r#"E22: not enough tokens in deposit"#;
pub const ERR23_NOT_ENOUGH_NEAR: &str = "E23: not enough NEAR in deposit";
// pub const ERR24_NON_ZERO_TOKEN_BALANCE: &str = "E24: non-zero token balance";
pub const ERR25_WITHDRAW_CALLBACK: &str = "E25: expected 1 promise result from withdraw";

// TOKEN STAKED
pub const ERR30_NOT_ENOUGH_STAKE: &str = "E30: not enough staked tokens";

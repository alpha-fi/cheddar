pub mod constants {
    use near_sdk::{Balance, Gas};

    /// Gas constants
    /// Amount of gas for fungible token transfers.
    pub const TGAS: Gas = Gas::ONE_TERA;
    pub const GAS_FOR_FT_TRANSFER: Gas = Gas(10 * TGAS.0);
    pub const GAS_FOR_NFT_TRANSFER: Gas = Gas(20 * TGAS.0);
    pub const GAS_FOR_CALLBACK: Gas = Gas(5 * TGAS.0);
    pub const GAS_FOR_MINT_CALLBACK: Gas = Gas(20 * TGAS.0);

    /// one second in nanoseconds
    pub const SECOND: u64 = 1_000_000_000;
    /// round duration in seconds
    pub const ROUND: u64 = 60; // 1 minute
    pub const ROUND_NS: u64 = 60 * 1_000_000_000; // round duration in nanoseconds

    pub const ONE_YOCTO: Balance = 1;
    const MILLI_NEAR: Balance = 1000_000000_000000_000000; // 1e21
    pub const STORAGE_COST: Balance = MILLI_NEAR * 60; // 0.06 NEAR
    /// E24 is 1 in yocto
    pub const E24: Balance = MILLI_NEAR * 1_000;

    pub const BASIS_P: Balance = 10_000;

    pub const MAX_STAKE: Balance = E24 * 100_000;

    /// accumulator overflow, used to correctly update the self.s accumulator.
    // TODO: need to addjust it accordingly to the reward rate and the staked token.
    // Eg: if
    pub const ACC_OVERFLOW: Balance = 10_000_000; // 1e7

    // TOKENS
    pub const NEAR_TOKEN: &str = "near";
}

pub mod errors {
    // Token registration

    pub const ERR10_NO_ACCOUNT: &str = "E10: account not found. Register the account.";

    // Token Deposit errors //

    // TOKEN STAKED
    pub const ERR30_NOT_ENOUGH_STAKE: &str = "E30: not enough staked tokens";

    // TODO errors-covered all cases
}

pub mod helpers {
    use crate::constants::*;
    use near_sdk::json_types::U128;
    use near_sdk::{AccountId, Balance};
    use uint::construct_uint;

    construct_uint! {
        /// 256-bit unsigned integer.
        pub struct U256(4);
    }

    pub fn near() -> AccountId {
        NEAR_TOKEN.parse::<AccountId>().unwrap()
    }

    pub fn safe_mul(units: Balance, rate: Balance) -> Balance {
        let e24_big: U256 = U256::from(E24);
        (U256::from(units) * U256::from(rate) / e24_big).as_u128()
    }

    #[allow(non_snake_case)]
    pub fn to_U128s(v: &Vec<Balance>) -> Vec<U128> {
        v.iter().map(|x| U128::from(*x)).collect()
    }

    pub fn find_acc_idx(acc: &AccountId, acc_v: &Vec<AccountId>) -> usize {
        acc_v.iter().position(|x| x == acc).expect("invalid token")
    }

    pub fn check_all_zeros(v: &Vec<Balance>) -> bool {
        for x in v {
            if *x != 0 {
                return false;
            }
        }
        return true;
    }

    /// computes round number based on timestamp in seconds
    pub fn round_number(start: u64, end: u64, mut now: u64) -> u64 {
        if now < start {
            return 0;
        }
        // we start rounds from 0
        let mut adjust = 0;
        if now >= end {
            now = end;
            // if at the end of farming we don't start a new round then we need to force a new round
            if now % ROUND != 0 {
                adjust = 1
            };
        }
        let r: u64 = ((now - start) / ROUND).try_into().unwrap();
        r + adjust
    }
}

pub mod interfaces {
    use near_sdk::json_types::U128;
    use near_sdk::{ext_contract, AccountId};

    #[ext_contract(ext_self)]
    pub trait ExtSelf {
        fn transfer_staked_callback(
            &mut self,
            user: AccountId,
            token_i: usize,
            amount: U128,
            fee: U128,
        );
        fn transfer_farmed_callback(&mut self, user: AccountId, token_i: usize, amount: U128);
        fn withdraw_nft_callback(&mut self, user: AccountId, cheddy: String);
        fn withdraw_fees_callback(&mut self, token_i: usize, amount: U128);
        fn mint_callback(&mut self, user: AccountId, amount: U128);
        fn mint_callback_finally(&mut self);
        fn close_account(&mut self, user: AccountId);
    }

    #[ext_contract(ext_ft)]
    pub trait FungibleToken {
        fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
        fn ft_mint(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
    }

    #[ext_contract(ext_nft)]
    pub trait NonFungibleToken {
        fn nft_transfer(
            &mut self,
            receiver_id: AccountId,
            token_id: String,
            approval_id: Option<u64>,
            memo: Option<String>,
        );
        fn nft_transfer_call(
            &mut self,
            receiver_id: AccountId,
            token_id: String,
            approval_id: Option<u64>,
            memo: Option<String>,
            msg: String,
        );
    }
}

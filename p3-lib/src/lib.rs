
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
    pub const NEAR_TOKEN:&str = "near";
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
    use near_sdk::{AccountId, Balance};
    use near_sdk::json_types::U128;
    use uint::construct_uint;
    use crate::constants::*;

    construct_uint! {
        /// 256-bit unsigned integer.
        pub struct U256(4);
    }
    
    pub fn near() -> AccountId {
        NEAR_TOKEN.parse::<AccountId>().unwrap()
    }
    
    pub fn farmed_tokens(units: Balance, rate: Balance) -> Balance {
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
    use near_sdk::serde::{Deserialize, Serialize};
    use near_sdk::{ext_contract, AccountId};

    // Vec<TokenId> types
    pub (crate) type TokenId = String;
    pub (crate) type TokenIds = Vec<TokenId>;
    pub (crate) type NftContractId = AccountId;

    // #[ext_contract(ext_staking_pool)]
    pub trait StakingPool {
        // #[payable]
        fn stake(&mut self, amount: U128);

        // #[payable]
        fn unstake(&mut self, amount: U128) -> U128;

        fn withdraw_crop(&mut self, amount: U128);

        /****************/
        /* View methods */
        /****************/

        /// Returns amount of staked NEAR and farmed CHEDDAR of given account & the unix-timestamp for the calculation.
        fn status(&self, account_id: AccountId) -> (U128, U128, u64);
    }

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
            msg: String
        );
    }

    #[derive(Deserialize, Serialize)]
    #[serde(crate="near_sdk::serde")]
    pub struct ContractParams {
        pub is_active: bool,
        pub owner_id: AccountId,
        pub stake_tokens: Vec<AccountId>,
        pub stake_rates: Vec<U128>,
        pub farm_unit_emission: U128,
        pub farm_tokens: Vec<AccountId>,
        pub farm_token_rates: Vec<U128>,
        pub farm_deposits: Vec<U128>,
        pub farming_start: u64,
        pub farming_end: u64,
        /// NFT token used for boost
        pub cheddar_nft: AccountId,
        pub total_staked: Vec<U128>,
        /// total farmed is total amount of tokens farmed (not necessary minted - which would be
        /// total_harvested).
        pub total_farmed: Vec<U128>,
        pub fee_rate: U128,
        /// Number of accounts currently registered.
        pub accounts_registered: u64,
    }

    #[derive(Deserialize, Serialize)]
    #[serde(crate="near_sdk::serde")]
    #[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
    pub struct P4ContractParams {
        pub is_active: bool,
        pub owner_id: AccountId,
        pub stake_tokens: Vec<NftContractId>,
        pub stake_rates: Vec<U128>,
        pub farm_unit_emission: U128,
        pub farm_tokens: Vec<AccountId>,
        pub farm_token_rates: Vec<U128>,
        pub farm_deposits: Vec<U128>,
        pub farming_start: u64,
        pub farming_end: u64,
        /// NFT token used for boost
        pub boost_nft_contracts: Vec<NftContractId>,
        /// total staked is total amount of NFT tokens staked to farm
        pub total_staked: Vec<U128>,
        /// total farmed is total amount of tokens farmed (not necessary minted - which would be
        /// total_harvested).
        pub total_farmed: Vec<U128>,
        /// total boost is total amount of NFT tokens staked as a boost
        pub total_boost: Vec<U128>,
        pub fee_rate: U128,
        /// Number of accounts currently registered.
        pub accounts_registered: u64,
        /// Cheddar deposits
        pub cheddar_rate: U128,
        pub cheddar: AccountId
    }

    #[derive(Deserialize, Serialize)]
    #[serde(crate="near_sdk::serde")]
    pub struct Status {
        pub stake_tokens: Vec<U128>,
        /// the min stake
        pub stake: U128,
        /// Amount of accumulated, not withdrawn farmed units. This is the base farming unit which
        /// is translated into `farmed_tokens`.
        pub farmed_units: U128,
        /// Amount of accumulated, not withdrawn farmed tokens in the same order as
        /// contract `farm_tokens`. Computed based on `farmed_units` and the contarct
        /// `farmed_token_rates.`
        pub farmed_tokens: Vec<U128>,
        /// token ID of a staked Cheddy. Empty if user doesn't stake any Cheddy.
        pub cheddy_nft: String,
        /// timestamp (in seconds) of the current round.
        pub timestamp: u64,
    }

    #[derive(Deserialize, Serialize)]
    #[serde(crate="near_sdk::serde")]
    #[cfg_attr(not(target_arch = "wasm32"), derive(Debug, Clone))]
    pub struct P4Status {
        pub stake_tokens: Vec<TokenIds>,
        /// the min stake
        pub stake: U128,
        /// Amount of accumulated, not withdrawn farmed units. This is the base farming unit which
        /// is translated into `farmed_tokens`.
        pub farmed_units: U128,
        /// Amount of accumulated, not withdrawn farmed tokens in the same order as
        /// contract `farm_tokens`. Computed based on `farmed_units` and the contarct
        /// `farmed_token_rates.`
        pub farmed_tokens: Vec<U128>,
        /// token ID of a staked NFT boost. Empty if user doesn't stake any required boost NFT.
        pub boost_nfts: TokenId,
        /// timestamp (in seconds) of the current round.
        pub timestamp: u64,
        /// Cheddar stake
        pub total_cheddar_staked: U128
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}

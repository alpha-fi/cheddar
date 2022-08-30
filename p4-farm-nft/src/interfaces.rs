use crate::*;

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

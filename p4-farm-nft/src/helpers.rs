use std::convert::TryInto;

use crate::*;

// NFTs types
pub (crate) type TokenId = String;
pub (crate) type TokenIds = Vec<TokenId>;
pub (crate) type NftContractId = AccountId;
/// `contract_id@token_id` pair
pub (crate) type ContractNftTokenId = String;

/// NFT Delimeter
/// Using Paras-HQ standarts `NFT_DELIMETER` for `ContractNftTokenId` format
/// https://github.com/ParasHQ/paras-nft-farming-contract/blob/f762be16bc68a9c0da2c0ba30fbf555d78162074/ref-farming/src/utils.rs#L20
pub const NFT_DELIMETER: &str = "@";

/// Computing required amount of staked Cheddar based on
/// number of `staked_nft_tokens` and `Contract.cheddar_rate`
pub fn expected_cheddar_stake(num_nfts: usize, cheddar_rate: Balance) -> Balance {
    let expected_num_nfts_u128: Balance = (num_nfts + 1).try_into().unwrap();

    match expected_num_nfts_u128.checked_mul(cheddar_rate) {
        Some(balance) => balance,
        None => panic!("Math overflow while computing expected Cheddar stake")
    }
}

pub fn find_token_idx(token: &TokenId, token_v: &Vec<TokenId>) -> usize {
    token_v.iter().position(|x| x == token).expect("invalid token")
}

pub fn extract_contract_token_ids(contract_and_token_id: &ContractNftTokenId) -> (NftContractId, TokenId) {
    let contract_token_id_split: Vec<&str> = contract_and_token_id.split(NFT_DELIMETER).collect();
    assert!(contract_token_id_split.len() == 2 as usize, "expected 'contract_id@token_id' pair");
    let nft_contract_id:AccountId = contract_token_id_split[0].parse().unwrap();
    let token_id:TokenId = contract_token_id_split[1].to_string();
    (nft_contract_id, token_id)
}

pub fn min_stake(staked: &Vec<TokenIds>, stake_rates: &Vec<u128>) -> Balance {
    let mut min = std::u128::MAX;
    for (i, rate) in stake_rates.iter().enumerate() {
        let staked_tokens:u128 = staked[i].len() as u128 * E24; // Number of NFT tokens for nft_contract[i] as e24
        let s = farmed_tokens(staked_tokens, *rate);
        if s < min {
            min = s;
        }
    }
    return min;
}

pub fn all_zeros(v: &Vec<TokenIds>) -> bool {
    for x in v {
        if !x.is_empty() {
            return false;
        }
    }
    return true;
}

/// Returns true if the promise was failed. Otherwise returns false.
/// Fails if called outside a callback that received 1 promise result.
pub fn promise_result_as_failed() -> bool {
    require!(env::promise_results_count() == 1, "Contract expected a result on the callback");
    match env::promise_result(0) {
        PromiseResult::Failed => true,
        _ => false,
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn test_expected_cheddar() {
    let cheddar_rate = 555 * E24;
    assert_eq!(expected_cheddar_stake(10, cheddar_rate), 555 * (10 + 1) * E24);
    assert_eq!(expected_cheddar_stake(1, cheddar_rate), 555 * (1 + 1) * E24);
    assert_eq!(expected_cheddar_stake(0, cheddar_rate), 555 * E24);
    assert_eq!(expected_cheddar_stake(5, cheddar_rate), 555 * (5 + 1) * E24);
    assert_eq!(expected_cheddar_stake(30, cheddar_rate), 555 * (30 + 1) * E24);
}
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn test_extract_nft_contract_and_token_ids() {
    assert_eq!(extract_contract_token_ids(&("nft_contract.near@token_id123".into())), ("nft_contract.near".parse().unwrap(), "token_id123".into()));
    assert_eq!(extract_contract_token_ids(&("nft_contract.testnet@token_id123".into())), ("nft_contract.testnet".parse().unwrap(), "token_id123".into()));
}
#[cfg(not(target_arch = "wasm32"))]
#[test]
#[should_panic(expected="unexpected length of vector!")]
fn test_wrong_extract_nft_contract_and_token_ids() {
    let result = extract_contract_token_ids(&("nft_contract.near@token_id123@1".into()));
    dbg!("{:?}", result);
}
# P3 (NFT versioned) explanation 
## Function call sequences

#### fn storage_deposit () => StorageBalance
Registration in P3 contract. 
Return StorageBalance.
```rust
StorageBalance {
        total: STORAGE_COST.into(),
        available: U128::from(0),
    }
```
Reqiire attached deposit which equals to ```STORAGE_COST``` from ```constants.rs```

#### CROSS-CALL catching from stakeing token <=> fn nft_transfer_call(args)
##### msg from args is "cheddy"  => receive cheddy NFT and insert this one into user Vault
##### msg from args is "to farm" => fn internal_nft_stake () => bool
Staking / Add Cheddy NFT boost depends on ```msg``` from ```nft_transfer_call```
Panics when:
- In case of ToFarm when contract is paused
- In case of this called not as cross-contract call
- In case of NFT owner not a signer
Refund transfered token:
- Not allowed for stakeing NFT transfered into stake/boost
- Not registered signer (NFT owner)
- In case of Cheddy boost if it already was added to this user Vault before
- Wrong message or no message

#### [ fn internal_nft_stake () => bool ] (private)
Main logic for stake to farm.
Return ```true``` after vault changing and recomputing stake.
Return ```false``` if NFT not allowed to stake

Takes ```nft_contract_id``` and ```token_id``` from ```nft_transfer_call(args)``` and insert this to user vault as 
stake tokens(```vault.staked```) in format:
```rust
   
  [ [token_i, token_i, token_i...], [token_i, token_i, token_i...],... [token_i, token_i, token_i...] ]
//  ^------nft_contract_i--------^  ^------nft_contract_i--------^     ^------nft_contract_i--------^
```
Stake recomputing based on this token ids. Every time we are stake one token and it compute length of current user ```vault.staked[i].len()``` for this nft_contrtact_i. 
Farmed tokens counting based on amount of tokens which is actually as like in FT farming, but amount of NFT token units introduced as like ```vault.staked[i].len() * E24```. For example if we stake 1 token with stake RATE, our farmed units or stake will be counted based on 1e24 * RATE / 1e24 = RATE (see ```min_stake``` and ```farmed_tokens``` functions)

#### fn status (user) => Status
View method for seeing your current stats
```rust
pub struct Status {
    /// [ [token_i, token_i,...],... ] where each nft_contract is index
    pub stake_tokens: Vec<TokenIds>,
    /// the min stake based on stake rates and amount of staked tokens
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
```
#### fn withdraw_crop ()
Withdraw harvested rewards before farm ends/close. Don't closed account
Panics when
- Contract not active
- If predecessor didn't have a staked tokens

#### fn unstake (args) => Vec<TokenId>
- Unstake given token_id from nt_contract_id
- If token_id not declared - unstakes all tokens from this contract
- If user doesn't have any staked tokens for all contracts closes account

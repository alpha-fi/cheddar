# XCheddar Token Contract

### Sumary
* Stake CHEDDAR token to lock in the contract and get XCHEDDAR on price P,  
XCHEDDAR_amount = staked_CHEDDAR / P_staked,  
where P_staked = locked_CHEDDAR_token_amount / XCHEDDAR_total_supply.  

* Redeem CHEDDAR by unstake using XCHEDDAR token on price P,  
redeemed_CHEDDAR = unstaked_XCHEDDAR * P_unstaked,  
where P_unstaked = locked_CHEDDAR_token_amount / XCHEDDAR_total_supply. 

* Anyone can add CHEDDAR as reward for those locked CHEDDAR users.  
locked_CHEDDAR_token amount would increase `reward_per_month` after `reward_genesis_time_in_sec`.  

* Owner can modify `reward_genesis_time_in_sec` before it passed.

* Owner can modify `reward_per_month`.

### Compiling

You can build release version by running next scripts inside each contract folder:

```
cd xcheddar
rustup target add wasm32-unknown-unknown
RUSTFLAGS='-C link-arg=-s' cargo build --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/xcheddar_token.wasm xcheddar/res/xcheddar_token.wasm
```

#### Also build cheddar contract which you can find in ./cheddar folder:
```
cd cheddar
rustup target add wasm32-unknown-unknown
RUSTFLAGS='-C link-arg=-s' cargo build --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/cheddar_coin.wasm xcheddar/res/cheddar_coin.wasm
```

### Deploying to TestNet (export $XCHEDDAR_TOKEN id before)

To deploy to TestNet, you can use next command:
```bash
near deploy -f --wasmFile target/wasm32-unknown-unknown/release/xcheddar_token.wasm --accountId $XCHEDDAR_TOKEN
#dev-deploy
near dev-deploy -f --wasmFile target/wasm32-unknown-unknown/release/xcheddar_token.wasm
```

This will output on the contract ID it deployed.

### Contract Metadata
```rust
pub struct ContractMetadata {
    pub version: String,
    pub owner_id: AccountId,
    /// backend locked token id
    pub locked_token: AccountId,
    /// at prev_distribution_time, reward token that haven't distribute yet
    pub undistributed_reward: U128,
    /// at prev_distribution_time, backend staked token amount
    pub locked_token_amount: U128,
    // at call time, the amount of undistributed reward
    pub cur_undistributed_reward: U128,
    // at call time, the amount of backend staked token
    pub cur_locked_token_amount: U128,
    /// XCHEDDAR token supply
    pub supply: U128,
    /// previous reward distribution time in secs
    pub prev_distribution_time_in_sec: u32,
    /// reward start distribution time in secs
    pub reward_genesis_time_in_sec: u32,
    /// reward token amount per 30-day period
    pub reward_per_month: U128,
    /// XCHEDDAR holders account number
    pub account_number: u64,
}
```

### FT Metadata
```rust
FungibleTokenMetadata {
    spec: FT_METADATA_SPEC.to_string(),
    name: String::from("XCheddar Finance Token"),
    symbol: String::from("XCHEDDAR"),
    // see code for the detailed icon content
    icon: Some(String::from("<svg>...")),
    cheddarerence: None,
    cheddarerence_hash: None,
    decimals: 24,
}
```

### Initialize
fill with your accounts for token contract, owner and default user for tests

```shell
export CHEDDAR_TOKEN=token-v3.cheddar.testnet
export XCHEDDAR_TOKEN=
export XCHEDDAR_OWNER=
export USER_ACCOUNT=
export GAS=100000000000000
export HUNDRED_CHEDDAR=100000000000000000000000000
export ONE_CHEDDAR=1000000000000000000000000
export FIVE_CHEDDAR=5000000000000000000000000
export EIGHT_CHEDDAR=8000000000000000000000000

near call $XCHEDDAR_TOKEN new '{"owner_id": "'$XCHEDDAR_OWNER'", "locked_token": "'$CHEDDAR_TOKEN'"}' --account_id=$XCHEDDAR_TOKEN
```
Note: It would set the reward genesis time into 30 days from then on.

### Usage

#### view functions
```bash
# contract metadata gives contract details
near view $XCHEDDAR_TOKEN contract_metadata
# converted timestamps to UTC Datetime and converted from yocto to tokens amounts
near view $XCHEDDAR_TOKEN contract_metadata_human_readable
# get the CHEDDAR / X-CHEDDAR price in 1e8
near view $XCHEDDAR_TOKEN get_virtual_price

# ************* from NEP-141 *************
# see user if registered
near view $XCHEDDAR_TOKEN storage_balance_of '{"account_id": "'$USER_ACCOUNT'"}'
# token metadata
near view $XCHEDDAR_TOKEN ft_metadata
# user token balance
near view $XCHEDDAR_TOKEN ft_balance_of '{"account_id": "'$USER_ACCOUNT'"}'
```

#### register
from NEP-141.
```bash
near view $XCHEDDAR_TOKEN storage_balance_of '{"account_id": "'$USER_ACCOUNT'"}'
near call $XCHEDDAR_TOKEN storage_deposit '{"account_id": "'$USER_ACCOUNT'", "registration_only": true}' --account_id=$USER_ACCOUNT --amount=0.1
# register XCHEDDAR in CHEDDAR contract
near call $CHEDDAR_TOKEN storage_deposit '' --account_id=$XCHEDDAR_TOKEN --amount=0.1
```

#### stake 100 CHEDDAR to get XCHEDDAR
```bash
near call $CHEDDAR_TOKEN ft_transfer_call '{"receiver_id": "'$XCHEDDAR_TOKEN'", "amount": "'$HUNDRED_CHEDDAR'", "msg": ""}' --account_id=$USER_ACCOUNT --depositYocto=1 --gas=$GAS
```

#### add 100 CHEDDAR as a reward
```bash
near call $CHEDDAR_TOKEN ft_transfer_call '{"receiver_id": "'$XCHEDDAR_TOKEN'", "amount": "'$HUNDRED_CHEDDAR'", "msg": "reward"}' --account_id=$USER_ACCOUNT --depositYocto=1 --gas=$GAS
```

#### owner reset reward genesis time
```bash
near call $XCHEDDAR_TOKEN get_owner '' --account_id=$XCHEDDAR_OWNER 
# set to 2022-06-06 00:00:00 GMT time
near call $XCHEDDAR_TOKEN reset_reward_genesis_time_in_sec '{"reward_genesis_time_in_sec": 1654438300}' --account_id=$XCHEDDAR_OWNER
```
Note: would return false if already past old genesis time or the new genesis time is a past time.

#### owner modify reward_per_month to 5 CHEDDAR
```bash
near call $XCHEDDAR_TOKEN modify_monthly_reward '{"monthly_reward": "'$FIVE_CHEDDAR'", "distribute_before_change": true}' --account_id=$XCHEDDAR_OWNER --gas=$GAS
```
Note: If `distribute_before_change` is true, contract will sync up reward distribution using the old `reward_per_month` at call time before changing to the new one.

#### unstake 8 XCHEDDAR get CHEDDAR and reward back
```bash
near call $XCHEDDAR_TOKEN unstake '{"amount": "'$EIGHT_CHEDDAR'"}' --account_id=$USER_ACCOUNT --depositYocto=1 --gas=$GAS
```
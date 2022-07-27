# P3 NFT Token Farm with Many Staked and Many Farmed token types.

The P3-fixed farm allows to stake NFT tokens and farm FT. Constraints:

- The total supply of farmed tokens is fixed = `total_harvested`. This is computed by `reward_rate * number_rounds`.
- Cheddar/FT is farmed per round. During each round we farm `total_ft/number_rounds`.
- Each user, in each round will farm proportionally to the amount of NFT tokens (s)he staked.

The contract rewards algorithm is based on the ["Scalable Reward Distribution on the Ethereum
Blockchain"](https://uploads-ssl.webflow.com/5ad71ffeb79acc67c8bcdaba/5ad8d1193a40977462982470_scalable-reward-distribution-paper.pdf) algorithm.


near dev-deploy -f --wasmFile /Users/macbookpro/Documents/GitHub/cheddar/target/wasm32-unknown-unknown/release/p3_farm_nft.wasm

## Parameters

- Round duration: 1 minute

## Setup

1. Deploy contract and init
2. Register farm in token contract before. Then deposit required NEP-141 tokens (`farm_tokens`)
3. Activate by calling `finalize_setup()`. Must be done at least 12h before opening the farm.

## User Flow

Let's define a common variables:

```sh
# address of the farm
FARM=cheddy-nft.cheddar.testnet

# reward token address
CHEDDAR=token-v3.cheddar.testnet
GUA=guacharo.testnet
# the nft contract address(could be more then one) & token_ids we stake
STAKEING_NFT_CONTRACT_ONE=dev-1648729969586-65831239603610
CHEDDAR_RATE=5000000000000000000000000
TOKEN_ID_ONE=86
TOKEN_ID_TWO=
# cheddy
CHEDDY_NFT_CONTRACT=cheddy.testnet
# owner
OWNER=
# user
USER=rmlsnk.testnet
```

1. Register in the farm:

   ```bash
   #REGISTER FARM INTO FARM TOKENS
   near call $CHEDDAR storage_deposit '{}' --accountId $FARM --deposit 0.00125 
   #...AND AS USER INTO FARM
   near call $FARM storage_deposit '{}' --accountId $USER --amount 0.06
   ```

2. Stake tokens:

   ```bash
   # Add required Cheddar to be able to stake NFT
   near call $CHEDDAR ft_transfer_call '{"receiver_id": "'$FARM'", "amount":"'$CHEDDAR_RATE'", "msg": "cheddar stake"}' --accountId $USER --depositYocto 1 --gas=200000000000000

   # stake
   near call $STAKEING_NFT_CONTRACT_ONE nft_transfer_call '{"receiver_id": "'$FARM'", "token_id":"'$TOKEN_ID_ONE'", "msg": "to farm"}' --accountId $USER --depositYocto 1 --gas=200000000000000
   ```
   - Add your cheddy boost!
   ```bash
   near call $CHEDDY_NFT_CONTRACT nft_transfer_call '{"receiver_id": "'$FARM'", "token_id":"1", "msg": "cheddy"}' --accountId $USER --depositYocto 1 --gas=200000000000000
   ```

3. Enjoy farming, stake more, and observe your status:

   ```bash
   near view $FARM status '{"account_id": "'$USER'"}'
   ```

4. Harvest rewards (if you like to get your CHEDDAR before the farm closes):

   ```bash
   near call $FARM withdraw_crop '' --accountId $USER
   ```

5. Harvest all rewards and close the account (un-register) after the farm will close:
   ```bash
   near call $FARM close '' --accountId $USER --depositYocto 1 --gas=200000000000000
   ```
   Or u can unstake all (from declared nft contract) - it automatically close account if it was last staked contract
   ```bash
   near call $FARM unstake '{"nft_contract_id":"'$STAKEING_NFT_CONTRACT_ONE'"}' --accountId $USER --depositYocto 1 --gas=200000000000000
   ```

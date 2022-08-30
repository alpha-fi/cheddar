# P4 NFT Token Farm with Many Staked and Many Farmed token types.

The ["P3"](https://https://github.com/alpha-fi/cheddar/blob/master/p3-farm/README.md) farm allows to stake NFT tokens and farm Fungible Tokens with added staked NFT boost.


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
FARMING_1=token-v3.cheddar.testnet
FARMING_2=guacharo.testnet
# the nft contract address(could be more then one) & token_id(s) we stake
STAKEING_NFT_CONTRACT_ONE=dev-1648729969586-65831239603610
TOKEN_ID_ONE_ONE=86
TOKEN_ID_ONE_TWO=
STAKEING_NFT_CONTRACT_TWO=
TOKEN_ID_TWO_ONE=
TOKEN_ID_TWO_TWO=
# rate for required Cheddar deposit to have ability to stake 1 NFT
CHEDDAR_RATE=5000000000000000000000000
# boost
BOOST_NFT_CONTRACT=nfticket.testnet
TOKEN_ID_BOOST=
CHEDDY=nft.cheddar.testnet
CHEDDY_TOKEN_ID=
# owner
OWNER=
# user
USER_ID=me.testnet
```

1. Register in the farm:

   ```bash
   #REGISTER FARM INTO FARM TOKENS
   near call $CHEDDAR storage_deposit '{}' --accountId $FARM --deposit 0.00125 
   near call $SECOND_FARMED storage_deposit '{}' --accountId $FARM --deposit 0.00125
   #SETUP ([amount1, amount2] from finalize_setup_expected())
   near view $FARM finalize_setup_expected ''
   near call $CHEDDAR ft_transfer_call '{"receiver_id": "'$FARM'", "amount":"amount1", "msg": "setup reward deposit"}' --accountId $USER_ID --depositYocto 1 --gas=200000000000000

   near call $SECOND_FARMED ft_transfer_call '{"receiver_id": "'$FARM'", "amount":"amount2", "msg": "setup reward deposit"}' --accountId $USER_ID --depositYocto 1 --gas=200000000000000
   near call $FARM finalize_setup '' --accountId $FARM
   ```

2. Stake tokens:

   ```bash
   # REGISTER AS USER INTO FARM
   near call $FARM storage_deposit '{}' --accountId $USER_ID --amount 0.06
   # Add required Cheddar to be able to stake NFT
   near call $CHEDDAR ft_transfer_call '{"receiver_id": "'$FARM'", "amount":"'$CHEDDAR_RATE'", "msg": "cheddar stake"}' --accountId $USER_ID --depositYocto 1 --gas=200000000000000

   # stake
   near call $STAKEING_NFT_CONTRACT_ONE nft_transfer_call '{"receiver_id": "'$FARM'", "token_id":"'$TOKEN_ID_ONE_ONE'", "msg": "to farm"}' --accountId $USER_ID --depositYocto 1 --gas=200000000000000
   ```
   - Add your (cheddy) boost! (you can have only one boost per time)
   ```bash
   near call $BOOST_NFT_CONTRACT nft_transfer_call '{"receiver_id": "'$FARM'", "token_id":"'$TOKEN_ID_BOOST'", "msg": "to boost"}' --accountId $USER_ID --depositYocto 1 --gas=200000000000000
   near call $FARM withdraw_boost_nft '' --accountId $USER_ID
   near call $CHEDDY nft_transfer_call '{"receiver_id": "'$FARM'", "token_id":"'$CHEDDY_TOKEN_ID'", "msg": "to boost"}' --accountId $USER_ID --depositYocto 1 --gas=200000000000000
   ```

3. Enjoy farming, stake more, and observe your status:

   ```bash
   near view $FARM status '{"account_id": "'$USER_ID'"}'
   ```

4. Harvest rewards (if you like to get your CHEDDAR before the farm closes):

   ```bash
   near call $FARM withdraw_crop '' --accountId $USER_ID --gas=300000000000000
   ```

5. Harvest all rewards and close the account (un-register) after the farm will close:
   ```bash
   near call $FARM close '' --accountId $USER_ID --depositYocto 1 --gas=300000000000000
   ```
   Or u can unstake it automatically close account if it was last staked token
   ```bash
   near call $FARM unstake '{"nft_contract_id":"'$STAKEING_NFT_CONTRACT_ONE'", "token_id":"'$TOKEN_ID_ONE_ONE'"}' --accountId $USER_ID --depositYocto 1 --gas=300000000000000
   ```
```sh

```

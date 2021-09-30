# Token Farm with Fixed Supply

The P2-fixed farm allows to stake tokens and farm Cheddar. Constraints:
* The total supply of Cheddar is fixed = `total_cheddar`. This is computed by `reward_rate * number_rounds`.
* Cheddar is farmed per round. During each round we farm `total_cheddar/number_rounds`.
* Each user, in each round will farm proportionally to the amount of tokens (s)he staked.

The contract rewards algorithm is based on the ["Scalable Reward Distribution on the Ethereum
Blockchain"](https://uploads-ssl.webflow.com/5ad71ffeb79acc67c8bcdaba/5ad8d1193a40977462982470_scalable-reward-distribution-paper.pdf) algorithm.

## Parameters

* Round duration: 1 minute

## Flow

Let's define a common variables:
```sh
# address of the farm
FARM=p2-farm.cheddar.testnet
# reward token address
CHEDDAR=token.cheddar.testnet
# the token address we stake
STAKEING_TOKEN=abc.testnet
```

1. Register to the farm:
   ```
   near call $FARM storage_deposit '{}' --accountId me.testnet --deposit 0.05
   ```

2. Stake tokens:
   ```
   near call $STAKEING_TOKEN ft_transfer_call '{"receiver_id": "p2-farm.cheddar.testnet", "amount":"10", "msg": "to farm"}' --accountId me.testnet --depositYocto 1 --gas=200000000000000
   ```

3. Enjoy farming, stake more, and observe your status:
   ```
   near view $FARM status '{"account_id": "me.testnet"}'
   ```

4. Harvest rewards (if you like to get your CHEDDAR before the farm closes):
    ```
    near call $FARM withdraw_crop '' --accountId me.testnet --depositYocto 1
    ```

5. Harvest all rewards and close the account (un-register) after the farm will close:
    ```
    near call FARM close '' --accountId me.testnet --depositYocto 1 --gas=200000000000000
    ```

set -e
NETWORK=testnet
OWNER=cheddar.$NETWORK
MASTER_ACC=$OWNER
CONTRACT_ACC=p1.$MASTER_ACC

WASM=./res/p1_staking_pool_dyn.wasm
ls -l $WASM

export NODE_ENV=$NETWORK

E6="000000"
E12=$E6$E6
YOCTO=$E12$E12

YOCTO_CHEDDAR_PER_SECOND_PER_NEAR=578703703703703700
FARMING_START=1624158000
FARMING_END=1625886000

## delete acc
echo "Delete $CONTRACT_ACC? are you sure? Ctrl-C to cancel"
# read input
# near delete $CONTRACT_ACC $MASTER_ACC
# near create-account $CONTRACT_ACC --masterAccount $MASTER_ACC
# near deploy $CONTRACT_ACC $WASM new "{\"owner_id\":\"$OWNER\", \"cheddar_id\":\"token.$MASTER_ACC\",\"reward_rate\":\"$YOCTO_CHEDDAR_PER_SECOND_PER_NEAR\", \"farming_start\":$FARMING_START, \"farming_end\":$FARMING_END}" --accountId $MASTER_ACC

##redeploy only
near deploy $CONTRACT_ACC $WASM  --accountId $MASTER_ACC
#near call $CONTRACT_ACC set_rewards_per_year {\"new_value\":12500} --accountId $MASTER_ACC
#near call token.$MASTER_ACC mint "{\"account_id\":\"$CONTRACT_ACC\",\"amount\":\"100$E6$YOCTO\"}" --amount 0.000000000000000000000001 --accountId $MASTER_ACC
#near call $CONTRACT_ACC open {\"value\":true} --accountId $MASTER_ACC

#save last deployment  (to be able to recover state/tokens)
#cp ./res/nep_141_model.wasm ./res/nep_141_model.`date +%F.%T`.wasm
#date +%F.%T

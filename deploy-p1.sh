set -e
NETWORK=testnet
OWNER=cheddar.$NETWORK
MASTER_ACC=$OWNER
CONTRACT_ACC=p1.$MASTER_ACC

WASM=./res/p1_staking_pool.wasm
ls -l $WASM

export NODE_ENV=$NETWORK

## delete acc
#echo "Delete $CONTRACT_ACC? are you sure? Ctrl-C to cancel"
#read input
#near delete $CONTRACT_ACC $MASTER_ACC
#near create-account $CONTRACT_ACC --masterAccount $MASTER_ACC
#near deploy $CONTRACT_ACC $WASM new "{\"owner_id\":\"$OWNER\", \"cheddar_id\":\"token.$MASTER_ACC\",\"rewards_per_year\":12500}" --accountId $MASTER_ACC

##redeploy only
#near deploy $CONTRACT_ACC $WASM  --accountId $MASTER_ACC
E6="000000"
E12=$E6$E6
YOCTO=$E12$E12
echo $YOCTO
#near call $CONTRACT_ACC set_rewards_per_year {\"new_value\":12500} --accountId $MASTER_ACC
#near call token.$MASTER_ACC mint "{\"account_id\":\"$CONTRACT_ACC\",\"amount\":\"100$E6$YOCTO\"}" --amount 0.000000000000000000000001 --accountId $MASTER_ACC
#near call $CONTRACT_ACC open {\"value\":true} --accountId $MASTER_ACC


#save last deployment  (to be able to recover state/tokens)
#cp ./res/nep_141_model.wasm ./res/nep_141_model.`date +%F.%T`.wasm
#date +%F.%T

set -e
NETWORK=testnet
OWNER=cheddar.$NETWORK
MASTER_ACC=$OWNER
CONTRACT_ACC=token.$MASTER_ACC

export NODE_ENV=$NETWORK

## delete acc
echo "Delete $CONTRACT_ACC? are you sure? Ctrl-C to cancel"
read input
near delete $CONTRACT_ACC $MASTER_ACC
near create-account $CONTRACT_ACC --masterAccount $MASTER_ACC
near deploy $CONTRACT_ACC ./res/cheddar-token.wasm new "{\"owner_id\":\"$OWNER\"}" --accountId $MASTER_ACC
## set params


## redeploy code only
#near deploy $CONTRACT_ACC ./res/nep_141_model.wasm 

#save last deployment  (to be able to recover state/tokens)
#cp ./res/nep_141_model.wasm ./res/nep_141_model.`date +%F.%T`.wasm
#date +%F.%T

set -e
NETWORK=testnet
OWNER=cheddardao.$NETWORK
MASTER_ACC=$OWNER
CONTRACT_ACC=dao.$MASTER_ACC
TOKEN_ACC=token.cheddar.testnet
TREASURY_ACC=treasury.$MASTER_ACC

COUNCIL_ACC=alantest.$NETWORK

export NODE_ENV=$NETWORK
export POLICY='{
  "roles": [
    {
      "name": "council",
      "kind": { "Group": ["alantest.testnet", "alan1.testnet"]
      },
      "permissions": [
        "*:Finalize",
        "*:AddProposal",
        "*:VoteApprove",
        "*:VoteReject",
        "*:VoteRemove"
      ],
      "vote_policy": {}
    },
    {
      "name": "community",
      "kind": { "Group": ["alan1.testnet"] },
      "permissions": [
        "*:Finalize",
        "*:VoteApprove",
        "*:VoteReject",
        "*:VoteRemove"
      ],
      "vote_policy": {}
    }
  ],
  "default_vote_policy": { "weight_kind": "RoleWeight", "quorum": "0", "threshold": [ 1, 2 ] },
  "proposal_bond": "1000000000000000000000000",
  "proposal_period": "604800000000000",
  "bounty_bond": "1000000000000000000000000",
  "bounty_forgiveness_period": "86400000000000"
}}'

ARGS_MINT=`echo '{"account_id": "treasury.cheddardao.testnet", "amount": "20000000000000000000000000000000"}' | base64`


## delete acc
echo "Delete $CONTRACT_ACC? are you sure? Ctrl-C to cancel"
read input

near delete $CONTRACT_ACC $MASTER_ACC
near create-account $CONTRACT_ACC --masterAccount $MASTER_ACC --initialBalance 20
near deploy --wasmFile=res/sputnikdao2.wasm --initFunction "new" --initArgs "{\"config\": {\"name\": \"testpolicy\", \"purpose\": \"Test DAO Policy\", \"metadata\":\"\"}, \"policy\": $POLICY" --accountId $CONTRACT_ACC
near view $CONTRACT_ACC get_policy
echo "DAO succesfully deployed"

##redeploy only
#near deploy $CONTRACT_ACC --wasmFile=res/sputnikdao2.wasm  --accountId $MASTER_ACC

#save last deployment 
#cp ./res/sputnikdao2.wasm ./res/sputnikdao2.`date +%F.%T`.wasm
#date +%F.%T

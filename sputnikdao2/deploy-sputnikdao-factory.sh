set -e
NETWORK=testnet
OWNER=sputnikv2.$NETWORK

COUNCIL_ACC=alan1.testnet
DAO_NAME=mynewdao2 #mynewdao.sputnikv2.testnet

##Change NODE_ENV between mainnet, testnet and betanet
export NODE_ENV=testnet

#DAO Policy
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

#Args for creating DAO in sputnik-factory2
ARGS=`echo "{\"config\":  {\"name\": \"testpolicy\", \"purpose\": \"Test DAO Policy\", \"metadata\":\"\"}, \"policy\": $POLICY" | base64`
read input
# Call sputnik factory for deploying new dao with custom policy
near call sputnikv2.testnet create "{\"name\": \"$DAO_NAME\", \"args\": \"$ARGS\"}" --accountId $COUNCIL_ACC --amount 5 --gas 150000000000000
near view $DAO_NAME.$OWNER get_policy

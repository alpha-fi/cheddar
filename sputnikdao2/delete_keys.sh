set -e
NETWORK=testnet
OWNER=cheddardao.$NETWORK
MASTER_ACC=$OWNER
CONTRACT_ACC=dao50.$MASTER_ACC
TOKEN_ACC=token.cheddar.testnet
TREASURY_ACC=treasury.$MASTER_ACC

COUNCIL_ACC=alantest.$NETWORK

KEY_TO_DELETE="ed25519:983XUUte5uyhujGwVbyPuvWdimHNatUyXChkpQwbafuo"

export NODE_ENV=$NETWORK

near keys $CONTRACT_ACC
echo "¡IMPORTANT!"
echo "Modify the KEY_TO_DELETE parameter in delete_keys with the public_key that is going to be deleteded. Are you ready? Ctrl-C to cancel"
read input
echo "¡REMEMBER"
echo "Once you delete all access keys the account will not be possible to be deleted or redeployed. Ctrl-C to cancel"
read input
near delete-key $CONTRACT_ACC $KEY_TO_DELETE
echo "Keys succesfully deleted"

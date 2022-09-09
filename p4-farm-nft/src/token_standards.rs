use crate::*;

use near_contract_standards::{
    non_fungible_token::core::NonFungibleTokenReceiver,
    non_fungible_token::TokenId,
    fungible_token::receiver::FungibleTokenReceiver,
};

/// NFT Receiver message switcher.
/// Points to which transfer option is choosed for
enum TransferInstruction {
    ToFarm,
    ToBoost,
    Unknown
}

impl From<String> for TransferInstruction {
    fn from(msg: String) -> Self {
        match &msg[..] {
            "to farm"  => TransferInstruction::ToFarm,
            "to boost" => TransferInstruction::ToBoost,
            _ => TransferInstruction::Unknown
        }
    }
} 

/// NFT Receiver
/// Used when an NFT is transferred using `nft_transfer_call`.
/// This function is considered safe and will work when contract is paused to allow user
/// to accumulate bonuses.
/// Message from transfer switch options:
/// - NFT transfer to Farm
/// - Boost NFT transfer for rewards to Boost
#[allow(unused_variables)]
#[near_bindgen]
impl NonFungibleTokenReceiver for Contract {
    fn nft_on_transfer(
        &mut self,
        sender_id: AccountId,
        previous_owner_id: AccountId,
        token_id: TokenId,
        msg: String,
    ) -> PromiseOrValue<bool> {
        let nft_contract_id:NftContractId = env::predecessor_account_id();
        assert_ne!(
            nft_contract_id, env::signer_account_id(),
            "ERR_NOT_CROSS_CONTRACT_CALL"
        );
        assert_eq!(
            previous_owner_id, env::signer_account_id(),
            "ERR_OWNER_NOT_SIGNER"
        );
        
        match TransferInstruction::from(msg) {
            // "to boost" message for transfer P4 boost
            TransferInstruction::ToBoost => {
                self.assert_is_active();
                self._boost_stake(&previous_owner_id, &nft_contract_id, token_id);
                return PromiseOrValue::Value(true)
            },
            // "to farm" message for transfer NFT into P4 to stake
            TransferInstruction::ToFarm => {
                self.assert_is_active();
                self._nft_stake(&previous_owner_id, &nft_contract_id, token_id);
                return PromiseOrValue::Value(true)
            }
            // unknown message (or no message) - we are refund
            _ => {
                log!("ERR_UNKNOWN_MESSAGE");
                return PromiseOrValue::Value(true)
            }
        }
    }
}

/// FT Receiver
/// token deposits are done through NEP-141 ft_transfer_call to the NEARswap contract.
#[near_bindgen]
impl FungibleTokenReceiver for Contract {
    /**
    FungibleTokenReceiver implementation Callback on receiving tokens by this contract.
    Handles both farm deposits and stake deposits. For farm deposit (sending tokens
    to setup the farm) you must set "setup reward deposit" msg.
    Otherwise tokens will be staken.
    Returns zero.
    Panics when:
    - account is not registered
    - or receiving a wrong token
    - or making a farm deposit after farm is finalized
    - or staking before farm is finalized. */
    #[allow(unused_variables)]
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        let ft_token_id = env::predecessor_account_id();

        assert!(amount.0 > 0, "deposited amount must be positive");
        // deposit rewards
        if msg == "setup reward deposit" {
            self._setup_deposit(&ft_token_id, amount.0);
        } else {
            // cheddar staking
            if msg == "cheddar stake" {
                self.stake_cheddar(&sender_id, amount.0);
            } else {
                log!(
                    "Contract accept only NFT farming and staking! 
                     If you need to deposit Cheddar to be able for stake NFT, use 'cheddar stake' as msg.
                     Refund transfer from @{} with token {} amount {}",
                    sender_id,
                    ft_token_id,
                    amount.0
                );
                return PromiseOrValue::Value(amount)
            }
        }

        return PromiseOrValue::Value(U128(0))
    }
}

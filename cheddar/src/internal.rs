use near_sdk::json_types::{ValidAccountId, U128};
use near_sdk::{AccountId, Balance, PromiseResult};

use crate::*;

impl Contract {
    pub(crate) fn assert_owner_calling(&self) {
        assert!(
            env::predecessor_account_id() == self.owner_id,
            "can only be called by the owner"
        );
    }

    pub(crate) fn assert_minter(&self, account_id: String) {
        assert!(self.minters.contains(&account_id), "not a minter");
    }

    /// get stored metadata or default
    #[inline]
    pub(crate) fn internal_get_ft_metadata(&self) -> FungibleTokenMetadata {
        self.metadata.get().unwrap()
    }

    #[inline]
    pub(crate) fn internal_unwrap_balance_of(&self, account_id: &AccountId) -> Balance {
        self.accounts.get(&account_id).unwrap_or(0)
    }

    pub(crate) fn mint_into(&mut self, account_id: &AccountId, amount: Balance) {
        let balance = self.internal_unwrap_balance_of(account_id);
        self.internal_update_account(&account_id, balance + amount);
        self.total_supply += amount;
    }

    pub(crate) fn internal_burn(&mut self, account_id: &AccountId, amount: u128) {
        let balance = self.internal_unwrap_balance_of(account_id);
        assert!(balance >= amount);
        self.internal_update_account(&account_id, balance - amount);
        assert!(self.total_supply >= amount);
        self.total_supply -= amount;
    }

    pub(crate) fn internal_transfer(
        &mut self,
        sender_id: &AccountId,
        receiver_id: &AccountId,
        amount: Balance,
        memo: Option<String>,
    ) {
        assert_ne!(
            sender_id, receiver_id,
            "Sender and receiver should be different"
        );
        assert!(amount > 0, "The amount should be a positive number");

        // remove from sender
        let sender_balance = self.internal_unwrap_balance_of(sender_id);
        assert!(
            amount <= sender_balance,
            "The account doesn't have enough balance {}",
            sender_balance
        );
        let balance_left = sender_balance - amount;
        self.internal_update_account(&sender_id, balance_left);

        // check vesting
        match self.vested.get(&sender_id) {
            Some(vesting) => {
                //compute locked
                let locked = vesting.compute_amount_locked();
                if locked == 0 {
                    //vesting is complete. remove vesting lock
                    self.vested.remove(&sender_id);
                } else if balance_left < locked {
                    panic!("Vested account, balance can't go lower than {}", locked);
                }
            }
            None => {}
        }

        // add to receiver
        let receiver_balance = self.internal_unwrap_balance_of(receiver_id);
        self.internal_update_account(&receiver_id, receiver_balance + amount);

        log!("Transfer {} from {} to {}", amount, sender_id, receiver_id);
        if let Some(memo) = memo {
            log!("Memo: {}", memo);
        }
    }

    /// Inner method to save the given account for a given account ID.
    /// If the account balance is 0, the account is deleted instead to release storage.
    pub(crate) fn internal_update_account(&mut self, account_id: &AccountId, balance: u128) {
        if balance == 0 {
            self.accounts.remove(account_id);
        } else {
            self.accounts.insert(account_id, &balance); //insert_or_update
        }
    }

    /// Helper method to update balance of the sender and receiver based on the return
    /// from the `on_ft_transfer` call.
    /// Relper function which computes the amount refunded from the transfer_call and adjust
    /// sender and receiver balances.
    /// Returns: `(amount_credited_by_reciever, amount_burned)`, where
    /// * amount_credited_by_receiver - is the amount transferred to the receiver after
    ///   adjusting the balances
    /// * amount_burned - when sender account is deleted we burn the unused tokens.
    pub(crate) fn ft_resolve_transfer_adjust(
        &mut self,
        sender_id: &AccountId,
        receiver_id: ValidAccountId,
        amount: U128,
    ) -> (u128, u128) {
        let receiver_id: AccountId = receiver_id.into();
        let amount: Balance = amount.into();

        // Get the unused amount from the `ft_on_transfer` call result.
        let mut unused_amount = match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Successful(value) => {
                if let Ok(unused_amount) = near_sdk::serde_json::from_slice::<U128>(&value) {
                    std::cmp::min(amount, unused_amount.0)
                } else {
                    amount
                }
            }
            PromiseResult::Failed => amount,
        };

        if unused_amount > 0 {
            let receiver_balance = self.accounts.get(&receiver_id).unwrap_or(0);
            // receiver has positive balance, so we can refund.
            if receiver_balance > 0 {
                // adjust the refund amount to the receiver balance
                unused_amount = std::cmp::min(receiver_balance, unused_amount);
                self.accounts
                    .insert(&receiver_id, &(receiver_balance - unused_amount));

                // now we will try to update sender balance
                if let Some(sender_balance) = self.accounts.get(sender_id) {
                    self.accounts
                        .insert(sender_id, &(sender_balance + unused_amount));
                    log!(
                        "Refund {} from {} to {}",
                        unused_amount,
                        receiver_id,
                        sender_id
                    );
                    return (amount - unused_amount, 0);
                } else {
                    // Sender's account was deleted, so we need to burn tokens.
                    self.total_supply -= unused_amount;
                    log!(
                        "The sender account is deleted, can't make a refund. Burning {} from ft_transfer_call",
                        unused_amount
                    );
                    return (amount - unused_amount, unused_amount);
                }
            } else {
                log!("Reciever {} didn't use all tokens, but it's balance is 0 so can't refund {} tokens to the sender",
                     &receiver_id, unused_amount);
            }
        }
        (amount, 0)
    }
}

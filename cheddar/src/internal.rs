use near_sdk::json_types::{ValidAccountId, U128};
use near_sdk::{AccountId, Balance, PromiseResult};

use crate::storage::AccBalance;
use crate::*;

impl Contract {
    pub(crate) fn assert_owner(&self) {
        assert!(
            env::predecessor_account_id() == self.owner_id,
            "can only be called by the owner"
        );
    }

    #[inline]
    pub(crate) fn assert_minter(&self, account_id: String) {
        assert!(self.minters.contains(&account_id), "not a minter");
    }

    /// get stored metadata or default
    #[inline]
    pub(crate) fn internal_get_ft_metadata(&self) -> FungibleTokenMetadata {
        self.metadata.get().unwrap()
    }

    /// returns token balance
    #[inline]
    pub(crate) fn _balance_of(&self, account_id: &AccountId) -> Balance {
        match self.accounts.get(&account_id) {
            Some(a) => a.token,
            _ => 0,
        }
    }

    /// returns token balance, panics if an account is not registered
    #[inline]
    pub(crate) fn _must_balance_of(&self, account_id: &AccountId) -> AccBalance {
        self.accounts
            .get(&account_id)
            .expect(format!("Account {} is not registered", account_id).as_str())
    }

    pub(crate) fn mint(&mut self, account_id: &AccountId, amount: Balance) {
        let mut ab = self.try_register_account(account_id, 0);
        ab.token += amount;
        self.accounts.insert(account_id, &ab);
        self.total_supply += amount;
    }

    pub(crate) fn internal_burn(&mut self, account_id: &AccountId, amount: u128) {
        assert!(amount > 0, "can't burn 0 tokens");
        let mut ab = self._must_balance_of(account_id);
        ab.token -= amount;
        self.accounts.insert(account_id, &ab);
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
        let mut sender_balance = self._must_balance_of(sender_id);
        assert!(
            amount <= sender_balance.token,
            "The account doesn't have enough balance {}",
            sender_balance.token
        );
        sender_balance.token -= amount;
        self.accounts.insert(sender_id, &sender_balance);

        // check vesting
        match self.vested.get(&sender_id) {
            Some(vesting) => {
                //compute locked
                let locked = vesting.compute_amount_locked();
                if locked == 0 {
                    //vesting is complete. remove vesting lock
                    self.vested.remove(&sender_id);
                } else {
                    assert!(
                        sender_balance.token >= locked,
                        "Account with vesting, balance can't go lower than {}",
                        locked
                    );
                }
            }
            None => {}
        }

        // add to receiver
        let mut receiver_balance = self._must_balance_of(receiver_id);
        receiver_balance.token += amount;
        self.accounts.insert(receiver_id, &receiver_balance);

        log!(
            "Transfer {} from {} to {}, memo: {}",
            amount,
            sender_id,
            receiver_id,
            memo.unwrap_or_default()
        );
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
            let mut receiver_balance = self._must_balance_of(&receiver_id);
            // receiver has positive balance, so we can refund.
            if receiver_balance.token > 0 {
                // adjust the refund amount to the receiver balance
                unused_amount = std::cmp::min(receiver_balance.token, unused_amount);
                receiver_balance.token -= unused_amount;
                self.accounts.insert(&receiver_id, &receiver_balance);

                // now we will try to update sender balance
                if let Some(mut sender_balance) = self.accounts.get(sender_id) {
                    sender_balance.token += unused_amount;
                    self.accounts.insert(sender_id, &sender_balance);
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

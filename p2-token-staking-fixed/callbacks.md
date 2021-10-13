# Callback sequences

#### fn return_tokens => fn return_tokens_callback

        self.return_tokens(a.clone(), amount)
            .then(ext_self::return_tokens_callback(
                a,
                amount,
                &env::current_account_id(),
                0,
                GAS_FOR_MINT_CALLBACK,
            ))


   //
   // schedules an async call to ft_transfer to return staked-tokens to the user
   //
   fn return_tokens(&self, user: AccountId, amount: U128) -> Promise {
        return ext_ft::ft_transfer(
            user,
            amount,
            Some("unstaking".to_string()),
            &self.staking_token,
            1,
            GAS_FOR_FT_TRANSFER,
        );
   }

   // callback for return_tokens
   //
   pub fn return_tokens_callback(&mut self, user: AccountId, amount: U128) {
   
      verifies ft_transfer result
      in case of failure, restore token amount to user vault


#### fn mint_cheddar - does harvest and maybe close account

   Note: callers of fn mint_cheddar MUST set rewards to zero in the vault, because in case of failure the callbacks will re-add rewards to the vault
   Recommendation 1: change the arguments to u128 instead of U128: `fn mint_cheddar(&mut self, a: &AccountId, cheddar_amount: u128, tokens: u128)`

   fn mint_cheddar(&mut self, a: &AccountId, cheddar: U128, tokens: U128) -> Promise {

   --pseudocode
   
   if there's cheddar to mint:

         schedule a call to ext_ft::mint on the cheddar contract for the user

         if there are tokens to return
            schedule a parallel call to self.return_tokens

         else // no tokens to return
            schedule a then callback to self.mint_callback


   else if no cheddars to mint but tokens to return

         schedule a call to return_tokens with a then-callback to return_tokens_callback


   else // no cheddars to mint and no tokens to return

         do nothing
         exit

   end if

   finally, to the previously scheduled async calls, 
   add a .then callback to fn mint_callback_finally

##### fn mint_callback

      verifies cheddar mint result
      in case of failure, restore cheddar-rewards amount to user vault

##### fn mint_callback_finally

      verifies rewards were really minted for the user
      and if not, panics, so the user receives the correct error message.
      Why panic?: Given that ft_mint is an async call 
      if the call fails, the rewards are correctly restored
      but the user (front-end) do not receive an error message because 
      the main call was successful. We need to add the panic! so the user
      is informed that the harvesting failed

#### Problems:
* if cheddar_to_mint!=0 && tokens_to_return!=0, 
  * the callback `self.mint_callback` is not being scheduled
  * the callback `return_tokens_callback` is not being scheduled

Note: this problem only presents itself in unhappy paths (failure in the execution of the async calls), 
because the rollback callback is not being scheduled.

#### Solutions to test
* Solution A) Check if we can create two promises, each one with its own .then callback, and at the end combine both with and
* Solution B) Try chaining all promises as .then(), so first cheddar is harvested then the tokens returned
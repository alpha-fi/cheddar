# Callback sequences / rollbacks check

#### fn return_tokens => fn return_tokens_callback (OK)
```
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
```



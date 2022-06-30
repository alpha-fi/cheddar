use crate::*;
#[allow(unused_imports)]
use crate::utils::convert_from_yocto_cheddar;
use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_sdk::json_types::U128;
use near_sdk::{assert_one_yocto, env, log, Promise, PromiseResult};
use std::cmp::{max, min};

enum TransferInstruction {
    Deposit,
    Reward,
    Unknown
}

impl From<String> for TransferInstruction {
    fn from(msg: String) -> Self {
        match &msg[..] {
            "" => TransferInstruction::Deposit,
            "reward" => TransferInstruction::Reward,
            _  => TransferInstruction::Unknown
        }
    }
}

impl Contract {
    pub(crate) fn internal_stake(&mut self, account_id: &AccountId, amount: Balance) {
        // check account has registered
        assert!(self.ft.accounts.contains_key(account_id), "Account @{} is not registered", account_id);
        // amount of Xcheddar that user takes from stake cheddar
        let mut minted = amount;

        if self.ft.total_supply != 0 {
            assert!(self.locked_token_amount > 0, "{}", ERR_INTERNAL);
            minted = (U256::from(amount) * U256::from(self.ft.total_supply) / U256::from(self.locked_token_amount)).as_u128();
        }
        
        assert!(minted > 0, "{}", ERR_STAKE_TOO_SMALL);

        // increase locked_token_amount to staked
        self.locked_token_amount += amount;
        // increase total_supply to minted = staked * P, where P = total_supply/locked_token_amount <= 1
        self.ft.internal_deposit(account_id, minted);
        log!("@{} Stake {} (~{} CHEDDAR) assets, get {} (~{} xCHEDDAR) tokens",
            account_id, 
            amount,
            convert_from_yocto_cheddar(amount),
            minted,
            convert_from_yocto_cheddar(minted)
        );
        // total_supply += amount * P, where P<=1
        // locked_token_amount += amount
    }

    pub(crate) fn internal_add_reward(&mut self, account_id: &AccountId, amount: Balance) {
        self.undistributed_reward += amount;
        log!("@{} add {} (~{} CHEDDAR) assets as reward", account_id, amount, convert_from_yocto_cheddar(amount));
    }

    // return the amount of to be distribute reward this time
    pub(crate) fn try_distribute_reward(&self, cur_timestamp_in_sec: u32) -> Balance {
        if cur_timestamp_in_sec > self.reward_genesis_time_in_sec && cur_timestamp_in_sec > self.prev_distribution_time_in_sec {
            //reward * (duration between previous distribution and current time)
            let ideal_amount = self.reward_per_second * (cur_timestamp_in_sec - self.prev_distribution_time_in_sec) as u128;
            min(ideal_amount, self.undistributed_reward)
        } else {
            0
        }
    }

    pub(crate) fn distribute_reward(&mut self) {
        let cur_time = nano_to_sec(env::block_timestamp());
        let new_reward = self.try_distribute_reward(cur_time);
        if new_reward > 0 {
            self.undistributed_reward -= new_reward;
            self.locked_token_amount += new_reward;
            self.prev_distribution_time_in_sec = max(cur_time, self.reward_genesis_time_in_sec);
            log!("Distribution reward is {} ", new_reward);
        } else {
            log!("Distribution reward is zero for this time");
        }
    }
}

#[near_bindgen]
impl Contract {
    /// unstake token and send assets back to the predecessor account.
    /// Requirements:
    /// * The predecessor account should be registered.
    /// * `amount` must be a positive integer.
    /// * The predecessor account should have at least the `amount` of tokens.
    /// * Requires attached deposit of exactly 1 yoctoNEAR.
    /// ? : withdraw on every time or it opens in windows?
    #[payable]
    pub fn unstake(&mut self, amount: U128) -> Promise {
        // Checkpoint
        self.distribute_reward();

        assert_one_yocto();

        let account_id = env::predecessor_account_id();
        let amount: Balance = amount.into();

        assert!(self.ft.total_supply > 0, "{}", ERR_EMPTY_TOTAL_SUPPLY);
        let unlocked = (U256::from(amount) * U256::from(self.locked_token_amount) / U256::from(self.ft.total_supply)).as_u128();

        // total_supply -= amount
        self.ft.internal_withdraw(&account_id, amount);
        assert!(self.ft.total_supply >= 10u128.pow(24), "{}", ERR_KEEP_AT_LEAST_ONE_XCHEDDAR);
        // locked_token_amount -= amount * P, where P = locked_token_amount / total_supply >=1
        self.locked_token_amount -= unlocked;

        log!("Withdraw {} (~{} Cheddar) from @{}", amount, convert_from_yocto_cheddar(amount), account_id);

        // ext_fungible_token was deprecated at v4.0.0 near_sdk release
        /*
        ext_fungible_token::ft_transfer(
            account_id.clone(),
            U128(unlocked),
            None,
            self.locked_token.clone(),
            1,
            GAS_FOR_FT_TRANSFER,
        )
        .then(ext_self::callback_post_unstake(
            account_id.clone(),
            U128(unlocked),
            U128(amount),
            env::current_account_id(),
            NO_DEPOSIT,
            GAS_FOR_RESOLVE_TRANSFER,
        ))
         */

        ext_cheddar::ext(self.locked_token.clone())
        .with_attached_deposit(ONE_YOCTO)
        .with_static_gas(GAS_FOR_FT_TRANSFER)
        .ft_transfer(account_id.clone(), U128(unlocked), None)   
        .then(
            Self::ext(env::current_account_id())
                .with_static_gas(GAS_FOR_RESOLVE_TRANSFER)
                .callback_post_unstake(account_id.clone(), U128(unlocked), U128(amount))
        )
    }

    #[private]
    pub fn callback_post_unstake(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        share: U128,
    ) {
        assert_eq!(
            env::promise_results_count(),
            1,
            "{}", ERR_PROMISE_RESULT
        );
        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Successful(_) => {
                log!(
                        "Account @{} successful unstake {} (~{} CHEDDAR).",
                        sender_id,
                        amount.0,
                        convert_from_yocto_cheddar(amount.0)
                    );
            }
            PromiseResult::Failed => {
                // This reverts the changes from unstake function.
                // If account doesn't exit, the unlock token stay in contract.
                if self.ft.accounts.contains_key(&sender_id) {
                    self.locked_token_amount += amount.0;
                    self.ft.internal_deposit(&sender_id, share.0);
                    log!(
                            "Account @{} unstake failed and reverted.",
                            sender_id
                        );
                } else {
                    log!(
                            "Account @{} has unregistered. Unlocking token goes to contract.",
                            sender_id
                        );
                }
            }
        };
    }
}

#[near_bindgen]
impl FungibleTokenReceiver for Contract {
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        // Checkpoint
        self.distribute_reward();
        let token_in = env::predecessor_account_id();
        let amount: Balance = amount.into();
        assert_eq!(token_in, self.locked_token, "{}", ERR_MISMATCH_TOKEN);
        match TransferInstruction::from(msg) {
            TransferInstruction::Deposit => {
                // deposit for stake
                self.internal_stake(&sender_id, amount);
                PromiseOrValue::Value(U128(0))
            } 
            TransferInstruction::Reward => {
                // deposit for reward
                self.internal_add_reward(&sender_id, amount);
                PromiseOrValue::Value(U128(0))
            }
            TransferInstruction::Unknown => {
                log!(ERR_WRONG_TRANSFER_MSG);
                PromiseOrValue::Value(U128(amount))
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    const E24:u128 = 1_000_000_000_000_000_000_000_000;

    fn proportion(a:u128, numerator:u128, denominator:u128) -> u128 {
        (U256::from(a) * U256::from(numerator) / U256::from(denominator)).as_u128()
    }
    fn compute_p(total_locked: u128, total_supply:u128, staked:bool) -> u128 {
        if staked == true {
            total_supply * 100_000_000 / total_locked
        } else {
            total_locked * 100_000_000 / total_supply
        }
    }
    #[test]
    fn test_P_value() {

        let mut total_reward:u128 = 50_000 * E24;
        let mut total_locked:u128 = 52_500 * E24;
        let mut total_supply:u128 = 50_000 * E24;
        let reward_per_second:u128 = 10000000000000000000000; //0.01

        let p_unstaked = compute_p(total_locked, total_supply, false); // 1.05
        let p_staked = compute_p(total_locked, total_supply, true); // 1/1.05

        // stake 100
        let amount:u128 = 100 * E24; //100
        let minted = proportion(amount, total_supply, total_locked);
        total_locked += amount;
        total_supply += minted;
        assert_eq!(p_staked, compute_p(total_locked, total_supply, true));
        assert_eq!(p_unstaked, compute_p(total_locked, total_supply, false));
        println!(
            " P_staked: {}\n P_unstaked: {}\n P-deviation: {}\n locked: {}\n supply: {}",
            compute_p(total_locked, total_supply, true),
            compute_p(total_locked, total_supply, false),
            convert_from_yocto_cheddar((p_staked - compute_p(total_locked, total_supply, true))),
            total_locked,
            total_supply
        );

        // stake 1000
        let amount:u128 = 1000 * E24; //1000
        let minted = proportion(amount, total_supply, total_locked);
        total_locked += amount;
        total_supply += minted;
        assert_eq!(p_staked, compute_p(total_locked, total_supply, true));
        assert_eq!(p_unstaked, compute_p(total_locked, total_supply, false));
        println!(
            " P_staked: {}\n P_unstaked: {}\n P-deviation: {}\n locked: {}\n supply: {}",
            compute_p(total_locked, total_supply, true),
            compute_p(total_locked, total_supply, false),
            convert_from_yocto_cheddar((p_staked - compute_p(total_locked, total_supply, true))),
            total_locked,
            total_supply
        );
        
        // unstake 10000 after 1000 seconds
        // distribution
        total_locked += reward_per_second * 1000;
        let amount:u128 = 10000 * E24; //10000
        // unstaking
        let unlocked = proportion(amount, total_locked, total_supply);
        total_locked -= unlocked;
        total_supply -= amount;
        println!(
            " P_staked: {}\n P_unstaked: {}\n P-deviation: {}\n locked: {}\n supply: {}",
            compute_p(total_locked, total_supply, true),
            compute_p(total_locked, total_supply, false),
            convert_from_yocto_cheddar((p_staked - compute_p(total_locked, total_supply, true))),
            total_locked,
            total_supply
        );

        // unstake all and keep 1 token in supply after 1000 seconds
        // distribution
        total_locked += reward_per_second * 1000;
        let amount:u128 = 41046619047619047619047619047;
        // unstaking
        let unlocked = proportion(amount, total_locked, total_supply);
        total_locked -= unlocked;
        total_supply -= amount;
        println!(
            " P_staked: {}\n P_unstaked: {}\n P-deviation: {}\n locked: {}\n supply: {}",
            compute_p(total_locked, total_supply, true),
            compute_p(total_locked, total_supply, false),
            convert_from_yocto_cheddar((p_staked - compute_p(total_locked, total_supply, true))),
            total_locked,
            total_supply
        );
        


    }
}
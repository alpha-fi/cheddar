//! Implement all the relevant logic for owner of this contract.

use crate::*;

#[near_bindgen]
impl Contract {
    pub fn set_owner(&mut self, owner_id: AccountId) {
        self.assert_owner();
        assert!(
            env::is_valid_account_id(owner_id.as_bytes()),
            "Account @{} is invalid!",
            owner_id
        );
        self.owner_id = owner_id;
    }

    pub fn get_owner(&self) -> AccountId {
        self.owner_id.clone()
    }

    pub fn modify_monthly_reward(&mut self, monthly_reward: U128, distribute_before_change: bool) {
        self.assert_owner();
        if distribute_before_change {
            self.distribute_reward();
        }
        self.monthly_reward = monthly_reward.into();
    }

    pub fn reset_reward_genesis_time_in_sec(&mut self, reward_genesis_time_in_sec: u32) {
        self.assert_owner();
        let cur_time = nano_to_sec(env::block_timestamp());
        if reward_genesis_time_in_sec < cur_time {
            panic!("{}", ERR_RESET_TIME_IS_PAST_TIME);
        } else if self.reward_genesis_time_in_sec < cur_time {
            panic!("{}", ERR_REWARD_GENESIS_TIME_PASSED);
        }
        self.reward_genesis_time_in_sec = reward_genesis_time_in_sec;
        self.prev_distribution_time_in_sec = reward_genesis_time_in_sec;
    }

    pub(crate) fn assert_owner(&self) {
        assert_eq!(
            env::predecessor_account_id(),
            self.owner_id,
            "{}", ERR_NOT_ALLOWED
        );
    }

    // State migration function.
    // For next version upgrades, change this function.
    #[init(ignore_state)]
    #[private]
    pub fn migrate() -> Self {
        let prev: Contract = env::state_read().expect(ERR_NOT_INITIALIZED);
        prev
    }
}

#[cfg(target_arch = "wasm32")]
mod upgrade {
    use super::*;
    use near_sdk::env;
    use near_sdk::Gas;
    use near_sys as sys;
    /// Self upgrade and call migrate, optimizes gas by not loading into memory the code.
    /// Takes as input non serialized set of bytes of the code.
    /// After upgrade we call *pub fn migrate()* on the NEW CONTRACT CODE
    #[no_mangle]
    pub fn upgrade() {
        /// Gas for calling migration call. One Tera - 1 TGas
        pub const GAS_FOR_MIGRATE_CALL: Gas = Gas(5_000_000_000_000);
        /// 20 Tgas
        pub const GAS_FOR_UPGRADE: Gas = Gas(20_000_000_000_000);
        const BLOCKCHAIN_INTERFACE_NOT_SET_ERR: &str = "Blockchain interface not set.";

        env::setup_panic_hook();

        /// assert ownership
        let contract: Contract = env::state_read().expect("ERR_CONTRACT_IS_NOT_INITIALIZED");
        contract.assert_owner();

        let current_id = env::current_account_id();
        let migrate_method_name = "migrate".as_bytes().to_vec();
        let attached_gas = env::prepaid_gas() - env::used_gas() - GAS_FOR_UPGRADE;
        unsafe {
            // Load input (NEW CONTRACT CODE) into register 0.
            sys::input(0);
            // prepare self-call promise
            let promise_id = sys::promise_batch_create(current_id.as_bytes().len() as _, current_id.as_bytes().as_ptr() as _);
            
            // #Action_1 - deploy/upgrade code from register 0
            sys::promise_batch_action_deploy_contract(promise_id, u64::MAX as _, 0);
            // #Action_2 - schedule a call for migrate
            // Execute on NEW CONTRACT CODE
            sys::promise_batch_action_function_call(
                    promise_id,
                    migrate_method_name.len() as _,
                    migrate_method_name.as_ptr() as _,
                    0 as _,
                    0 as _,
                    0 as _,
                    u64::from(attached_gas),
                );
        }
    }
}
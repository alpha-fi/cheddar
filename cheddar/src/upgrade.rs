#[cfg(target_arch = "wasm32")]
mod upgrade {
    use near_sdk::env;
    use near_sdk::Gas;
    use crate::Contract;
    use near_sys as sys;

    use super::*;
    use crate::util::*;

    /// Self upgrade and call migrate, optimizes gas by not loading into memory the code.
    /// Takes as input non serialized set of bytes of the code.
    /// After upgrade we call *pub fn migrate()* on the NEW CONTRACT CODE
    #[no_mangle]
    pub fn upgrade() {
        /// Gas for calling migration call. One Tera - 1 TGas
        /// 20 Tgas
        pub const GAS_FOR_UPGRADE: Gas = Gas(20_000_000_000_000);
        const BLOCKCHAIN_INTERFACE_NOT_SET_ERR: &str = "Blockchain interface not set.";

        env::setup_panic_hook();

        ///assert ownership
        #[allow(unused_doc_comments)]
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
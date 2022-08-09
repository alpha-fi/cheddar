use near_sdk_sim::{call, view, to_yocto};
use xcheddar_token::ContractMetadata;
use near_sdk::json_types::U128;

mod common;
use crate::common::{
    init::*,
    utils::*
};

pub const DURATION_30DAYS_IN_SEC: u32 = 60 * 60 * 24 * 30;
pub const DURATION_1DAY_IN_SEC: u32 = 60 * 60 * 24;

#[test]
//passed
fn test_reset_reward_genesis_time(){
    let (root, owner, _, cheddar_contract, xcheddar_contract) = 
        init_env(true);
    
    let xcheddar_info = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    let init_genesis_time = xcheddar_info.reward_genesis_time_in_sec;
    assert_eq!(init_genesis_time, xcheddar_info.prev_distribution_time_in_sec);

    // reward_distribute won't touch anything before genesis time
    call!(
        owner,
        xcheddar_contract.modify_monthly_reward(to_yocto("1").into(), true)
    )
    .assert_success();
    call!(
        owner,
        cheddar_contract.ft_transfer_call(xcheddar_contract.account_id(), to_yocto("100").into(), None, "reward".to_string()),
        deposit = 1
    )
    .assert_success();
    let xcheddar_info = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    assert_eq!(init_genesis_time, xcheddar_info.reward_genesis_time_in_sec);
    assert_eq!(init_genesis_time, xcheddar_info.prev_distribution_time_in_sec);
    assert_eq!(U128(to_yocto("100")), xcheddar_info.undistributed_reward);
    assert_eq!(U128(to_yocto("1")), xcheddar_info.monthly_reward);
    assert_eq!(U128(to_yocto("100")), xcheddar_info.cur_undistributed_reward);
    assert_eq!(U128(to_yocto("0")), xcheddar_info.cur_locked_token_amount);

    // and reward won't be distributed before genesis time
    root.borrow_runtime_mut().cur_block.block_timestamp = 100_000_000_000;
    let xcheddar_info = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    assert_eq!(U128(to_yocto("100")), xcheddar_info.cur_undistributed_reward);
    assert_eq!(U128(to_yocto("0")), xcheddar_info.cur_locked_token_amount);

    // and nothing happen even if some action invoke the reward distribution before genesis time
    call!(
        owner,
        xcheddar_contract.modify_monthly_reward(to_yocto("5").into(), true)
    )
    .assert_success();
    let xcheddar_info = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    assert_eq!(init_genesis_time, xcheddar_info.reward_genesis_time_in_sec);
    assert_eq!(init_genesis_time, xcheddar_info.prev_distribution_time_in_sec);
    assert_eq!(U128(to_yocto("100")), xcheddar_info.undistributed_reward);
    assert_eq!(U128(to_yocto("5")), xcheddar_info.monthly_reward);
    assert_eq!(U128(to_yocto("100")), xcheddar_info.cur_undistributed_reward);
    assert_eq!(U128(to_yocto("0")), xcheddar_info.cur_locked_token_amount);
    
    // change genesis time would also change prev_distribution_time_in_sec
    let current_timestamp = root.borrow_runtime().cur_block.block_timestamp;
    call!(
        owner,
        xcheddar_contract.reset_reward_genesis_time_in_sec(nano_to_sec(current_timestamp) + 50)
    ).assert_success();
    let xcheddar_info = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    assert_eq!(xcheddar_info.reward_genesis_time_in_sec, nano_to_sec(current_timestamp) + 50);
    assert_eq!(xcheddar_info.prev_distribution_time_in_sec, nano_to_sec(current_timestamp) + 50);
    assert_eq!(U128(to_yocto("100")), xcheddar_info.undistributed_reward);
    assert_eq!(U128(to_yocto("5")), xcheddar_info.monthly_reward);
    assert_eq!(U128(to_yocto("100")), xcheddar_info.cur_undistributed_reward);
    assert_eq!(U128(to_yocto("0")), xcheddar_info.cur_locked_token_amount);

    // 2 month passed
    root.borrow_runtime_mut().cur_block.block_timestamp = current_timestamp + sec_to_nano(2 * DURATION_30DAYS_IN_SEC);
    let xcheddar_info = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    assert_eq!(U128(to_yocto("95")), xcheddar_info.cur_undistributed_reward);
    assert_eq!(U128(to_yocto("5")), xcheddar_info.cur_locked_token_amount);
    // when some call invoke reward distribution after reward genesis time
    root.borrow_runtime_mut().cur_block.block_timestamp = current_timestamp + sec_to_nano(DURATION_30DAYS_IN_SEC + DURATION_1DAY_IN_SEC);
    call!(
        owner,
        xcheddar_contract.modify_monthly_reward(to_yocto("10").into(), true)
    )
    .assert_success();
    //3 month passed
    root.borrow_runtime_mut().cur_block.block_timestamp = current_timestamp + sec_to_nano(3 * DURATION_30DAYS_IN_SEC);
    let xcheddar_info = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    assert_eq!(xcheddar_info.reward_genesis_time_in_sec, nano_to_sec(current_timestamp) + 50);
    assert_eq!(xcheddar_info.prev_distribution_time_in_sec, nano_to_sec(current_timestamp) + 2678401);
    assert_eq!(U128(to_yocto("95")), xcheddar_info.undistributed_reward);
    assert_eq!(U128(to_yocto("5")), xcheddar_info.locked_token_amount);
    assert_eq!(U128(to_yocto("10")), xcheddar_info.monthly_reward);
    assert_eq!(U128(to_yocto("85")), xcheddar_info.cur_undistributed_reward);
    assert_eq!(U128(to_yocto("15")), xcheddar_info.cur_locked_token_amount);

}
//passed
#[test]
fn test_reset_reward_genesis_time_use_past_time(){
    let (root, owner, _, _, xcheddar_contract) = 
        init_env(true);
    
    let xcheddar_info = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();

    let current_timestamp = root.borrow_runtime().cur_block.block_timestamp;
    let out_come = call!(
        owner,
        xcheddar_contract.reset_reward_genesis_time_in_sec(nano_to_sec(current_timestamp) - 1)
    );
    assert_eq!(get_error_count(&out_come), 1);
    assert!(get_error_status(&out_come).contains("Used reward_genesis_time_in_sec must be less than current time!"));

    let xcheddar_info1 = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    assert_eq!(xcheddar_info.reward_genesis_time_in_sec, xcheddar_info1.reward_genesis_time_in_sec);
}
//passed
#[test]
fn test_reward_genesis_time_passed(){
    let (root, owner, _, _, xcheddar_contract) = 
        init_env(true);
    
    let xcheddar_info = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();

    root.borrow_runtime_mut().cur_block.block_timestamp = (xcheddar_info.reward_genesis_time_in_sec + 1) as u64 * 1_000_000_000;
    let current_timestamp = root.borrow_runtime().cur_block.block_timestamp;
    let out_come = call!(
        owner,
        xcheddar_contract.reset_reward_genesis_time_in_sec(nano_to_sec(current_timestamp) + 1)
    );
    assert_eq!(get_error_count(&out_come), 1);
    assert!(get_error_status(&out_come).contains("Setting in contract Genesis time must be less than current time!"));

    let xcheddar_info1 = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    assert_eq!(xcheddar_info.reward_genesis_time_in_sec, xcheddar_info1.reward_genesis_time_in_sec);
}

#[test]
fn test_modify_monthly_reward(){
    let (_, owner, _, _, xcheddar_contract) = 
        init_env(true);
    
    call!(
        owner,
        xcheddar_contract.modify_monthly_reward(to_yocto("1").into(), true)
    )
    .assert_success();
    let xcheddar_info = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    assert_eq!(xcheddar_info.monthly_reward.0, to_yocto("1"));
}
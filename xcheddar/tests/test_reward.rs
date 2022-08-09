use near_sdk_sim::{call, view, to_yocto};
use xcheddar_token::ContractMetadata;
use near_sdk::json_types::U128;

mod common;
use crate::common::{
    init::*,
    utils::*
};
//failed
#[test]
fn test_reward() {
    let (root, owner, user, cheddar_contract, xcheddar_contract) = 
        init_env(true);
    let mut total_reward = 0;
    let mut total_locked = 0;
    let mut total_supply = 0;

    call!(
        owner,
        xcheddar_contract.modify_monthly_reward(to_yocto("1").into(), true)
    )
    .assert_success();
    let xcheddar_info = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    assert_eq!(xcheddar_info.monthly_reward.0, to_yocto("1"));

    let current_timestamp = root.borrow_runtime_mut().cur_block.block_timestamp;
    call!(
        owner,
        xcheddar_contract.reset_reward_genesis_time_in_sec(nano_to_sec(current_timestamp) + 10)
    ).assert_success();
    let xcheddar_info = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    assert_eq!(xcheddar_info.reward_genesis_time_in_sec, nano_to_sec(current_timestamp) + 10);
    
    root.borrow_runtime_mut().cur_block.block_timestamp = sec_to_nano(nano_to_sec(current_timestamp) + 10);

    //add reward trigger distribute_reward, just update prev_distribution_time
    call!(
        owner,
        cheddar_contract.ft_transfer_call(xcheddar_contract.account_id(), to_yocto("100").into(), None, "reward".to_string()),
        deposit = 1
    )
    .assert_success();
    total_reward += to_yocto("100");

    let xcheddar_info0 = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    assert_xcheddar(&xcheddar_info0, to_yocto("100"), 0, 0);
    assert_eq!(to_yocto("1"), xcheddar_info0.monthly_reward.0);

    
    //stake trigger distribute_reward
    call!(
        user,
        cheddar_contract.ft_transfer_call(xcheddar_contract.account_id(), to_yocto("11").into(), None, "".to_string()),
        deposit = 1
    )
    .assert_success();
    total_locked += to_yocto("11");
    total_supply += to_yocto("11");

    let xcheddar_info1 = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    let time_diff = (xcheddar_info1.prev_distribution_time_in_sec - xcheddar_info0.prev_distribution_time_in_sec) / DURATION_30DAYS_IN_SEC;
    total_reward -= time_diff as u128 * xcheddar_info1.monthly_reward.0;
    total_locked += time_diff as u128 * xcheddar_info1.monthly_reward.0;
    assert_xcheddar(&xcheddar_info1, total_reward, total_locked, total_supply);
    assert_eq!(to_yocto("89"), view!(cheddar_contract.ft_balance_of(user.account_id())).unwrap_json::<U128>().0);

    assert!(root.borrow_runtime_mut().produce_block().is_ok());

    //modify_monthly_reward trigger distribute_reward
    call!(
        owner,
        xcheddar_contract.modify_monthly_reward(to_yocto("1").into(), true)
    )
    .assert_success();
    let xcheddar_info2 = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    assert_eq!(xcheddar_info2.monthly_reward.0, to_yocto("1"));

    let time_diff = (xcheddar_info2.prev_distribution_time_in_sec - xcheddar_info1.prev_distribution_time_in_sec) / DURATION_30DAYS_IN_SEC;
    total_reward -= time_diff as u128 * xcheddar_info2.monthly_reward.0;
    total_locked += time_diff as u128 * xcheddar_info2.monthly_reward.0;
    assert_xcheddar(&xcheddar_info2, total_reward, total_locked, total_supply);
    
    assert!(root.borrow_runtime_mut().produce_block().is_ok());

    //modify_monthly_reward not trigger distribute_reward
    call!(
        owner,
        xcheddar_contract.modify_monthly_reward(to_yocto("1").into(), false)
    )
    .assert_success();
    let xcheddar_info2_1 = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();

    let time_diff = (xcheddar_info2_1.prev_distribution_time_in_sec - xcheddar_info2.prev_distribution_time_in_sec) / DURATION_30DAYS_IN_SEC;
    total_reward -= time_diff as u128 * xcheddar_info2_1.monthly_reward.0;
    total_locked += time_diff as u128 * xcheddar_info2_1.monthly_reward.0;
    assert_xcheddar(&xcheddar_info2_1, total_reward, total_locked, total_supply);
    assert_eq!(time_diff, 0);
    
    assert!(root.borrow_runtime_mut().produce_block().is_ok());

    //nothing trigger distribute_reward
    let xcheddar_info3 = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    assert_xcheddar(&xcheddar_info3, total_reward, total_locked, total_supply);
    assert_eq!(to_yocto("89"), view!(cheddar_contract.ft_balance_of(user.account_id())).unwrap_json::<U128>().0);
    
    //add reward trigger distribute_reward
    call!(
        owner,
        cheddar_contract.ft_transfer_call(xcheddar_contract.account_id(), to_yocto("100").into(), None, "reward".to_string()),
        deposit = 1
    )
    .assert_success();
    total_reward += to_yocto("100");

    let xcheddar_info4 = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    let time_diff = (xcheddar_info4.prev_distribution_time_in_sec - xcheddar_info3.prev_distribution_time_in_sec) / DURATION_30DAYS_IN_SEC;
    total_reward -= time_diff as u128 * xcheddar_info4.monthly_reward.0;
    total_locked += time_diff as u128 * xcheddar_info4.monthly_reward.0;
    assert_xcheddar(&xcheddar_info4, total_reward, total_locked, total_supply);

    assert!(root.borrow_runtime_mut().produce_block().is_ok());

    //unstake trigger distribute_reward
    call!(
        user,
        xcheddar_contract.unstake(to_yocto("10").into()),
        deposit = 1
    )
    .assert_success();

    let xcheddar_info5 = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    let time_diff = (xcheddar_info5.prev_distribution_time_in_sec - xcheddar_info4.prev_distribution_time_in_sec) / DURATION_30DAYS_IN_SEC;
    total_reward -= time_diff as u128 * xcheddar_info5.monthly_reward.0;
    total_locked += time_diff as u128 * xcheddar_info5.monthly_reward.0;

    let unlocked = (U256::from(to_yocto("10")) * U256::from(total_locked) / U256::from(total_supply)).as_u128();
    total_locked -= unlocked;
    total_supply -= to_yocto("10");

    assert_eq!(to_yocto("1"), total_supply);
    assert_xcheddar(&xcheddar_info5, total_reward, total_locked, total_supply);
    assert_eq!(to_yocto("89") + unlocked, view!(cheddar_contract.ft_balance_of(user.account_id())).unwrap_json::<U128>().0);

    assert!(root.borrow_runtime_mut().produce_blocks(1000).is_ok());

    //nothing trigger distribute_reward
    let xcheddar_info6 = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    assert_xcheddar(&xcheddar_info6, total_reward, total_locked, total_supply);

    //stake trigger distribute_reward，total_reward less then distribute_reward
    call!(
        user,
        cheddar_contract.ft_transfer_call(xcheddar_contract.account_id(), to_yocto("10").into(), None, "".to_string()),
        deposit = 1
    )
    .assert_success();
    
    let xcheddar_info7 = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    let time_diff = (xcheddar_info7.prev_distribution_time_in_sec - xcheddar_info6.prev_distribution_time_in_sec) / DURATION_30DAYS_IN_SEC;
    assert!(total_reward < time_diff as u128 * xcheddar_info7.monthly_reward.0);
    total_locked += total_reward;
    total_reward -= total_reward;

    total_supply += (U256::from(to_yocto("10")) * U256::from(total_supply) / U256::from(total_locked)).as_u128();
    total_locked += to_yocto("10");
    
    assert_xcheddar(&xcheddar_info7, total_reward, total_locked, total_supply);
    assert_eq!(to_yocto("79") + unlocked, view!(cheddar_contract.ft_balance_of(user.account_id())).unwrap_json::<U128>().0);

    //stake when total_locked contains reward
    call!(
        user,
        cheddar_contract.ft_transfer_call(xcheddar_contract.account_id(), to_yocto("10").into(), None, "".to_string()),
        deposit = 1
    )
    .assert_success();

    total_supply += (U256::from(to_yocto("10")) * U256::from(total_supply) / U256::from(total_locked)).as_u128();
    total_locked += to_yocto("10");

    let xcheddar_info8 = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    assert_xcheddar(&xcheddar_info8, total_reward, total_locked, total_supply);
    assert_eq!(to_yocto("69") + unlocked, view!(cheddar_contract.ft_balance_of(user.account_id())).unwrap_json::<U128>().0);
}

#[test]
fn test_no_reward_before_reset_reward_genesis_time(){
    let (root, owner, user, cheddar_contract, xcheddar_contract) = 
        init_env(true);
    let mut total_reward = 0;
    let mut total_locked = 0;
    let mut total_supply = 0;

    call!(
        owner,
        xcheddar_contract.modify_monthly_reward(to_yocto("1").into(), true)
    )
    .assert_success();
    let xcheddar_info = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    assert_eq!(xcheddar_info.monthly_reward.0, to_yocto("1"));

    //add reward trigger distribute_reward, just update prev_distribution_time
    call!(
        owner,
        cheddar_contract.ft_transfer_call(xcheddar_contract.account_id(), to_yocto("100").into(), None, "reward".to_string()),
        deposit = 1
    )
    .assert_success();
    total_reward += to_yocto("100");

    let xcheddar_info1 = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    assert_xcheddar(&xcheddar_info1, to_yocto("100"), 0, 0);
    assert_eq!(to_yocto("1"), xcheddar_info1.monthly_reward.0);

    //stake trigger distribute_reward
    call!(
        user,
        cheddar_contract.ft_transfer_call(xcheddar_contract.account_id(), to_yocto("10").into(), None, "".to_string()),
        deposit = 1
    )
    .assert_success();
    total_locked += to_yocto("10");
    total_supply += to_yocto("10");

    let xcheddar_info2 = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    let time_diff = xcheddar_info2.prev_distribution_time_in_sec - xcheddar_info1.prev_distribution_time_in_sec;
    total_reward -= time_diff as u128 * xcheddar_info2.monthly_reward.0;
    total_locked += time_diff as u128 * xcheddar_info2.monthly_reward.0;
    assert_xcheddar(&xcheddar_info2, total_reward, total_locked, total_supply);
    assert_eq!(to_yocto("90"), view!(cheddar_contract.ft_balance_of(user.account_id())).unwrap_json::<U128>().0);

    assert!(root.borrow_runtime_mut().produce_blocks(10).is_ok());

    //stake trigger distribute_reward again
    call!(
        user,
        cheddar_contract.ft_transfer_call(xcheddar_contract.account_id(), to_yocto("10").into(), None, "".to_string()),
        deposit = 1
    )
    .assert_success();
    total_locked += to_yocto("10");
    total_supply += to_yocto("10");

    let xcheddar_info3 = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    let time_diff = xcheddar_info3.prev_distribution_time_in_sec - xcheddar_info2.prev_distribution_time_in_sec;
    total_reward -= time_diff as u128 * xcheddar_info3.monthly_reward.0;
    total_locked += time_diff as u128 * xcheddar_info3.monthly_reward.0;
    assert_xcheddar(&xcheddar_info3, total_reward, total_locked, total_supply);
    assert_eq!(to_yocto("80"), view!(cheddar_contract.ft_balance_of(user.account_id())).unwrap_json::<U128>().0);

    assert_eq!(xcheddar_info3.undistributed_reward.0, to_yocto("100"));
    assert_eq!(xcheddar_info3.locked_token_amount.0, to_yocto("20"));

    assert!(root.borrow_runtime_mut().produce_blocks(10).is_ok());

    //unstake trigger distribute_reward
    call!(
        user,
        xcheddar_contract.unstake(to_yocto("10").into()),
        deposit = 1
    )
    .assert_success();

    let xcheddar_info4 = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    let time_diff = xcheddar_info4.prev_distribution_time_in_sec - xcheddar_info3.prev_distribution_time_in_sec;
    total_reward -= time_diff as u128 * xcheddar_info4.monthly_reward.0;
    total_locked += time_diff as u128 * xcheddar_info4.monthly_reward.0;

    let unlocked = (U256::from(to_yocto("10")) * U256::from(total_locked) / U256::from(total_supply)).as_u128();
    total_locked -= unlocked;
    total_supply -= to_yocto("10");

    assert_eq!(to_yocto("10"), total_locked);
    assert_eq!(to_yocto("10"), total_supply);
    assert_xcheddar(&xcheddar_info4, total_reward, total_locked, total_supply);
    assert_eq!(to_yocto("80") + unlocked, view!(cheddar_contract.ft_balance_of(user.account_id())).unwrap_json::<U128>().0);

    assert_eq!(unlocked, to_yocto("10"));
    assert_eq!(xcheddar_info4.undistributed_reward.0, to_yocto("100"));
    assert_eq!(xcheddar_info4.locked_token_amount.0, to_yocto("10"));
}
use near_sdk_sim::{call, view, to_yocto};
use xcheddar_token::ContractMetadata;
use near_sdk::json_types::U128;

mod common;
use crate::common::{
    init::*,
    utils::*
};

#[test]
fn test_unstake(){
    let (_, _, user, cheddar_contract, xcheddar_contract) = 
        init_env(true);
    
    call!(
        user,
        cheddar_contract.ft_transfer_call(xcheddar_contract.account_id(), to_yocto("10").into(), None, "".to_string()),
        deposit = 1
    )
    .assert_success();

    assert_eq!(to_yocto("90"), view!(cheddar_contract.ft_balance_of(user.account_id())).unwrap_json::<U128>().0);
    let current_xcheddar_info = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    assert_xcheddar(&current_xcheddar_info, 0, to_yocto("10"), to_yocto("10"));
    assert_eq!(100000000_u128, view!(xcheddar_contract.get_virtual_price()).unwrap_json::<U128>().0);

    call!(
        user,
        xcheddar_contract.unstake(to_yocto("8").into()),
        deposit = 1
    )
    .assert_success();

    assert_eq!(to_yocto("98"), view!(cheddar_contract.ft_balance_of(user.account_id())).unwrap_json::<U128>().0);
    let current_xcheddar_info = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    assert_xcheddar(&current_xcheddar_info, 0, to_yocto("2"), to_yocto("2"));
    assert_eq!(100000000_u128, view!(xcheddar_contract.get_virtual_price()).unwrap_json::<U128>().0);

    let out_come = call!(
        user,
        xcheddar_contract.unstake(to_yocto("2").into()),
        deposit = 1
    );
    assert_eq!(get_error_count(&out_come), 1);
    assert!(get_error_status(&out_come).contains("At least 1 Cheddar must be on lockup contract account"));
}

#[test]
fn test_unstake_empty_total_supply(){
    let (_, _, user, cheddar_contract, xcheddar_contract) = 
        init_env(true);

    let out_come = call!(
        user,
        xcheddar_contract.unstake(to_yocto("1").into()),
        deposit = 1
    );
    assert_eq!(get_error_count(&out_come), 1);
    assert!(get_error_status(&out_come).contains("Total supply cannot be empty!"));

    assert_eq!(to_yocto("100"), view!(cheddar_contract.ft_balance_of(user.account_id())).unwrap_json::<U128>().0);
    let current_xcheddar_info = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    assert_xcheddar(&current_xcheddar_info, 0, 0, 0);
}

#[test]
fn test_unstake_not_enough_balance(){
    let (_, _, user, cheddar_contract, xcheddar_contract) = 
        init_env(true);
    
    call!(
        user,
        cheddar_contract.ft_transfer_call(xcheddar_contract.account_id(), to_yocto("10").into(), None, "".to_string()),
        deposit = 1
    )
    .assert_success();

    assert_eq!(to_yocto("90"), view!(cheddar_contract.ft_balance_of(user.account_id())).unwrap_json::<U128>().0);
    let current_xcheddar_info = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    assert_xcheddar(&current_xcheddar_info, 0, to_yocto("10"), to_yocto("10"));
    assert_eq!(100000000_u128, view!(xcheddar_contract.get_virtual_price()).unwrap_json::<U128>().0);

    let out_come = call!(
        user,
        xcheddar_contract.unstake(to_yocto("11").into()),
        deposit = 1
    );
    assert_eq!(get_error_count(&out_come), 1);
    assert!(get_error_status(&out_come).contains("The account doesn't have enough balance"));

    assert_eq!(to_yocto("90"), view!(cheddar_contract.ft_balance_of(user.account_id())).unwrap_json::<U128>().0);
    let current_xcheddar_info = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    assert_xcheddar(&current_xcheddar_info, 0, to_yocto("10"), to_yocto("10"));
    assert_eq!(100000000_u128, view!(xcheddar_contract.get_virtual_price()).unwrap_json::<U128>().0);
}
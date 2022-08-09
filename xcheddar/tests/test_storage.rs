use near_sdk_sim::{call, view, to_yocto};
use xcheddar_token::ContractMetadata;

mod common;
use crate::common::init::*;
//passed
#[test]
fn test_account_number(){
    let (root, _, user, _, xcheddar_contract) = 
        init_env(true);
    let current_xcheddar_info = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    assert_eq!(current_xcheddar_info.account_number, 1);

    let user2 = root.create_user("user2".parse().unwrap(), to_yocto("100"));
    call!(user2, xcheddar_contract.storage_deposit(None, None), deposit = to_yocto("1")).assert_success();
    let current_xcheddar_info = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    assert_eq!(current_xcheddar_info.account_number, 2);

    call!(user2, xcheddar_contract.storage_unregister(None), deposit = 1).assert_success();
    let current_xcheddar_info = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    assert_eq!(current_xcheddar_info.account_number, 1);

    call!(user, xcheddar_contract.storage_unregister(None), deposit = 1).assert_success();
    let current_xcheddar_info = view!(xcheddar_contract.contract_metadata()).unwrap_json::<ContractMetadata>();
    assert_eq!(current_xcheddar_info.account_number, 0);
}
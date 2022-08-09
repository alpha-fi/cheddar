use near_sdk_sim::{call, deploy, init_simulator, to_yocto, ContractAccount, UserAccount};

use cheddar_coin::ContractContract as CheddarToken;
use xcheddar_token::ContractContract as XCheddarToken;

near_sdk_sim::lazy_static_include::lazy_static_include_bytes! {
    TEST_WASM_BYTES => "./res/cheddar_coin.wasm",
    XCHEDDAR_WASM_BYTES => "./res/xcheddar_token.wasm", 
}

pub fn init_env(register_user: bool) -> (UserAccount, UserAccount, UserAccount, ContractAccount<CheddarToken>, ContractAccount<XCheddarToken>){
    let root = init_simulator(None);

    let owner = root.create_user("owner".parse().unwrap(), to_yocto("100"));
    let user = root.create_user("user".parse().unwrap(), to_yocto("100"));

    let cheddar_contract = deploy!(
        contract: CheddarToken,
        contract_id: "cheddar",
        bytes: &TEST_WASM_BYTES,
        signer_account: root
    );
    call!(root, cheddar_contract.new(owner.account_id())).assert_success();
    call!(owner, cheddar_contract.storage_deposit(None, None), deposit = to_yocto("1")).assert_success();
    call!(user, cheddar_contract.storage_deposit(None, None), deposit = to_yocto("1")).assert_success();
    
    call!(
        owner, 
        cheddar_contract.add_minter(owner.account_id()),
        deposit = 1
    );
    call!(
        owner, 
        cheddar_contract.add_minter(user.account_id()),
        deposit = 1
    );

    call!(
        owner, 
        cheddar_contract.ft_mint(&owner.account_id(), to_yocto("10000").into(), None),
        deposit = 1
    ).assert_success();
    call!(
        user, 
        cheddar_contract.ft_mint(&user.account_id(), to_yocto("100").into(), None),
        deposit = 1
    ).assert_success();

    let xcheddar_contract = deploy!(
        contract: XCheddarToken,
        contract_id: "xcheddar",
        bytes: &XCHEDDAR_WASM_BYTES,
        signer_account: root
    );
    call!(root, xcheddar_contract.new(owner.account_id(), cheddar_contract.account_id())).assert_success();
    call!(root, cheddar_contract.storage_deposit(Some(xcheddar_contract.account_id()), None), deposit = to_yocto("1")).assert_success();
    if register_user {
        call!(user, xcheddar_contract.storage_deposit(None, None), deposit = to_yocto("1")).assert_success();
    }
    (root, owner, user, cheddar_contract, xcheddar_contract)
}
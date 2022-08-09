
use near_sdk_sim::{deploy, view, init_simulator, to_yocto};

use xcheddar_token::{ContractContract as XCheddar, ContractMetadata};

near_sdk_sim::lazy_static_include::lazy_static_include_bytes! {
    PREV_XCHEDDAR_WASM_BYTES => "./res/xcheddar_token.wasm",
    XCHEDDAR_WASM_BYTES => "./res/xcheddar_token.wasm",
}

#[test]
fn test_upgrade() {
    let root = init_simulator(None);
    let test_user = root.create_user("test".parse().unwrap(), to_yocto("100"));
    let xcheddar = deploy!(
        contract: XCheddar,
        contract_id: "xcheddar".to_string(),
        bytes: &PREV_XCHEDDAR_WASM_BYTES,
        signer_account: root,
        init_method: new(root.account_id(), root.account_id())
    );
    // Failed upgrade with no permissions.
    let result = test_user
        .call(
            xcheddar.account_id().clone(),
            "upgrade",
            &XCHEDDAR_WASM_BYTES,
            near_sdk_sim::DEFAULT_GAS,
            0,
        )
        .status();
    assert!(format!("{:?}", result).contains("Owner's method"));

    root.call(
        xcheddar.account_id().clone(),
        "upgrade",
        &XCHEDDAR_WASM_BYTES,
        near_sdk_sim::DEFAULT_GAS,
        0,
    )
    .assert_success();
    let metadata = view!(xcheddar.contract_metadata()).unwrap_json::<ContractMetadata>();
    // println!("{:#?}", metadata);
    assert_eq!(metadata.version, "1.0.2".to_string());

    // Upgrade to the same code migration is skipped.
    root.call(
        xcheddar.account_id().clone(),
        "upgrade",
        &XCHEDDAR_WASM_BYTES,
        near_sdk_sim::DEFAULT_GAS,
        0,
    )
    .assert_success();
}
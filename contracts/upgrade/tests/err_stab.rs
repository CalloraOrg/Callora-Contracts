use callora_upgrade::errors::{ContractError, UpgradeError};
use callora_upgrade::{CalloraUpgrade, CalloraUpgradeClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, BytesN, Env};
use std::collections::BTreeSet;

/// Frozen snapshot of client-facing ContractError / UpgradeError code mappings.
const FROZEN_ERROR_SNAPSHOT: [(u32, UpgradeError); 16] = [
    (1, UpgradeError::NotInitialized),
    (2, UpgradeError::AlreadyInitialized),
    (3, UpgradeError::Unauthorized),
    (4, UpgradeError::InvalidWasmHash),
    (5, UpgradeError::UpgradeNotAllowed),
    (6, UpgradeError::MigrationPending),
    (7, UpgradeError::TimelockNotExpired),
    (8, UpgradeError::SameWasmHash),
    (9, UpgradeError::SameVersion),
    (10, UpgradeError::InvalidVersion),
    (11, UpgradeError::Overflow),
    (12, UpgradeError::AlreadyUpgraded),
    (13, UpgradeError::StaleNonce),
    (14, UpgradeError::MigrationSameAddress),
    (15, UpgradeError::InvalidMigrationTarget),
    (16, UpgradeError::NoUpgradePending),
];

#[test]
fn test_error_codes_frozen_against_snapshot() {
    let mut seen = BTreeSet::new();

    for (expected_code, variant) in FROZEN_ERROR_SNAPSHOT {
        let code_as_u32 = variant as u32;
        assert_eq!(
            code_as_u32, expected_code,
            "Error code for variant {:?} does not match frozen snapshot discriminant",
            variant
        );

        assert!(
            seen.insert(expected_code),
            "Duplicate error code detected in snapshot: {expected_code}"
        );
    }

    assert_eq!(
        seen.len(),
        FROZEN_ERROR_SNAPSHOT.len(),
        "Snapshot size mismatch"
    );
}

#[test]
fn test_contract_error_type_alias_compatibility() {
    let err: ContractError = UpgradeError::Unauthorized;
    assert_eq!(err as u32, 3);
}

#[test]
fn test_error_code_docs_coverage() {
    let docs = include_str!("../../../docs/ERROR_CODES.md");

    let expected_doc_lines = [
        "| 1 | `NotInitialized` | Upgrade | Contract has not been initialized yet |",
        "| 2 | `AlreadyInitialized` | Upgrade | `init` was called more than once |",
        "| 3 | `Unauthorized` | Upgrade | Caller is not authorized for the operation |",
        "| 4 | `InvalidWasmHash` | Upgrade | Provided WASM hash is zero or invalid |",
        "| 5 | `UpgradeNotAllowed` | Upgrade | Upgrade operation is currently disabled |",
        "| 6 | `MigrationPending` | Upgrade | A migration or upgrade is already pending |",
        "| 7 | `TimelockNotExpired` | Upgrade | Required timelock delay has not elapsed |",
        "| 8 | `SameWasmHash` | Upgrade | New WASM hash is identical to current WASM hash |",
        "| 9 | `SameVersion` | Upgrade | Proposed version matches current version |",
        "| 10 | `InvalidVersion` | Upgrade | Proposed version number is invalid or non-increasing |",
        "| 11 | `Overflow` | Upgrade | Arithmetic calculation overflowed |",
        "| 12 | `AlreadyUpgraded` | Upgrade | Contract has already been upgraded to this state |",
        "| 13 | `StaleNonce` | Upgrade | Transaction nonce is stale or invalid |",
        "| 14 | `MigrationSameAddress` | Upgrade | Target migration contract address matches source |",
        "| 15 | `InvalidMigrationTarget` | Upgrade | Target migration contract address is invalid |",
        "| 16 | `NoUpgradePending` | Upgrade | No pending upgrade was found to execute or cancel |",
    ];

    for line in expected_doc_lines {
        assert!(
            docs.contains(line),
            "missing upgrade error docs entry: {line}"
        );
    }
}

#[test]
fn test_contract_execution_returns_frozen_error_codes() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(CalloraUpgrade, ());
    let client = CalloraUpgradeClient::new(&env, &contract_id);

    // 1. Uninitialized query -> NotInitialized (code 1)
    let res_admin = client.try_get_admin();
    assert_eq!(res_admin, Err(Ok(UpgradeError::NotInitialized)));

    // 2. Initialize contract
    let admin = Address::generate(&env);
    let init_res = client.try_init(&admin);
    assert!(init_res.is_ok());

    // Double init -> AlreadyInitialized (code 2)
    let double_init_res = client.try_init(&admin);
    assert_eq!(double_init_res, Err(Ok(UpgradeError::AlreadyInitialized)));

    // 3. Unauthorized propose -> Unauthorized (code 3)
    let non_admin = Address::generate(&env);
    let dummy_hash = BytesN::from_array(&env, &[1u8; 32]);
    let unauth_prop = client.try_propose_upgrade(&non_admin, &dummy_hash, &3600);
    assert_eq!(unauth_prop, Err(Ok(UpgradeError::Unauthorized)));

    // 4. Propose valid upgrade
    let prop_res = client.try_propose_upgrade(&admin, &dummy_hash, &3600);
    assert!(prop_res.is_ok());

    // 5. Propose duplicate proposal -> MigrationPending (code 6)
    let pending_res = client.try_propose_upgrade(&admin, &dummy_hash, &3600);
    assert_eq!(pending_res, Err(Ok(UpgradeError::MigrationPending)));

    // 6. Execute before timelock -> TimelockNotExpired (code 7)
    let early_exec = client.try_execute_upgrade(&admin);
    assert_eq!(early_exec, Err(Ok(UpgradeError::TimelockNotExpired)));

    // 7. Cancel proposal
    let cancel_res = client.try_cancel_upgrade(&admin);
    assert!(cancel_res.is_ok());

    // 8. Cancel again -> NoUpgradePending (code 16)
    let cancel_again = client.try_cancel_upgrade(&admin);
    assert_eq!(cancel_again, Err(Ok(UpgradeError::NoUpgradePending)));

    // 9. Execute when no proposal -> NoUpgradePending (code 16)
    let exec_no_pending = client.try_execute_upgrade(&admin);
    assert_eq!(exec_no_pending, Err(Ok(UpgradeError::NoUpgradePending)));
}

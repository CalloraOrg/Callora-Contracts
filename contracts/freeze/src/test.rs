extern crate std;

use crate::errors::FreezeError;
use crate::{CalloraFreeze, CalloraFreezeClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, Symbol};
use std::collections::BTreeSet;

#[test]
fn test_init_and_get_admin() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(CalloraFreeze, ());
    let client = CalloraFreezeClient::new(&env, &contract_id);

    // Get admin before init returns NotInitialized
    assert_eq!(client.try_get_admin(), Err(Ok(FreezeError::NotInitialized)));

    let admin = Address::generate(&env);
    client.init(&admin);

    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.is_frozen(), false);

    // Double init fails
    assert_eq!(
        client.try_init(&admin),
        Err(Ok(FreezeError::AlreadyInitialized))
    );
}

#[test]
fn test_freeze_and_unfreeze_flow() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(CalloraFreeze, ());
    let client = CalloraFreezeClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.init(&admin);

    let reason = Symbol::new(&env, "exploit_risk");
    let freeze_res = client.try_freeze(&admin, &reason);
    assert!(freeze_res.is_ok());
    assert_eq!(client.is_frozen(), true);

    // Freeze when already frozen fails
    assert_eq!(
        client.try_freeze(&admin, &reason),
        Err(Ok(FreezeError::AlreadyFrozen))
    );

    // Unfreeze succeeds
    let unfreeze_res = client.try_unfreeze(&admin);
    assert!(unfreeze_res.is_ok());
    assert_eq!(client.is_frozen(), false);

    // Unfreeze when not frozen fails
    assert_eq!(
        client.try_unfreeze(&admin),
        Err(Ok(FreezeError::NotFrozen))
    );
}

#[test]
fn test_operator_freeze_permissions() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(CalloraFreeze, ());
    let client = CalloraFreezeClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let operator = Address::generate(&env);
    let stranger = Address::generate(&env);

    client.init(&admin);

    // Stranger freeze fails
    let reason = Symbol::new(&env, "emergency");
    assert_eq!(
        client.try_freeze(&stranger, &reason),
        Err(Ok(FreezeError::Unauthorized))
    );

    // Set operator
    client.set_freeze_operator(&admin, &Some(operator.clone()));
    assert_eq!(client.get_freeze_operator(), Some(operator.clone()));

    // Operator freeze succeeds
    let op_freeze = client.try_freeze(&operator, &reason);
    assert!(op_freeze.is_ok());
    assert_eq!(client.is_frozen(), true);

    // Operator unfreeze fails (only admin can unfreeze)
    assert_eq!(
        client.try_unfreeze(&operator),
        Err(Ok(FreezeError::Unauthorized))
    );

    // Admin unfreeze succeeds
    client.unfreeze(&admin);

    // Revoke operator
    client.set_freeze_operator(&admin, &None);
    assert_eq!(client.get_freeze_operator(), None);

    // Revoked operator freeze fails
    assert_eq!(
        client.try_freeze(&operator, &reason),
        Err(Ok(FreezeError::Unauthorized))
    );
}

#[test]
fn test_set_operator_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(CalloraFreeze, ());
    let client = CalloraFreezeClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let stranger = Address::generate(&env);

    client.init(&admin);

    assert_eq!(
        client.try_set_freeze_operator(&stranger, &Some(stranger.clone())),
        Err(Ok(FreezeError::Unauthorized))
    );
}

#[test]
fn test_freeze_error_codes_stability_and_uniqueness() {
    let mappings = [
        (1_u32, FreezeError::NotInitialized),
        (2, FreezeError::AlreadyInitialized),
        (3, FreezeError::Unauthorized),
        (4, FreezeError::AlreadyFrozen),
        (5, FreezeError::NotFrozen),
        (6, FreezeError::Overflow),
    ];

    let mut seen = BTreeSet::new();
    for (expected_code, variant) in mappings {
        assert_eq!(variant as u32, expected_code);
        assert!(
            seen.insert(expected_code),
            "duplicate freeze error code {expected_code}"
        );
    }

    assert_eq!(seen.len(), 6);
}

#[test]
fn test_error_code_docs_coverage() {
    let docs = include_str!("../../../docs/ERROR_CODES.md");
    let expected_lines = [
        "| 1 | `NotInitialized` | Freeze | Contract has not been initialized yet |",
        "| 2 | `AlreadyInitialized` | Freeze | `init` was called more than once |",
        "| 3 | `Unauthorized` | Freeze | Caller is not authorized for the operation |",
        "| 4 | `AlreadyFrozen` | Freeze | Contract is already frozen |",
        "| 5 | `NotFrozen` | Freeze | Contract is not currently frozen |",
        "| 6 | `Overflow` | Freeze | Arithmetic overflow detected |",
    ];

    for line in expected_lines {
        assert!(docs.contains(line), "missing freeze docs line: {line}");
    }
}

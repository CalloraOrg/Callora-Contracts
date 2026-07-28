#![cfg(test)]

use callora_reflector::{ReflectorContract, ReflectorContractClient};
use soroban_sdk::{
    testutils::{Address as _, AuthorizedFunction, AuthorizedInvocation},
    Address, Env, IntoVal, Symbol,
};

#[test]
fn test_mock_all_auths() {
    let env = Env::default();
    env.mock_all_auths(); // Harness 1: mock all auths (e.g. happy path without strict verification)

    let contract_id = env.register(ReflectorContract, ());
    let client = ReflectorContractClient::new(&env, &contract_id);

    let signer = Address::generate(&env);
    
    // Act
    client.reflect_auth(&signer);

    // Assert identity is captured
    assert_eq!(client.get_last_signer(), Some(signer.clone()));
}

#[test]
fn test_strict_auth_harness() {
    let env = Env::default();
    
    let contract_id = env.register(ReflectorContract, ());
    let client = ReflectorContractClient::new(&env, &contract_id);

    let signer = Address::generate(&env);

    // Harness 2: strict require_auth flow (mocking the exact invocation)
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &signer,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "reflect_auth",
            args: (&signer,).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    // Act
    client.reflect_auth(&signer);

    // Assert identity is captured
    assert_eq!(client.get_last_signer(), Some(signer.clone()));
}

#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
fn test_missing_auth_panics() {
    let env = Env::default();
    
    let contract_id = env.register(ReflectorContract, ());
    let client = ReflectorContractClient::new(&env, &contract_id);

    let signer = Address::generate(&env);

    // Harness 3: no auth mocked at all.
    // The `signer.require_auth()` call inside the contract will trigger a panic in the host.
    client.reflect_auth(&signer);
}

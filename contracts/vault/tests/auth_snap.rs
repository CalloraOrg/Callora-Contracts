#![cfg(test)]

use crate::{Vault, VaultClient};
use soroban_sdk::{testutils::Address as _, Address, Env, Symbol};

#[test]
fn test_vault_auth_snapshot() {
    let env = Env::default();

    env.mock_all_auths();

    let caller = Address::generate(&env);
    let vault_id = env.register_contract(None, Vault);
    let client = VaultClient::new(&env, &vault_id);

    let deposit_amount = 1000_i128;
    client.deposit(&caller, &deposit_amount);

    let auths = env.auths();
    assert_eq!(
        auths.len(),
        1,
        "Expected exactly one auth snapshot for deposit"
    );
    assert_eq!(
        auths[0].0, caller,
        "Caller address mismatch in auth snapshot"
    );
    assert_eq!(
        auths[0].1.function,
        Symbol::new(&env, "deposit"),
        "Function symbol mismatch in auth snapshot"
    );

    let withdraw_amount = 500_i128;
    client.withdraw(&caller, &withdraw_amount);

    let auths = env.auths();
    let latest_auth = auths.last().expect("Missing auth snapshot for withdraw");

    assert_eq!(
        latest_auth.0, caller,
        "Caller address mismatch in auth snapshot"
    );
    assert_eq!(
        latest_auth.1.function,
        Symbol::new(&env, "withdraw"),
        "Function symbol mismatch in auth snapshot"
    );
}

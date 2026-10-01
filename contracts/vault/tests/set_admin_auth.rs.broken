//! Explicit `require_auth` assertions for `CalloraVault::set_admin`.
//!
//! The existing `set_admin_unauthorized_fails` test in `src/test.rs` runs
//! under `env.mock_all_auths()` and therefore only exercises the in-body
//! identity check (`caller != cur => Err(Unauthorized)`). Removing
//! `caller.require_auth()` from `set_admin` would leave that test green,
//! because `mock_all_auths` silently approves any caller and the identity
//! check alone is enough to reject a non-admin signer.
//!
//! The tests below close that gap by asserting the Soroban auth framework
//! itself rejects unauthenticated invocations of `set_admin`.

use callora_vault::{CalloraVault, CalloraVaultClient};
use soroban_sdk::testutils::{Address as _, MockAuth, MockAuthInvoke};
use soroban_sdk::{Address, Env, IntoVal};

fn setup(env: &Env) -> (CalloraVaultClient<'_>, Address, Address) {
    let owner = Address::generate(env);
    let vault_addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(env, &vault_addr);
    let usdc = env
        .register_stellar_asset_contract_v2(owner.clone())
        .address();
    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    (client, owner, vault_addr)
}

/// With no mocked auths, `set_admin` must fail: `caller.require_auth()` is
/// the only line standing between an unauthenticated caller and the
/// `PendingAdmin` storage write.
#[test]
fn set_admin_fails_without_authorization() {
    let env = Env::default();
    let (client, owner, _vault) = setup(&env);
    let new_admin = Address::generate(&env);

    env.set_auths(&[]);

    let result = client.try_set_admin(&owner, &new_admin);
    assert!(
        result.is_err(),
        "set_admin must reject calls when caller.require_auth() has no signature"
    );
}

/// A successful `set_admin` call must record an auth entry against the
/// caller. If `caller.require_auth()` were removed, `env.auths()` would be
/// empty after the call and this assertion would fail.
#[test]
fn set_admin_records_caller_auth_entry() {
    let env = Env::default();
    let (client, owner, vault_addr) = setup(&env);
    let new_admin = Address::generate(&env);

    env.mock_auths(&[MockAuth {
        address: &owner,
        invoke: &MockAuthInvoke {
            contract: &vault_addr,
            fn_name: "set_admin",
            args: (owner.clone(), new_admin.clone()).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    client.set_admin(&owner, &new_admin);

    let auths = env.auths();
    assert!(
        auths.iter().any(|(signer, _)| signer == &owner),
        "expected require_auth entry for owner; got {auths:?}"
    );
}

/// Mocking auth for a different address than the declared `caller` must
/// still cause `set_admin` to fail. This proves `require_auth` checks the
/// `caller` parameter specifically, not "any authorized signer in scope".
#[test]
fn set_admin_fails_when_only_other_party_authorizes() {
    let env = Env::default();
    let (client, owner, vault_addr) = setup(&env);
    let intruder = Address::generate(&env);
    let new_admin = Address::generate(&env);

    env.mock_auths(&[MockAuth {
        address: &intruder,
        invoke: &MockAuthInvoke {
            contract: &vault_addr,
            fn_name: "set_admin",
            args: (owner.clone(), new_admin.clone()).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = client.try_set_admin(&owner, &new_admin);
    assert!(
        result.is_err(),
        "set_admin must reject when the authorized signer is not the declared caller"
    );
}

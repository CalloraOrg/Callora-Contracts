//! Cross-contract access-control matrix.
//!
//! For every privileged entrypoint covered below, the same table-driven
//! harness asserts three outcomes:
//!
//! 1. **No auth** — the correct role is passed but no authorization is
//!    installed; the call must be rejected by `require_auth`.
//! 2. **Wrong role** — an unrelated address is authenticated (or, for
//!    entrypoints without a `caller` parameter, only the unrelated address is
//!    mocked); the call must be rejected by the role check.
//! 3. **Right role** — the expected role is authenticated; the call must
//!    succeed and `env.auths()` must record that address.
//!
//! ## Deliberate skips
//!
//! * `vault::execute_upgrade` and `distribute::upgrade` /
//!   `revenue_pool::upgrade` swap the executing WASM and (for the revenue pool)
//!   run storage-migration validation. A successful right-role call needs a
//!   real uploaded WASM, which is outside this matrix; their `propose_*`
//!   counterparts are covered instead.
//! * Settlement interactions from the vault are asserted on the settlement
//!   side because the vault's settlement client is a no-op stub outside wasm
//!   builds (see `contracts/vault/src/test_idempotency.rs`).

#![cfg(test)]

extern crate std;

use std::format;
use std::string::String;
use std::vec::Vec;

use callora_distribute::{Distribute, DistributeClient};
use callora_revenue_pool::emergency::EMERGENCY_DRAIN_TIMELOCK_SECONDS;
use callora_revenue_pool::{RevenuePool, RevenuePoolClient, Severity};
use callora_settlement::{CalloraSettlement, CalloraSettlementClient};
use callora_vault::{CalloraVault, CalloraVaultClient};
use soroban_sdk::testutils::{Address as _, Ledger as _, MockAuth, MockAuthInvoke};
use soroban_sdk::token;
use soroban_sdk::{
    Address, BytesN, Env, IntoVal, String as SorobanString, Symbol, Val, Vec as SorobanVec,
};

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    NoAuth,
    WrongRole,
    RightRole,
}

struct Case {
    contract: &'static str,
    entrypoint: &'static str,
    run: fn(Role) -> bool,
}

fn run_matrix(cases: &[Case]) {
    let mut failures: Vec<String> = Vec::new();
    for case in cases {
        for (role, expect_success) in [
            (Role::NoAuth, false),
            (Role::WrongRole, false),
            (Role::RightRole, true),
        ] {
            let succeeded = (case.run)(role);
            if succeeded != expect_success {
                failures.push(format!(
                    "{}::{} as {:?}: expected {}, got {}",
                    case.contract,
                    case.entrypoint,
                    role,
                    if expect_success {
                        "success"
                    } else {
                        "rejection"
                    },
                    if succeeded { "success" } else { "rejection" },
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "access-control matrix failures:\n{}",
        failures.join("\n")
    );
}

/// Authenticate every address for the successful path; the wrong-role path for
/// `caller`-taking entrypoints passes an unrelated address as `caller`.
fn arm(env: &Env, role: Role) {
    match role {
        Role::NoAuth => env.mock_auths(&[]),
        Role::WrongRole | Role::RightRole => env.mock_all_auths(),
    }
}

fn outsider(env: &Env) -> Address {
    Address::generate(env)
}

fn assert_auth_recorded(env: &Env, expected: &Address, entrypoint: &str) {
    let auths = env.auths();
    assert!(
        !auths.is_empty(),
        "{entrypoint}: successful call recorded no authorization"
    );
    assert!(
        auths.iter().any(|(address, _)| *address == *expected),
        "{entrypoint}: expected authorization from the expected role"
    );
}

/// Mock authorization for exactly one address, used for entrypoints that take
/// no `caller` parameter and therefore need a targeted wrong-role auth.
fn with_mock_auth<T, A>(
    env: &Env,
    who: &Address,
    contract: &Address,
    fn_name: &'static str,
    args: A,
    call: impl FnOnce() -> T,
) -> T
where
    A: IntoVal<Env, SorobanVec<Val>>,
{
    let invoke = MockAuthInvoke {
        contract,
        fn_name,
        args: args.into_val(env),
        sub_invokes: &[],
    };
    env.mock_auths(&[MockAuth {
        address: who,
        invoke: &invoke,
    }]);
    call()
}

fn nonzero_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[1u8; 32])
}

// ---------------------------------------------------------------------------
// Vault
// ---------------------------------------------------------------------------

struct VaultCtx<'a> {
    addr: Address,
    client: CalloraVaultClient<'a>,
    owner: Address,
    usdc: Address,
}

fn setup_vault<'a>(env: &'a Env) -> VaultCtx<'a> {
    env.mock_all_auths();
    let owner = Address::generate(env);
    let sac_contract = env.register_stellar_asset_contract_v2(owner.clone());
    let usdc = sac_contract.address();
    let sac = token::StellarAssetClient::new(env, &usdc);
    let addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(env, &addr);
    sac.mint(&addr, &10_000i128);
    client.init(
        &owner,
        &usdc,
        &Some(1_000i128),
        &None,
        &Some(1i128),
        &None,
        &Some(10_000i128),
        &None,
    );
    VaultCtx {
        addr,
        client,
        owner,
        usdc,
    }
}

fn caller_for(env: &Env, role: Role, expected: &Address) -> Address {
    match role {
        Role::WrongRole => outsider(env),
        Role::NoAuth | Role::RightRole => expected.clone(),
    }
}

fn vault_owner_entrypoint(
    role: Role,
    entrypoint: &'static str,
    call: impl FnOnce(&VaultCtx, &Address) -> bool,
) -> bool {
    let env = Env::default();
    let ctx = setup_vault(&env);
    let caller = caller_for(&env, role, &ctx.owner);
    arm(&env, role);
    let succeeded = call(&ctx, &caller);
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.owner, entrypoint);
    }
    succeeded
}

fn vault_pause(role: Role) -> bool {
    vault_owner_entrypoint(role, "vault::pause", |ctx, caller| {
        ctx.client.try_pause(caller).is_ok()
    })
}

fn vault_unpause(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_vault(&env);
    ctx.client.pause(&ctx.owner);
    let caller = caller_for(&env, role, &ctx.owner);
    arm(&env, role);
    let succeeded = ctx.client.try_unpause(&caller).is_ok();
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.owner, "vault::unpause");
    }
    succeeded
}

fn vault_set_max_deduct(role: Role) -> bool {
    vault_owner_entrypoint(role, "vault::set_max_deduct", |ctx, caller| {
        matches!(ctx.client.try_set_max_deduct(caller, &2_000i128), Ok(Ok(_)))
    })
}

fn vault_set_settlement(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_vault(&env);
    let settlement = outsider(&env);
    let caller = caller_for(&env, role, &ctx.owner);
    arm(&env, role);
    let succeeded = matches!(
        ctx.client.try_set_settlement(&caller, &settlement),
        Ok(Ok(_))
    );
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.owner, "vault::set_settlement");
    }
    succeeded
}

fn vault_set_authorized_caller(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_vault(&env);
    let new_caller = outsider(&env);
    let call = || {
        matches!(
            ctx.client
                .try_set_authorized_caller(&Some(new_caller.clone()), &0u64),
            Ok(Ok(_))
        )
    };
    let succeeded = match role {
        Role::NoAuth => {
            env.mock_auths(&[]);
            call()
        }
        Role::WrongRole => with_mock_auth(
            &env,
            &outsider(&env),
            &ctx.addr,
            "set_authorized_caller",
            (),
            call,
        ),
        Role::RightRole => {
            env.mock_all_auths();
            call()
        }
    };
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.owner, "vault::set_authorized_caller");
    }
    succeeded
}

fn vault_set_admin(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_vault(&env);
    let new_admin = outsider(&env);
    let caller = caller_for(&env, role, &ctx.owner);
    arm(&env, role);
    let succeeded = matches!(ctx.client.try_set_admin(&caller, &new_admin), Ok(Ok(_)));
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.owner, "vault::set_admin");
    }
    succeeded
}

fn vault_accept_admin(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_vault(&env);
    let pending = outsider(&env);
    ctx.client.set_admin(&ctx.owner, &pending);
    let call = || ctx.client.try_accept_admin().is_ok();
    let succeeded = match role {
        Role::NoAuth => {
            env.mock_auths(&[]);
            call()
        }
        Role::WrongRole => {
            with_mock_auth(&env, &outsider(&env), &ctx.addr, "accept_admin", (), call)
        }
        Role::RightRole => {
            env.mock_all_auths();
            call()
        }
    };
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &pending, "vault::accept_admin");
    }
    succeeded
}

fn vault_transfer_ownership(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_vault(&env);
    let new_owner = outsider(&env);
    let caller = caller_for(&env, role, &ctx.owner);
    arm(&env, role);
    let succeeded = matches!(
        ctx.client.try_transfer_ownership(&caller, &new_owner),
        Ok(Ok(_))
    );
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.owner, "vault::transfer_ownership");
    }
    succeeded
}

fn vault_accept_ownership(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_vault(&env);
    let pending = outsider(&env);
    ctx.client.transfer_ownership(&ctx.owner, &pending);
    let call = || ctx.client.try_accept_ownership().is_ok();
    let succeeded = match role {
        Role::NoAuth => {
            env.mock_auths(&[]);
            call()
        }
        Role::WrongRole => with_mock_auth(
            &env,
            &outsider(&env),
            &ctx.addr,
            "accept_ownership",
            (),
            call,
        ),
        Role::RightRole => {
            env.mock_all_auths();
            call()
        }
    };
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &pending, "vault::accept_ownership");
    }
    succeeded
}

fn vault_propose_pause(role: Role) -> bool {
    vault_owner_entrypoint(role, "vault::propose_pause", |ctx, caller| {
        matches!(ctx.client.try_propose_pause(caller), Ok(Ok(_)))
    })
}

fn vault_execute_pause(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_vault(&env);
    ctx.client.propose_pause(&ctx.owner);
    env.ledger().set_timestamp(172_801);
    let caller = caller_for(&env, role, &ctx.owner);
    arm(&env, role);
    let succeeded = matches!(ctx.client.try_execute_pause(&caller), Ok(Ok(_)));
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.owner, "vault::execute_pause");
    }
    succeeded
}

fn vault_cancel_pause(role: Role) -> bool {
    vault_owner_entrypoint(role, "vault::cancel_pause", |ctx, caller| {
        matches!(ctx.client.try_cancel_pause(caller), Ok(Ok(_)))
    })
}

fn vault_propose_upgrade(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_vault(&env);
    let hash = nonzero_hash(&env);
    let caller = caller_for(&env, role, &ctx.owner);
    arm(&env, role);
    let succeeded = matches!(ctx.client.try_propose_upgrade(&caller, &hash), Ok(Ok(_)));
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.owner, "vault::propose_upgrade");
    }
    succeeded
}

fn vault_add_address(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_vault(&env);
    let depositor = outsider(&env);
    let caller = caller_for(&env, role, &ctx.owner);
    arm(&env, role);
    let succeeded = matches!(ctx.client.try_add_address(&caller, &depositor), Ok(Ok(_)));
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.owner, "vault::add_address");
    }
    succeeded
}

fn vault_remove_address(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_vault(&env);
    let depositor = outsider(&env);
    let caller = caller_for(&env, role, &ctx.owner);
    arm(&env, role);
    let succeeded = matches!(
        ctx.client.try_remove_address(&caller, &depositor),
        Ok(Ok(_))
    );
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.owner, "vault::remove_address");
    }
    succeeded
}

fn vault_clear_all(role: Role) -> bool {
    vault_owner_entrypoint(role, "vault::clear_all", |ctx, caller| {
        matches!(ctx.client.try_clear_all(caller), Ok(Ok(_)))
    })
}

fn vault_admin_rescue(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_vault(&env);
    let recipient = outsider(&env);
    let caller = caller_for(&env, role, &ctx.owner);
    arm(&env, role);
    let succeeded = matches!(
        ctx.client
            .try_admin_rescue(&caller, &ctx.usdc, &recipient, &100i128),
        Ok(Ok(_))
    );
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.owner, "vault::admin_rescue");
    }
    succeeded
}

fn vault_set_reserve_cap(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_vault(&env);
    let caller = caller_for(&env, role, &ctx.owner);
    arm(&env, role);
    let succeeded = matches!(
        ctx.client
            .try_set_reserve_cap(&caller, &ctx.usdc, &5_000i128),
        Ok(Ok(_))
    );
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.owner, "vault::set_reserve_cap");
    }
    succeeded
}

fn vault_withdraw(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_vault(&env);
    let call = || matches!(ctx.client.try_withdraw(&100i128), Ok(Ok(_)));
    let succeeded = match role {
        Role::NoAuth => {
            env.mock_auths(&[]);
            call()
        }
        Role::WrongRole => with_mock_auth(
            &env,
            &outsider(&env),
            &ctx.addr,
            "withdraw",
            (100i128,),
            call,
        ),
        Role::RightRole => {
            env.mock_all_auths();
            call()
        }
    };
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.owner, "vault::withdraw");
    }
    succeeded
}

fn vault_withdraw_to(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_vault(&env);
    let recipient = outsider(&env);
    let call = || matches!(ctx.client.try_withdraw_to(&recipient, &100i128), Ok(Ok(_)));
    let succeeded = match role {
        Role::NoAuth => {
            env.mock_auths(&[]);
            call()
        }
        Role::WrongRole => with_mock_auth(
            &env,
            &outsider(&env),
            &ctx.addr,
            "withdraw_to",
            (recipient.clone(), 100i128),
            call,
        ),
        Role::RightRole => {
            env.mock_all_auths();
            call()
        }
    };
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.owner, "vault::withdraw_to");
    }
    succeeded
}

fn vault_distribute(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_vault(&env);
    let recipient = outsider(&env);
    let caller = caller_for(&env, role, &ctx.owner);
    arm(&env, role);
    let succeeded = ctx
        .client
        .try_distribute(&caller, &recipient, &100i128)
        .is_ok();
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.owner, "vault::distribute");
    }
    succeeded
}

#[test]
fn vault_privileged_entrypoints() {
    let cases = [
        Case {
            contract: "vault",
            entrypoint: "pause",
            run: vault_pause,
        },
        Case {
            contract: "vault",
            entrypoint: "unpause",
            run: vault_unpause,
        },
        Case {
            contract: "vault",
            entrypoint: "set_max_deduct",
            run: vault_set_max_deduct,
        },
        Case {
            contract: "vault",
            entrypoint: "set_settlement",
            run: vault_set_settlement,
        },
        Case {
            contract: "vault",
            entrypoint: "set_authorized_caller",
            run: vault_set_authorized_caller,
        },
        Case {
            contract: "vault",
            entrypoint: "set_admin",
            run: vault_set_admin,
        },
        Case {
            contract: "vault",
            entrypoint: "accept_admin",
            run: vault_accept_admin,
        },
        Case {
            contract: "vault",
            entrypoint: "transfer_ownership",
            run: vault_transfer_ownership,
        },
        Case {
            contract: "vault",
            entrypoint: "accept_ownership",
            run: vault_accept_ownership,
        },
        Case {
            contract: "vault",
            entrypoint: "propose_pause",
            run: vault_propose_pause,
        },
        Case {
            contract: "vault",
            entrypoint: "execute_pause",
            run: vault_execute_pause,
        },
        Case {
            contract: "vault",
            entrypoint: "cancel_pause",
            run: vault_cancel_pause,
        },
        Case {
            contract: "vault",
            entrypoint: "propose_upgrade",
            run: vault_propose_upgrade,
        },
        Case {
            contract: "vault",
            entrypoint: "add_address",
            run: vault_add_address,
        },
        Case {
            contract: "vault",
            entrypoint: "remove_address",
            run: vault_remove_address,
        },
        Case {
            contract: "vault",
            entrypoint: "clear_all",
            run: vault_clear_all,
        },
        Case {
            contract: "vault",
            entrypoint: "admin_rescue",
            run: vault_admin_rescue,
        },
        Case {
            contract: "vault",
            entrypoint: "set_reserve_cap",
            run: vault_set_reserve_cap,
        },
        Case {
            contract: "vault",
            entrypoint: "withdraw",
            run: vault_withdraw,
        },
        Case {
            contract: "vault",
            entrypoint: "withdraw_to",
            run: vault_withdraw_to,
        },
        Case {
            contract: "vault",
            entrypoint: "distribute",
            run: vault_distribute,
        },
    ];
    run_matrix(&cases);
}

// ---------------------------------------------------------------------------
// Settlement
// ---------------------------------------------------------------------------

struct SettlementCtx<'a> {
    client: CalloraSettlementClient<'a>,
    admin: Address,
}

fn setup_settlement<'a>(env: &'a Env) -> SettlementCtx<'a> {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let vault = Address::generate(env);
    let addr = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(env, &addr);
    client.init(&admin, &vault);
    SettlementCtx { client, admin }
}

fn settlement_set_usdc_token(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_settlement(&env);
    let token = outsider(&env);
    let caller = caller_for(&env, role, &ctx.admin);
    arm(&env, role);
    let succeeded = ctx.client.try_set_usdc_token(&caller, &token).is_ok();
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.admin, "settlement::set_usdc_token");
    }
    succeeded
}

fn settlement_add_supported_token(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_settlement(&env);
    let token = outsider(&env);
    let caller = caller_for(&env, role, &ctx.admin);
    arm(&env, role);
    let succeeded = ctx.client.try_add_supported_token(&caller, &token).is_ok();
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.admin, "settlement::add_supported_token");
    }
    succeeded
}

fn settlement_set_daily_withdraw_cap(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_settlement(&env);
    let developer = outsider(&env);
    let caller = caller_for(&env, role, &ctx.admin);
    arm(&env, role);
    let succeeded = matches!(
        ctx.client
            .try_set_daily_withdraw_cap(&caller, &developer, &1_000i128),
        Ok(Ok(_))
    );
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.admin, "settlement::set_daily_withdraw_cap");
    }
    succeeded
}

fn settlement_force_credit_developer(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_settlement(&env);
    let developer = outsider(&env);
    let token = outsider(&env);
    let caller = caller_for(&env, role, &ctx.admin);
    arm(&env, role);
    let succeeded = ctx
        .client
        .try_force_credit_developer(
            &caller,
            &developer,
            &100i128,
            &token,
            &Symbol::new(&env, "matrix"),
        )
        .is_ok();
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.admin, "settlement::force_credit_developer");
    }
    succeeded
}

fn settlement_set_admin(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_settlement(&env);
    let new_admin = outsider(&env);
    let caller = caller_for(&env, role, &ctx.admin);
    arm(&env, role);
    let succeeded = ctx.client.try_set_admin(&caller, &new_admin).is_ok();
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.admin, "settlement::set_admin");
    }
    succeeded
}

fn settlement_propose_vault(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_settlement(&env);
    let new_vault = outsider(&env);
    let caller = caller_for(&env, role, &ctx.admin);
    arm(&env, role);
    let succeeded = ctx.client.try_propose_vault(&caller, &new_vault).is_ok();
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.admin, "settlement::propose_vault");
    }
    succeeded
}

fn settlement_accept_vault(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_settlement(&env);
    let pending = outsider(&env);
    ctx.client.propose_vault(&ctx.admin, &pending);
    let caller = caller_for(&env, role, &pending);
    arm(&env, role);
    let succeeded = ctx.client.try_accept_vault(&caller).is_ok();
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &pending, "settlement::accept_vault");
    }
    succeeded
}

fn settlement_freeze_developer(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_settlement(&env);
    let developer = outsider(&env);
    let caller = caller_for(&env, role, &ctx.admin);
    arm(&env, role);
    let succeeded = matches!(
        ctx.client
            .try_freeze_developer(&caller, &developer, &Symbol::new(&env, "matrix")),
        Ok(Ok(_))
    );
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.admin, "settlement::freeze_developer");
    }
    succeeded
}

fn settlement_set_price(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_settlement(&env);
    let offering = SorobanString::from_str(&env, "offering-1");
    let price = SorobanString::from_str(&env, "10.00");
    let caller = caller_for(&env, role, &ctx.admin);
    arm(&env, role);
    let succeeded = ctx.client.try_set_price(&caller, &offering, &price).is_ok();
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.admin, "settlement::set_price");
    }
    succeeded
}

fn settlement_propose_upgrade(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_settlement(&env);
    let hash = nonzero_hash(&env);
    let caller = caller_for(&env, role, &ctx.admin);
    arm(&env, role);
    let succeeded = ctx.client.try_propose_upgrade(&caller, &hash).is_ok();
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.admin, "settlement::propose_upgrade");
    }
    succeeded
}

#[test]
fn settlement_privileged_entrypoints() {
    let cases = [
        Case {
            contract: "settlement",
            entrypoint: "set_usdc_token",
            run: settlement_set_usdc_token,
        },
        Case {
            contract: "settlement",
            entrypoint: "add_supported_token",
            run: settlement_add_supported_token,
        },
        Case {
            contract: "settlement",
            entrypoint: "set_daily_withdraw_cap",
            run: settlement_set_daily_withdraw_cap,
        },
        Case {
            contract: "settlement",
            entrypoint: "force_credit_developer",
            run: settlement_force_credit_developer,
        },
        Case {
            contract: "settlement",
            entrypoint: "set_admin",
            run: settlement_set_admin,
        },
        Case {
            contract: "settlement",
            entrypoint: "propose_vault",
            run: settlement_propose_vault,
        },
        Case {
            contract: "settlement",
            entrypoint: "accept_vault",
            run: settlement_accept_vault,
        },
        Case {
            contract: "settlement",
            entrypoint: "freeze_developer",
            run: settlement_freeze_developer,
        },
        Case {
            contract: "settlement",
            entrypoint: "set_price",
            run: settlement_set_price,
        },
        Case {
            contract: "settlement",
            entrypoint: "propose_upgrade",
            run: settlement_propose_upgrade,
        },
    ];
    run_matrix(&cases);
}

// ---------------------------------------------------------------------------
// Revenue pool
// ---------------------------------------------------------------------------

struct PoolCtx<'a> {
    client: RevenuePoolClient<'a>,
    admin: Address,
}

fn setup_pool<'a>(env: &'a Env) -> PoolCtx<'a> {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let sac_contract = env.register_stellar_asset_contract_v2(admin.clone());
    let usdc = sac_contract.address();
    let sac = token::StellarAssetClient::new(env, &usdc);
    let addr = env.register(RevenuePool, ());
    let client = RevenuePoolClient::new(env, &addr);
    client.init(&admin, &usdc);
    sac.mint(&addr, &100_000i128);
    PoolCtx { client, admin }
}

fn pool_admin_entrypoint(
    role: Role,
    entrypoint: &'static str,
    call: impl FnOnce(&PoolCtx, &Address) -> bool,
) -> bool {
    let env = Env::default();
    let ctx = setup_pool(&env);
    let caller = caller_for(&env, role, &ctx.admin);
    arm(&env, role);
    let succeeded = call(&ctx, &caller);
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.admin, entrypoint);
    }
    succeeded
}

fn pool_set_max_distribute(role: Role) -> bool {
    pool_admin_entrypoint(role, "revenue_pool::set_max_distribute", |ctx, caller| {
        ctx.client
            .try_set_max_distribute(caller, &1_000i128)
            .is_ok()
    })
}

fn pool_distribute(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_pool(&env);
    let recipient = outsider(&env);
    let caller = caller_for(&env, role, &ctx.admin);
    arm(&env, role);
    let succeeded = ctx
        .client
        .try_distribute(&caller, &recipient, &100i128)
        .is_ok();
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.admin, "revenue_pool::distribute");
    }
    succeeded
}

fn pool_batch_distribute(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_pool(&env);
    let recipient = outsider(&env);
    let payments = soroban_sdk::vec![&env, (recipient, 100i128)];
    let caller = caller_for(&env, role, &ctx.admin);
    arm(&env, role);
    let succeeded = matches!(
        ctx.client.try_batch_distribute(&caller, &payments),
        Ok(Ok(_))
    );
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.admin, "revenue_pool::batch_distribute");
    }
    succeeded
}

fn pool_pause(role: Role) -> bool {
    pool_admin_entrypoint(role, "revenue_pool::pause", |ctx, caller| {
        ctx.client.try_pause(caller).is_ok()
    })
}

fn pool_set_pause_guardian(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_pool(&env);
    let guardian = outsider(&env);
    let caller = caller_for(&env, role, &ctx.admin);
    arm(&env, role);
    let succeeded = ctx
        .client
        .try_set_pause_guardian(&caller, &guardian)
        .is_ok();
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.admin, "revenue_pool::set_pause_guardian");
    }
    succeeded
}

fn pool_emergency_pause(role: Role) -> bool {
    pool_admin_entrypoint(role, "revenue_pool::emergency_pause", |ctx, caller| {
        ctx.client.try_emergency_pause(caller).is_ok()
    })
}

fn pool_propose_emergency_drain(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_pool(&env);
    let treasury = outsider(&env);
    let caller = caller_for(&env, role, &ctx.admin);
    arm(&env, role);
    let succeeded = ctx
        .client
        .try_propose_emergency_drain(&caller, &treasury, &100i128)
        .is_ok();
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.admin, "revenue_pool::propose_emergency_drain");
    }
    succeeded
}

fn pool_execute_emergency_drain(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_pool(&env);
    let treasury = outsider(&env);
    ctx.client
        .propose_emergency_drain(&ctx.admin, &treasury, &100i128);
    env.ledger()
        .set_timestamp(EMERGENCY_DRAIN_TIMELOCK_SECONDS + 1);
    let caller = caller_for(&env, role, &ctx.admin);
    arm(&env, role);
    let succeeded = ctx.client.try_execute_emergency_drain(&caller).is_ok();
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.admin, "revenue_pool::execute_emergency_drain");
    }
    succeeded
}

fn pool_cancel_emergency_drain(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_pool(&env);
    let treasury = outsider(&env);
    ctx.client
        .propose_emergency_drain(&ctx.admin, &treasury, &100i128);
    let caller = caller_for(&env, role, &ctx.admin);
    arm(&env, role);
    let succeeded = ctx.client.try_cancel_emergency_drain(&caller).is_ok();
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.admin, "revenue_pool::cancel_emergency_drain");
    }
    succeeded
}

fn pool_broadcast(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_pool(&env);
    let message = SorobanString::from_str(&env, "maintenance window");
    let caller = caller_for(&env, role, &ctx.admin);
    arm(&env, role);
    let succeeded = ctx
        .client
        .try_broadcast(&caller, &Severity::Info, &message)
        .is_ok();
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.admin, "revenue_pool::broadcast");
    }
    succeeded
}

#[test]
fn revenue_pool_privileged_entrypoints() {
    let cases = [
        Case {
            contract: "revenue_pool",
            entrypoint: "set_max_distribute",
            run: pool_set_max_distribute,
        },
        Case {
            contract: "revenue_pool",
            entrypoint: "distribute",
            run: pool_distribute,
        },
        Case {
            contract: "revenue_pool",
            entrypoint: "batch_distribute",
            run: pool_batch_distribute,
        },
        Case {
            contract: "revenue_pool",
            entrypoint: "pause",
            run: pool_pause,
        },
        Case {
            contract: "revenue_pool",
            entrypoint: "set_pause_guardian",
            run: pool_set_pause_guardian,
        },
        Case {
            contract: "revenue_pool",
            entrypoint: "emergency_pause",
            run: pool_emergency_pause,
        },
        Case {
            contract: "revenue_pool",
            entrypoint: "propose_emergency_drain",
            run: pool_propose_emergency_drain,
        },
        Case {
            contract: "revenue_pool",
            entrypoint: "execute_emergency_drain",
            run: pool_execute_emergency_drain,
        },
        Case {
            contract: "revenue_pool",
            entrypoint: "cancel_emergency_drain",
            run: pool_cancel_emergency_drain,
        },
        Case {
            contract: "revenue_pool",
            entrypoint: "broadcast",
            run: pool_broadcast,
        },
    ];
    run_matrix(&cases);
}

#[test]
fn revenue_pool_guardian_can_pause_but_not_unpause() {
    let env = Env::default();
    let ctx = setup_pool(&env);
    let guardian = outsider(&env);
    ctx.client.set_pause_guardian(&ctx.admin, &guardian);

    env.mock_all_auths();
    ctx.client.pause(&guardian);
    assert!(ctx.client.is_paused());

    let result = ctx.client.try_unpause(&guardian);
    assert!(
        result.is_err(),
        "the pause guardian must not be able to unpause the pool"
    );
}

// ---------------------------------------------------------------------------
// Distribute
// ---------------------------------------------------------------------------

struct DistributeCtx<'a> {
    client: DistributeClient<'a>,
    admin: Address,
}

fn setup_distribute<'a>(env: &'a Env) -> DistributeCtx<'a> {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let sac_contract = env.register_stellar_asset_contract_v2(admin.clone());
    let usdc = sac_contract.address();
    let sac = token::StellarAssetClient::new(env, &usdc);
    let addr = env.register(Distribute, ());
    let client = DistributeClient::new(env, &addr);
    client.init(&admin, &usdc);
    sac.mint(&addr, &100_000i128);
    DistributeCtx { client, admin }
}

fn distribute_admin_entrypoint(
    role: Role,
    entrypoint: &'static str,
    call: impl FnOnce(&DistributeCtx, &Address) -> bool,
) -> bool {
    let env = Env::default();
    let ctx = setup_distribute(&env);
    let caller = caller_for(&env, role, &ctx.admin);
    arm(&env, role);
    let succeeded = call(&ctx, &caller);
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.admin, entrypoint);
    }
    succeeded
}

fn distribute_set_admin(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_distribute(&env);
    let new_admin = outsider(&env);
    let caller = caller_for(&env, role, &ctx.admin);
    arm(&env, role);
    let succeeded = ctx.client.try_set_admin(&caller, &new_admin).is_ok();
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.admin, "distribute::set_admin");
    }
    succeeded
}

fn distribute_pause(role: Role) -> bool {
    distribute_admin_entrypoint(role, "distribute::pause", |ctx, caller| {
        ctx.client.try_pause(caller).is_ok()
    })
}

fn distribute_set_max_distribute(role: Role) -> bool {
    distribute_admin_entrypoint(role, "distribute::set_max_distribute", |ctx, caller| {
        ctx.client
            .try_set_max_distribute(caller, &1_000i128)
            .is_ok()
    })
}

fn distribute_distribute(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_distribute(&env);
    let recipient = outsider(&env);
    let caller = caller_for(&env, role, &ctx.admin);
    arm(&env, role);
    let succeeded = ctx
        .client
        .try_distribute(&caller, &recipient, &100i128)
        .is_ok();
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.admin, "distribute::distribute");
    }
    succeeded
}

fn distribute_batch_distribute(role: Role) -> bool {
    let env = Env::default();
    let ctx = setup_distribute(&env);
    let recipient = outsider(&env);
    let payments = soroban_sdk::vec![&env, (recipient, 100i128)];
    let caller = caller_for(&env, role, &ctx.admin);
    arm(&env, role);
    let succeeded = ctx.client.try_batch_distribute(&caller, &payments).is_ok();
    if role == Role::RightRole && succeeded {
        assert_auth_recorded(&env, &ctx.admin, "distribute::batch_distribute");
    }
    succeeded
}

#[test]
fn distribute_privileged_entrypoints() {
    let cases = [
        Case {
            contract: "distribute",
            entrypoint: "set_admin",
            run: distribute_set_admin,
        },
        Case {
            contract: "distribute",
            entrypoint: "pause",
            run: distribute_pause,
        },
        Case {
            contract: "distribute",
            entrypoint: "set_max_distribute",
            run: distribute_set_max_distribute,
        },
        Case {
            contract: "distribute",
            entrypoint: "distribute",
            run: distribute_distribute,
        },
        Case {
            contract: "distribute",
            entrypoint: "batch_distribute",
            run: distribute_batch_distribute,
        },
    ];
    run_matrix(&cases);
}

// ---------------------------------------------------------------------------
// Cross-cutting edge cases
// ---------------------------------------------------------------------------

/// A nominated but not-yet-accepted admin must not be able to use admin
/// entrypoints.
#[test]
fn pending_settlement_admin_cannot_act_before_acceptance() {
    let env = Env::default();
    let ctx = setup_settlement(&env);
    let pending = outsider(&env);
    let developer = outsider(&env);
    ctx.client.set_admin(&ctx.admin, &pending);

    env.mock_all_auths();
    let result = ctx
        .client
        .try_set_daily_withdraw_cap(&pending, &developer, &1_000i128);
    assert!(
        result.is_err(),
        "a pending admin must not act before accepting the role"
    );
}

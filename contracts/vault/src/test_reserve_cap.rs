extern crate std;

use soroban_sdk::testutils::{Address as _, Events as _, MockAuth, MockAuthInvoke};
use soroban_sdk::{token, Address, Env, Error, IntoVal, InvokeError};

use super::*;
use callora_settlement::CalloraSettlement;

type Setup<'a> = (
    Address,
    CalloraVaultClient<'a>,
    Address,
    Address,
    token::Client<'a>,
    token::StellarAssetClient<'a>,
    Address,
);

fn setup(env: &Env, tracked_balance: i128) -> Setup<'_> {
    env.mock_all_auths_allowing_non_root_auth();
    let owner = Address::generate(env);
    let usdc = env
        .register_stellar_asset_contract_v2(owner.clone())
        .address();
    let usdc_client = token::Client::new(env, &usdc);
    let usdc_admin = token::StellarAssetClient::new(env, &usdc);
    let vault_address = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(env, &vault_address);
    let settlement_address = env.register(CalloraSettlement, ());
    let settlement_client =
        callora_settlement::CalloraSettlementClient::new(env, &settlement_address);
    settlement_client.init(&owner, &vault_address);

    if tracked_balance > 0 {
        usdc_admin.mint(&vault_address, &tracked_balance);
    }
    client.init(
        &owner,
        &usdc,
        &Some(tracked_balance),
        &Some(owner.clone()),
        &Some(1),
        &None,
        &Some(i128::MAX),
        &Some(settlement_address.clone()),
    );

    (
        owner,
        client,
        usdc,
        vault_address,
        usdc_client,
        usdc_admin,
        settlement_address,
    )
}

fn prepare_deposit(
    owner: &Address,
    vault_address: &Address,
    usdc_client: &token::Client<'_>,
    usdc_admin: &token::StellarAssetClient<'_>,
    amount: i128,
) {
    usdc_admin.mint(owner, &amount);
    usdc_client.approve(owner, vault_address, &amount, &10_000);
}

fn expect_contract_error<
    V: core::fmt::Debug,
    CE: Into<Error> + core::fmt::Debug,
    E: Into<Error> + core::fmt::Debug + Copy,
>(
    result: Result<Result<V, CE>, Result<E, InvokeError>>,
    expected_code: u32,
) {
    let actual_code = match &result {
        Err(Ok(error)) => Some((*error).into().get_code()),
        _ => None,
    };
    assert_eq!(
        actual_code,
        Some(expected_code),
        "unexpected result: {result:?}"
    );
}

#[test]
fn deposit_succeeds_with_unset_cap() {
    let env = Env::default();
    let (owner, client, _, vault_address, usdc, usdc_admin, _) = setup(&env, 900);
    prepare_deposit(&owner, &vault_address, &usdc, &usdc_admin, 1_000_000);

    client.deposit(&owner, &1_000_000);

    assert_eq!(client.balance(), 1_000_900);
}

#[test]
fn deposit_succeeds_at_inclusive_cap_boundary() {
    let env = Env::default();
    let (owner, client, usdc_token, vault_address, usdc, usdc_admin, _) = setup(&env, 900);
    client.set_reserve_cap(&owner, &usdc_token, &1_000);
    prepare_deposit(&owner, &vault_address, &usdc, &usdc_admin, 100);

    client.deposit(&owner, &100);

    assert_eq!(client.balance(), 1_000);
}

#[test]
fn deposit_over_cap_is_rejected_without_mutating_anything_or_emitting_deposit() {
    let env = Env::default();
    let (owner, client, usdc_token, vault_address, usdc, usdc_admin, _) = setup(&env, 900);
    client.set_reserve_cap(&owner, &usdc_token, &1_000);
    prepare_deposit(&owner, &vault_address, &usdc, &usdc_admin, 101);

    let tracked_before = client.balance();
    let sender_before = usdc.balance(&owner);
    let vault_before = usdc.balance(&vault_address);
    let result = client.try_deposit(&owner, &101);

    let rejected_events = env.events().all();
    expect_contract_error(result, VaultError::ExceedsReserveCap as u32);
    assert_eq!(client.balance(), tracked_before);
    assert_eq!(usdc.balance(&owner), sender_before);
    assert_eq!(usdc.balance(&vault_address), vault_before);
    assert!(rejected_events.is_empty());
}

#[test]
fn explicitly_unlimited_cap_accepts_deposit() {
    let env = Env::default();
    let (owner, client, usdc_token, vault_address, usdc, usdc_admin, _) = setup(&env, 900);
    client.set_reserve_cap(&owner, &usdc_token, &i128::MAX);
    prepare_deposit(&owner, &vault_address, &usdc, &usdc_admin, 1_000_000);

    client.deposit(&owner, &1_000_000);

    assert_eq!(client.balance(), 1_000_900);
}

#[test]
fn lowering_cap_rejects_deposits_but_withdraw_and_deduct_remain_available() {
    let env = Env::default();
    let (owner, client, usdc_token, vault_address, usdc, usdc_admin, settlement) = setup(&env, 900);
    client.set_reserve_cap(&owner, &usdc_token, &800);
    prepare_deposit(&owner, &vault_address, &usdc, &usdc_admin, 1);

    expect_contract_error(
        client.try_deposit(&owner, &1),
        VaultError::ExceedsReserveCap as u32,
    );
    assert_eq!(client.balance(), 900);

    client.withdraw(&50);
    assert_eq!(client.balance(), 850);
    client.deduct(&owner, &100, &0);

    assert_eq!(client.balance(), 750);
    client.deposit(&owner, &1);
    assert_eq!(client.balance(), 751);
    assert_eq!(usdc.balance(&settlement), 100);
}

#[test]
fn set_reserve_cap_rejects_non_usdc_token_without_state_change() {
    let env = Env::default();
    let (owner, client, usdc_token, _, _, _, _) = setup(&env, 0);
    client.set_reserve_cap(&owner, &usdc_token, &500);
    let unsupported_token = Address::generate(&env);

    expect_contract_error(
        client.try_set_reserve_cap(&owner, &unsupported_token, &700),
        ReserveCapError::CapTokenNotUsdc as u32,
    );

    assert!(env.events().all().is_empty());
    assert_eq!(client.get_reserve_cap(&usdc_token), 500);
    assert_eq!(client.get_reserve_cap(&unsupported_token), i128::MAX);
}

#[test]
fn auth_is_checked_before_pause_and_amount_validation() {
    let env = Env::default();
    let (owner, client, _, vault_address, _, _, _) = setup(&env, 900);
    client.set_reserve_cap(&owner, &client.get_usdc_token(), &1_000);
    env.as_contract(&vault_address, || {
        env.storage().instance().set(&DataKey::Paused, &true);
    });

    env.set_auths(&[]);
    let result = client.try_deposit(&owner, &0);

    assert!(
        matches!(result, Err(Err(_))),
        "missing caller authorization must fail before pause or amount checks: {result:?}"
    );
}

#[test]
fn paused_vault_is_rejected_before_invalid_amount_and_cap() {
    let env = Env::default();
    let (owner, client, usdc_token, vault_address, usdc, usdc_admin, _) = setup(&env, 900);
    client.set_reserve_cap(&owner, &usdc_token, &1_000);
    prepare_deposit(&owner, &vault_address, &usdc, &usdc_admin, 101);
    env.as_contract(&vault_address, || {
        env.storage().instance().set(&DataKey::Paused, &true);
    });

    expect_contract_error(client.try_deposit(&owner, &0), VaultError::Paused as u32);
    expect_contract_error(client.try_deposit(&owner, &101), VaultError::Paused as u32);
}

#[test]
fn invalid_amount_is_rejected_before_allowlist_and_cap() {
    let env = Env::default();
    let (owner, client, usdc_token, vault_address, _, _, _) = setup(&env, 900);
    client.set_reserve_cap(&owner, &usdc_token, &1_000);
    env.as_contract(&vault_address, || {
        env.storage().instance().set(&DataKey::MinDeposit, &10_i128);
    });
    let stranger = Address::generate(&env);
    expect_contract_error(
        client.try_deposit(&stranger, &1),
        VaultError::BelowMinDeposit as u32,
    );
    expect_contract_error(
        client.try_deposit(&stranger, &0),
        VaultError::AmountNotPositive as u32,
    );
}

#[test]
fn allowlist_error_precedes_reserve_cap_error() {
    let env = Env::default();
    let (owner, client, usdc_token, vault_address, usdc, usdc_admin, _) = setup(&env, 900);
    client.set_reserve_cap(&owner, &usdc_token, &1_000);
    let stranger = Address::generate(&env);
    prepare_deposit(&stranger, &vault_address, &usdc, &usdc_admin, 101);

    expect_contract_error(
        client.try_deposit(&stranger, &101),
        VaultError::CallerNotInAllowlist as u32,
    );
}

fn force_tracked_balance_to_max(
    env: &Env,
    vault_address: &Address,
    client: &CalloraVaultClient<'_>,
    usdc: &token::Client<'_>,
    usdc_admin: &token::StellarAssetClient<'_>,
    owner: &Address,
) {
    // Deliberately inconsistent ledger state isolates i128 overflow in deposit
    // without requiring the token contract to hold i128::MAX tokens.
    env.as_contract(vault_address, || {
        env.storage().instance().set(&DataKey::Balance, &i128::MAX);
    });
    prepare_deposit(owner, vault_address, usdc, usdc_admin, 1);
    assert_eq!(client.balance(), i128::MAX);
}

#[test]
fn deposit_overflow_with_finite_cap_returns_overflow() {
    let env = Env::default();
    let (owner, client, usdc_token, vault_address, usdc, usdc_admin, _) = setup(&env, 0);
    client.set_reserve_cap(&owner, &usdc_token, &1);
    force_tracked_balance_to_max(&env, &vault_address, &client, &usdc, &usdc_admin, &owner);

    expect_contract_error(client.try_deposit(&owner, &1), VaultError::Overflow as u32);
}

#[test]
fn deposit_overflow_with_unlimited_cap_returns_overflow() {
    let env = Env::default();
    let (owner, client, usdc_token, vault_address, usdc, usdc_admin, _) = setup(&env, 0);
    client.set_reserve_cap(&owner, &usdc_token, &i128::MAX);
    force_tracked_balance_to_max(&env, &vault_address, &client, &usdc, &usdc_admin, &owner);

    expect_contract_error(client.try_deposit(&owner, &1), VaultError::Overflow as u32);
}

#[test]
fn reserve_cap_deposit_cpu() {
    for cap in [None, Some(i128::MAX), Some(1_000)] {
        let env = Env::default();
        let (owner, client, usdc_token, vault_address, usdc, usdc_admin, _) = setup(&env, 900);
        if let Some(cap) = cap {
            client.set_reserve_cap(&owner, &usdc_token, &cap);
        }
        prepare_deposit(&owner, &vault_address, &usdc, &usdc_admin, 100);
        env.cost_estimate().budget().reset_unlimited();
        client.deposit(&owner, &100);
        let cpu = env.cost_estimate().budget().cpu_instruction_cost();
        std::println!("reserve_cap deposit cap={cap:?} cpu={cpu}");
        assert_eq!(client.balance(), 1_000);
    }
}

#[test]
fn reserve_cap_auth_rejects_wrong_signer_before_pause() {
    let env = Env::default();
    let (owner, client, usdc_token, vault_address, _, _, _) = setup(&env, 900);
    client.set_reserve_cap(&owner, &usdc_token, &1_000);
    client.pause(&owner);
    let intruder = Address::generate(&env);
    env.mock_auths(&[MockAuth {
        address: &intruder,
        invoke: &MockAuthInvoke {
            contract: &vault_address,
            fn_name: "deposit",
            args: (owner.clone(), 101_i128).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(matches!(client.try_deposit(&owner, &101), Err(Err(_))));
    env.mock_auths(&[MockAuth {
        address: &owner,
        invoke: &MockAuthInvoke {
            contract: &vault_address,
            fn_name: "deposit",
            args: (owner.clone(), 101_i128).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    expect_contract_error(client.try_deposit(&owner, &101), VaultError::Paused as u32);
}

#[test]
fn reserve_cap_setter_preserves_owner_and_amount_checks() {
    let env = Env::default();
    let (owner, client, usdc_token, _, _, _, _) = setup(&env, 0);
    let stranger = Address::generate(&env);
    expect_contract_error(client.try_set_reserve_cap(&stranger, &usdc_token, &0), 3);
    expect_contract_error(client.try_set_reserve_cap(&owner, &usdc_token, &0), 6);
    env.set_auths(&[]);
    assert!(matches!(
        client.try_set_reserve_cap(&owner, &usdc_token, &1),
        Err(Err(_))
    ));
}

#[test]
fn reserve_cap_limits_branches() {
    let env = Env::default();
    let (owner, client, usdc_token, vault_address, _, _, _) = setup(&env, 0);
    env.as_contract(&vault_address, || {
        assert_eq!(limits::check(&env, &usdc_token, i128::MAX, 1), Ok(()));
    });
    client.set_reserve_cap(&owner, &usdc_token, &i128::MAX);
    env.as_contract(&vault_address, || {
        // Unlimited caps skip arithmetic, including an otherwise overflowing sum.
        assert_eq!(limits::check(&env, &usdc_token, i128::MAX, 1), Ok(()));
    });
    client.set_reserve_cap(&owner, &usdc_token, &1_000);
    env.as_contract(&vault_address, || {
        assert_eq!(limits::check(&env, &usdc_token, 900, 99), Ok(()));
        assert_eq!(limits::check(&env, &usdc_token, 900, 100), Ok(()));
        assert_eq!(
            limits::check(&env, &usdc_token, 900, 101),
            Err(VaultError::ExceedsReserveCap)
        );
        assert_eq!(
            limits::check(&env, &usdc_token, i128::MAX, 1),
            Err(VaultError::Overflow)
        );
    });
}

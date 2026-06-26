//! TTL/Archival simulation tests for the RevenuePool contract.
//!
//! # Purpose
//! Verifies storage TTL extension behavior when the ledger advances
//! near or past `BUMP_AMOUNT`. Ensures that:
//! - Write operations (`distribute`) bump TTL and succeed.
//! - Reads after idle periods beyond the TTL window surface archival errors.
//!
//! # Constants found in lib.rs (Phase 1 research)
//! ```
//! pub const BUMP_AMOUNT:         u32 = 10000;  // ~16 days at 5s/ledger
//! pub const LIFETIME_THRESHOLD:  u32 = 1000;   // ~1.5 days at 5s/ledger
//! ```
//!
//! # How ledger time is advanced
//! `env.ledger().with_mut(|li| li.sequence_number += N)` fast-forwards
//! the ledger without executing intervening transactions. This is the
//! canonical test idiom for TTL simulation in Soroban SDK testutils.
//!
//! # Soroban v22 test environment behaviour
//! Soroban SDK v22 DOES enforce archival in the test environment. When the
//! ledger advances past an entry's TTL, the host marks it archived and panics
//! with `Error(Storage, InternalError)` on any subsequent access.
//!
//! `distribute()` calls `extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT)` BEFORE the
//! USDC transfer. This means:
//! - If the instance is not yet archived when `distribute()` is called, the
//!   `extend_ttl` refreshes the window and the transfer succeeds.
//! - If the instance IS already archived when `distribute()` is called, the call
//!   will panic before reaching `extend_ttl`.
//!
//! # Functions under test
//! - `distribute(caller, to, amount)` — admin-only USDC transfer that bumps instance TTL.

#![cfg(test)]

extern crate std;

use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{token, Address, Env};

use super::{RevenuePool, RevenuePoolClient, BUMP_AMOUNT, LIFETIME_THRESHOLD};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn create_usdc<'a>(
    env: &'a Env,
    admin: &Address,
) -> (
    Address,
    token::Client<'a>,
    token::StellarAssetClient<'a>,
) {
    let contract_address = env.register_stellar_asset_contract_v2(admin.clone());
    let address = contract_address.address();
    let client = token::Client::new(env, &address);
    let admin_client = token::StellarAssetClient::new(env, &address);
    (address, client, admin_client)
}

fn create_pool<'a>(env: &'a Env) -> (Address, RevenuePoolClient<'a>) {
    let address = env.register(RevenuePool, ());
    let client = RevenuePoolClient::new(env, &address);
    (address, client)
}

fn fund_pool(
    usdc_admin_client: &token::StellarAssetClient,
    pool_address: &Address,
    amount: i128,
) {
    usdc_admin_client.mint(pool_address, &amount);
}

// ---------------------------------------------------------------------------
// TTL/Archival simulation tests
// ---------------------------------------------------------------------------

/// Simulates an idle revenue pool where no writes occur for `BUMP_AMOUNT` ledger
/// entries. After fast-forwarding PAST the bump window, calling `distribute()` is
/// attempted.
#[test]
fn test_revenue_pool_distribute_after_ttl_expiry() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let developer = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 1_000);

    let bump_amount = BUMP_AMOUNT;
    env.ledger().with_mut(|li| {
        li.sequence_number += bump_amount + 1;
    });

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.distribute(&admin, &developer, &400)
    }));

    assert!(
        result.is_err(),
        "distribute() after TTL expiry must panic with archival error"
    );
}

/// Boundary condition: advance ledger to EXACTLY `BUMP_AMOUNT`.
#[test]
fn test_revenue_pool_distribute_at_ttl_threshold() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let developer = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, usdc_client, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 2_000);

    let bump_amount = BUMP_AMOUNT;
    env.ledger().with_mut(|li| {
        li.sequence_number += bump_amount;
    });

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.distribute(&admin, &developer, &500)
    }));

    match result {
        Ok(()) => {
            assert_eq!(
                usdc_client.balance(&developer),
                500,
                "developer must have received the distributed amount"
            );
        }
        Err(_) => {}
    }
}

/// Verifies that a write operation (`distribute`) before TTL expiry resets the
/// TTL window, allowing subsequent `distribute` calls to succeed even after the
/// original window elapsed.
#[test]
fn test_revenue_pool_write_resets_ttl() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let developer = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, usdc_client, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 3_000);

    let bump_amount = BUMP_AMOUNT;
    env.ledger().with_mut(|li| {
        li.sequence_number += bump_amount / 2;
    });

    client.distribute(&admin, &developer, &1_000);
    assert_eq!(
        usdc_client.balance(&developer),
        1_000,
        "first distribute must succeed within the original TTL window"
    );

    env.ledger().with_mut(|li| {
        li.sequence_number += bump_amount / 2 + 100;
    });

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.distribute(&admin, &developer, &500)
    }));

    assert!(
        result.is_ok(),
        "second distribute must succeed after TTL was reset by first distribute"
    );
    assert_eq!(
        usdc_client.balance(&developer),
        1_500,
        "developer must have received both distributed amounts"
    );
}

/// Verifies that `BUMP_AMOUNT` and `LIFETIME_THRESHOLD` match specifications.
#[test]
fn test_revenue_pool_ttl_constants_match_spec() {
    assert_eq!(
        BUMP_AMOUNT,
        10_000,
        "BUMP_AMOUNT must equal 10_000 ledgers (~16 days)"
    );
    assert_eq!(
        LIFETIME_THRESHOLD,
        1_000,
        "LIFETIME_THRESHOLD must equal 1_000 ledgers (~1.5 days)"
    );
}

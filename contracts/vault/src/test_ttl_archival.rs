//! TTL/Archival simulation tests for the Vault contract.
//!
//! # Purpose
//! Verifies storage TTL extension behavior when the ledger advances
//! near or past `INSTANCE_BUMP_AMOUNT`. Ensures that either:
//! - The contract successfully bumps TTL on write access, or
//! - Archival errors surface cleanly for contracts without TTL bumps.
//!
//! # Constants found in lib.rs (Phase 1 research)
//! ```
//! pub const INSTANCE_BUMP_THRESHOLD: u32 = 17_280 * 30;  // ~30 days
//! pub const INSTANCE_BUMP_AMOUNT:    u32 = 17_280 * 60;  // ~60 days = 1_036_800 ledgers
//! ```
//!
//! # How ledger time is advanced
//! `env.ledger().with_mut(|li| li.sequence_number += N)` fast-forwards
//! the ledger without executing intervening transactions. This is the
//! canonical test idiom for TTL simulation in Soroban SDK testutils.
//!
//! # Soroban v22 test environment behaviour
//! Soroban SDK v22 DOES enforce archival in the test environment:
//! - Advancing `sequence_number` past the entry's TTL causes the host to mark it
//!   archived and return `Error(Storage, InternalError)` on access.
//! - Write operations that call `extend_ttl` before the window expires will reset
//!   the TTL window, preventing archival.
//! - `balance()` is a pure read; it does NOT call `extend_ttl`. However, the vault
//!   initializes with `INSTANCE_BUMP_AMOUNT = 1_036_800` ledgers, which is large
//!   enough that these tests (advancing by bump_amount + 1 ≈ 1_036_801 ledgers) may
//!   still hit the archival boundary.
//!
//! # Key observation
//! After `init()`, the vault's instance TTL is set to `INSTANCE_BUMP_AMOUNT` ledgers.
//! Advancing past that window AND then reading will trigger archival.
//! A `deposit()` call before the window closes resets the TTL.
//!
//! # Functions under test
//! - `balance()` — returns `Ok(meta.balance)` from instance storage; no TTL bump on read.
//! - `deposit()` — mutating write that bumps instance TTL after updating balance.

#![cfg(test)]

extern crate std;

use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{token, Address, Env};

use super::{CalloraVault, CalloraVaultClient, INSTANCE_BUMP_AMOUNT, INSTANCE_BUMP_THRESHOLD};

// ---------------------------------------------------------------------------
// Test helpers — mirroring the pattern from existing vault/src/test.rs
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

fn create_vault<'a>(env: &'a Env) -> (Address, CalloraVaultClient<'a>) {
    let address = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(env, &address);
    (address, client)
}

fn fund_vault(
    usdc_admin_client: &token::StellarAssetClient,
    vault_address: &Address,
    amount: i128,
) {
    usdc_admin_client.mint(vault_address, &amount);
}

// ---------------------------------------------------------------------------
// TTL/Archival simulation tests
// ---------------------------------------------------------------------------

/// Simulates an idle vault where no writes occur for `INSTANCE_BUMP_AMOUNT` ledger
/// entries. After fast-forwarding PAST the bump threshold, calling `balance()` is
/// attempted. Since `balance()` is a pure read with no `extend_ttl` call, the
/// Soroban v22 host archives the instance and the call returns an error.
#[test]
fn test_vault_balance_after_ttl_expiry() {
    let env = Env::default();
    env.mock_all_auths();

    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(&owner, &usdc, &Some(500), &None, &None, &None, &None);

    assert_eq!(client.balance(), 500);

    let bump_amount = INSTANCE_BUMP_AMOUNT;
    env.ledger().with_mut(|li| {
        li.sequence_number += bump_amount + 1;
    });

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.balance()
    }));

    assert!(
        result.is_err(),
        "balance() after TTL expiry must panic with archival error — \
         the instance is archived after INSTANCE_BUMP_AMOUNT ledgers without a write."
    );
}

/// Boundary condition: advance ledger to EXACTLY `INSTANCE_BUMP_AMOUNT`.
#[test]
fn test_vault_balance_at_ttl_threshold() {
    let env = Env::default();
    env.mock_all_auths();

    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(&owner, &usdc, &Some(1000), &None, &None, &None, &None);

    assert_eq!(client.balance(), 1000);

    let bump_amount = INSTANCE_BUMP_AMOUNT;
    env.ledger().with_mut(|li| {
        li.sequence_number += bump_amount;
    });

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.balance()
    }));

    match result {
        Ok(bal) => {
            assert_eq!(bal, 1000, "balance must be preserved at the TTL boundary if accessible");
        }
        Err(_) => {
            // Archival panic at boundary is also valid
        }
    }
}

/// Verifies that a write operation (`deposit`) before TTL expiry resets the TTL
/// window, allowing a subsequent read to succeed even after the original window
/// elapsed.
#[test]
fn test_vault_write_resets_ttl() {
    let env = Env::default();
    env.mock_all_auths();

    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    fund_vault(&usdc_admin, &vault_address, 200);
    client.init(&owner, &usdc, &Some(200), &None, &None, &None, &None);

    usdc_admin.mint(&owner, &100);
    usdc_client.approve(&owner, &vault_address, &100, &(INSTANCE_BUMP_AMOUNT * 3));

    assert_eq!(client.balance(), 200);

    let bump_amount = INSTANCE_BUMP_AMOUNT;
    env.ledger().with_mut(|li| {
        li.sequence_number += bump_amount / 2;
    });

    let new_balance = client.deposit(&owner, &100);
    assert_eq!(new_balance, 300, "deposit should succeed and return updated balance");

    env.ledger().with_mut(|li| {
        li.sequence_number += bump_amount / 2 + 100;
    });

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.balance()
    }));

    assert!(
        result.is_ok(),
        "balance() must succeed after write reset the TTL — the new TTL window is still active"
    );
    assert_eq!(
        result.unwrap(),
        300,
        "balance must reflect the value set by the deposit"
    );
}

/// Verifies that `INSTANCE_BUMP_THRESHOLD` and `INSTANCE_BUMP_AMOUNT` are set
/// to the expected values documented in the contract header. If someone changes
/// these constants, this test will fail and force a review of `STORAGE.md`.
#[test]
fn test_vault_ttl_constants_match_spec() {
    assert_eq!(
        INSTANCE_BUMP_THRESHOLD,
        17_280 * 30,
        "INSTANCE_BUMP_THRESHOLD must equal 17_280 * 30 (~30 days)"
    );
    assert_eq!(
        INSTANCE_BUMP_AMOUNT,
        17_280 * 60,
        "INSTANCE_BUMP_AMOUNT must equal 17_280 * 60 (~60 days)"
    );
}

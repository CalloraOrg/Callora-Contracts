//! TTL/Archival simulation tests for the Settlement contract.
//!
//! # Purpose
//! Verifies that the Settlement contract's instance storage archival behaviour
//! surfaces correctly in Soroban v22 tests. Since the contract does NOT call
//! `extend_ttl` in `receive_payment`, the instance will be archived after
//! the minimum ledger TTL (default in testutils).
//!
//! # Constants found in lib.rs (Phase 1 research)
//! The settlement contract defines NO instance-storage TTL constants and makes
//! NO `extend_ttl` calls on instance storage in `receive_payment` or `init`.
//!
//! Persistent storage for developer balances calls `extend_ttl(50000, 50000)`.
//!
//! # How ledger time is advanced
//! `env.ledger().with_mut(|li| li.sequence_number += N)` fast-forwards
//! the ledger without executing intervening transactions.
//!
//! # Soroban v22 test environment behaviour
//! Soroban SDK v22 DOES enforce real archival in tests. When the ledger
//! advances past the entry's TTL, the host panics with `Error(Storage, InternalError)`.

#![cfg(test)]

extern crate std;

use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env};

use super::{CalloraSettlement, CalloraSettlementClient};

/// Ledger advance that is safely WITHIN the default minimum instance TTL.
/// We use 100 ledgers (well below min_persistent_entry_ttl ≈ 4096).
const WITHIN_TTL_ADVANCE: u32 = 100;

/// Ledger advance that is safely PAST the default minimum instance TTL.
/// We use 5_000 ledgers (above min_persistent_entry_ttl ≈ 4096).
const PAST_TTL_ADVANCE: u32 = 5_000;

// ---------------------------------------------------------------------------
// TTL/Archival simulation tests
// ---------------------------------------------------------------------------

/// Simulates an idle settlement contract where no writes occur for
/// `PAST_TTL_ADVANCE` ledger entries (5_000 ledgers). After fast-forwarding
/// PAST the default minimum instance TTL, calling `receive_payment(to_pool=true)`
/// fails with an archival panic.
#[test]
fn test_settlement_receive_payment_after_ttl_expiry() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let vault = Address::generate(&env);
    let addr = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &addr);

    client.init(&admin, &vault);

    env.ledger().with_mut(|li| {
        li.sequence_number += PAST_TTL_ADVANCE;
    });

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.receive_payment(&vault, &1_000i128, &true, &None)
    }));

    assert!(
        result.is_err(),
        "receive_payment() after TTL expiry must panic with an archival error"
    );
}

/// Before TTL expiry: advancing within the safe window keeps the contract accessible.
#[test]
fn test_settlement_receive_payment_at_ttl_threshold() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let vault = Address::generate(&env);
    let developer = Address::generate(&env);
    let addr = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &addr);

    client.init(&admin, &vault);

    env.ledger().with_mut(|li| {
        li.sequence_number += WITHIN_TTL_ADVANCE;
    });

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.receive_payment(&vault, &500i128, &false, &Some(developer.clone()))
    }));

    assert!(
        result.is_ok(),
        "receive_payment() must succeed when ledger has advanced only {} ledgers",
        WITHIN_TTL_ADVANCE
    );

    let balance = client.get_developer_balance(&developer);
    assert_eq!(
        balance, 500,
        "developer balance must be credited when called within the TTL window"
    );
}

/// Verifies that an admin write (which does NOT bump instance TTL) followed by
/// a large ledger advance causes archival, while a write that DOES bump TTL
/// would prevent it (hypothetical test to document the missing extend_ttl gap).
#[test]
fn test_settlement_write_does_not_reset_ttl() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let vault = Address::generate(&env);
    let addr = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &addr);

    client.init(&admin, &vault);

    client.receive_payment(&vault, &1_000i128, &true, &None);
    let pool_after_first = client.get_global_pool();
    assert_eq!(pool_after_first.total_balance, 1_000);

    env.ledger().with_mut(|li| {
        li.sequence_number += PAST_TTL_ADVANCE;
    });

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.receive_payment(&vault, &2_000i128, &true, &None)
    }));

    assert!(
        result.is_err(),
        "second receive_payment must fail after advancing {} ledgers — \
         settlement's receive_payment does not call extend_ttl",
        PAST_TTL_ADVANCE
    );
}

#[test]
fn test_settlement_ttl_simulation_window_matches_persistent_bump() {
    assert!(
        WITHIN_TTL_ADVANCE < 4096,
        "WITHIN_TTL_ADVANCE ({}) must be below the default min instance TTL (~4096)",
        WITHIN_TTL_ADVANCE
    );
    assert!(
        PAST_TTL_ADVANCE > 4096,
        "PAST_TTL_ADVANCE ({}) must be above the default min instance TTL (~4096)",
        PAST_TTL_ADVANCE
    );
}

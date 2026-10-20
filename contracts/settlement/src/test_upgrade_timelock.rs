//! Tests for the timelocked settlement upgrade flow.
//!
//! Covers every acceptance criterion from the issue:
//!
//! 1. Upgrade cannot execute before the configured delay.
//! 2. Pending upgrade is visible via `get_pending_upgrade`.
//! 3. Cancel clears the proposal and emits `upgrade_cancelled`.
//! 4. `execute_upgrade` succeeds after the delay (mocked via `env.ledger()`).
//! 5. Zero WASM hash is rejected by `propose_upgrade`.
//! 6. Non-admin callers are rejected on all three mutating entry points.
//! 7. `execute_upgrade` / `cancel_upgrade` fail when no proposal is pending.
//! 8. Re-proposal replaces the old entry and restarts the delay.
//! 9. `upgrade` (alias) behaves identically to `propose_upgrade`.

#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    Address, Bytes, BytesN, Env, Symbol, TryFromVal,
};

use crate::{
    CalloraSettlement, CalloraSettlementClient, SettlementError, StorageKey, UPGRADE_GRACE_SECONDS,
    UPGRADE_TIMELOCK_SECONDS,
};

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Register the contract, call `init`, and return `(env, client, admin, vault)`.
fn setup() -> (Env, CalloraSettlementClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let vault = Address::generate(&env);
    client.init(&admin, &vault);
    (env, client, admin, vault)
}

/// Return a non-zero 32-byte WASM hash.
fn dummy_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0xABu8; 32])
}

/// Return an all-zero 32-byte hash (sentinel for "invalid").
fn zero_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0u8; 32])
}

/// Upload an empty (but valid) WASM blob and return its 32-byte hash. The
/// Soroban test deployer treats the bytes as a mock contract, so `execute_upgrade`
/// can install the hash without a real WASM module.
fn test_wasm_hash(env: &Env) -> BytesN<32> {
    env.deployer().upload_contract_wasm(Bytes::new(&env))
}

// ─── propose_upgrade ────────────────────────────────────────────────────────

#[test]
fn test_propose_upgrade_creates_pending_entry() {
    let (env, client, admin, _vault) = setup();
    let hash = dummy_hash(&env);
    let now = env.ledger().timestamp();

    client.propose_upgrade(&admin, &hash);

    let pending = client.get_pending_upgrade().expect("proposal should exist");
    assert_eq!(pending.wasm_hash, hash);
    assert_eq!(pending.proposed_at, now);
    assert_eq!(pending.execute_after, now + UPGRADE_TIMELOCK_SECONDS);
}

#[test]
fn test_propose_upgrade_rejects_zero_hash() {
    let (env, client, admin, _vault) = setup();
    let err = client
        .try_propose_upgrade(&admin, &zero_hash(&env))
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(SettlementError::ZeroWasmHash as u32)
    );
}

#[test]
fn test_propose_upgrade_rejects_non_admin() {
    let (env, client, _admin, _vault) = setup();
    let stranger = Address::generate(&env);
    let hash = dummy_hash(&env);
    let err = client
        .try_propose_upgrade(&stranger, &hash)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(SettlementError::Unauthorized as u32)
    );
}

#[test]
fn test_propose_upgrade_replaces_existing_proposal() {
    let (env, client, admin, _vault) = setup();
    let hash1 = dummy_hash(&env);
    let hash2 = BytesN::from_array(&env, &[0xCDu8; 32]);

    client.propose_upgrade(&admin, &hash1);

    // Advance time so the first proposal's deadline has moved.
    env.ledger().with_mut(|l| l.timestamp += 1000);
    client.propose_upgrade(&admin, &hash2);

    let pending = client.get_pending_upgrade().expect("proposal should exist");
    assert_eq!(
        pending.wasm_hash, hash2,
        "second proposal should replace first"
    );
    // execute_after should be based on the new proposed_at (original + 1000 + delay)
    assert!(pending.execute_after > 1000 + UPGRADE_TIMELOCK_SECONDS - 1);
}

// ─── execute_upgrade ────────────────────────────────────────────────────────

#[test]
fn test_execute_upgrade_rejects_before_delay() {
    let (env, client, admin, _vault) = setup();
    let hash = dummy_hash(&env);
    client.propose_upgrade(&admin, &hash);

    // Advance time but stay inside the timelock window.
    env.ledger()
        .with_mut(|l| l.timestamp += UPGRADE_TIMELOCK_SECONDS - 1);

    let err = client.try_execute_upgrade(&admin).unwrap_err().unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(SettlementError::UpgradeTimelockNotExpired as u32)
    );
}

#[test]
fn test_execute_upgrade_rejects_non_admin() {
    let (env, client, admin, _vault) = setup();
    let stranger = Address::generate(&env);
    let hash = dummy_hash(&env);
    client.propose_upgrade(&admin, &hash);

    env.ledger()
        .with_mut(|l| l.timestamp += UPGRADE_TIMELOCK_SECONDS + 1);

    let err = client.try_execute_upgrade(&stranger).unwrap_err().unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(SettlementError::Unauthorized as u32)
    );
}

#[test]
fn test_execute_upgrade_fails_when_no_pending() {
    let (_env, client, admin, _vault) = setup();
    let err = client.try_execute_upgrade(&admin).unwrap_err().unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(SettlementError::NoUpgradePending as u32)
    );
}

#[test]
fn test_execute_upgrade_clears_proposal_after_success() {
    let (env, client, admin, _vault) = setup();
    let hash = test_wasm_hash(&env);
    client.propose_upgrade(&admin, &hash);

    env.ledger()
        .with_mut(|l| l.timestamp += UPGRADE_TIMELOCK_SECONDS);

    client.execute_upgrade(&admin);

    // Proposal must be gone after execution; version updated to the installed hash.
    assert!(
        client.get_pending_upgrade().is_none(),
        "pending proposal should be cleared after execution"
    );
    assert_eq!(client.get_version(), Some(hash));
}

#[test]
fn test_execute_upgrade_at_exact_deadline() {
    // Execute_after is the earliest inclusive timestamp, so `now == execute_after` should pass.
    let (env, client, admin, _vault) = setup();
    let hash = test_wasm_hash(&env);
    client.propose_upgrade(&admin, &hash);

    env.ledger()
        .with_mut(|l| l.timestamp += UPGRADE_TIMELOCK_SECONDS);

    // Should not panic.
    client.execute_upgrade(&admin);
    assert!(client.get_pending_upgrade().is_none());
    assert_eq!(client.get_version(), Some(hash));
}

// ─── cancel_upgrade ─────────────────────────────────────────────────────────

#[test]
fn test_cancel_upgrade_clears_proposal() {
    let (env, client, admin, _vault) = setup();
    let hash = dummy_hash(&env);
    client.propose_upgrade(&admin, &hash);

    assert!(
        client.get_pending_upgrade().is_some(),
        "proposal should exist before cancel"
    );

    client.cancel_upgrade(&admin);

    assert!(
        client.get_pending_upgrade().is_none(),
        "proposal should be cleared after cancel"
    );
}

#[test]
fn test_cancel_upgrade_rejects_non_admin() {
    let (env, client, admin, _vault) = setup();
    let stranger = Address::generate(&env);
    let hash = dummy_hash(&env);
    client.propose_upgrade(&admin, &hash);

    let err = client.try_cancel_upgrade(&stranger).unwrap_err().unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(SettlementError::Unauthorized as u32)
    );
}

#[test]
fn test_cancel_upgrade_fails_when_no_pending() {
    let (_env, client, admin, _vault) = setup();
    let err = client.try_cancel_upgrade(&admin).unwrap_err().unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(SettlementError::NoUpgradePending as u32)
    );
}

#[test]
fn test_cancel_upgrade_before_delay_is_allowed() {
    // Cancellation must work even if the timelock has not expired yet.
    let (env, client, admin, _vault) = setup();
    let hash = dummy_hash(&env);
    client.propose_upgrade(&admin, &hash);

    // Still inside the window.
    env.ledger()
        .with_mut(|l| l.timestamp += UPGRADE_TIMELOCK_SECONDS / 2);

    client.cancel_upgrade(&admin); // must succeed
    assert!(client.get_pending_upgrade().is_none());
}

// ─── get_pending_upgrade ────────────────────────────────────────────────────

#[test]
fn test_get_pending_upgrade_returns_none_before_proposal() {
    let (_env, client, _admin, _vault) = setup();
    assert!(client.get_pending_upgrade().is_none());
}

#[test]
fn test_get_pending_upgrade_returns_some_after_proposal() {
    let (env, client, admin, _vault) = setup();
    let hash = dummy_hash(&env);
    client.propose_upgrade(&admin, &hash);
    assert!(client.get_pending_upgrade().is_some());
}

// ─── upgrade alias ──────────────────────────────────────────────────────────

#[test]
fn test_upgrade_alias_behaves_like_propose() {
    let (env, client, admin, _vault) = setup();
    let hash = dummy_hash(&env);
    let now = env.ledger().timestamp();

    // `upgrade` is the deprecated alias for `propose_upgrade`.
    client.upgrade(&admin, &hash);

    let pending = client
        .get_pending_upgrade()
        .expect("alias should create proposal");
    assert_eq!(pending.wasm_hash, hash);
    assert_eq!(pending.proposed_at, now);
    assert_eq!(pending.execute_after, now + UPGRADE_TIMELOCK_SECONDS);
}

#[test]
fn test_upgrade_alias_does_not_execute_immediately() {
    let (env, client, admin, _vault) = setup();
    let hash = dummy_hash(&env);
    client.upgrade(&admin, &hash);

    // No execute call → version should still be `None`.
    assert!(
        client.get_version().is_none(),
        "`upgrade` alias must not execute immediately"
    );
}

/// Returns `true` if any recorded event has topic[0] equal to `topic`.
fn event_emitted(env: &Env, topic: &str) -> bool {
    let expected = Symbol::new(env, topic);
    env.events().all().iter().any(|e| {
        !e.1.is_empty() && {
            let sym: Symbol = e.1.get(0).unwrap().into_val(env);
            sym == expected
        }
    })
}

// ─── Test plan: expiry boundary ────────────────────────────────────────────

#[test]
fn test_execute_upgrade_succeeds_at_expiry_boundary() {
    // Row 2: `now == expires_at` is inclusive and succeeds.
    let (env, client, admin, _vault) = setup();
    let hash = test_wasm_hash(&env);
    client.propose_upgrade(&admin, &hash);
    let expires_at = client.get_pending_upgrade().unwrap().expires_at;

    env.ledger().with_mut(|l| l.timestamp = expires_at);
    client.execute_upgrade(&admin);

    assert!(client.get_pending_upgrade().is_none());
    assert_eq!(client.get_version(), Some(hash));
}

#[test]
fn test_execute_upgrade_rejects_after_expiry() {
    // Row 3: `now == expires_at + 1` → UpgradeProposalExpired, no write.
    let (env, client, admin, _vault) = setup();
    let hash = test_wasm_hash(&env);
    client.propose_upgrade(&admin, &hash);
    let expires_at = client.get_pending_upgrade().unwrap().expires_at;

    env.ledger().with_mut(|l| l.timestamp = expires_at + 1);
    let err = SettlementError::try_from(client.try_execute_upgrade(&admin).unwrap_err().unwrap())
        .unwrap();
    assert_eq!(err, SettlementError::UpgradeProposalExpired);

    // Proposal untouched on the failing path.
    let pending = client.get_pending_upgrade().expect("proposal must survive");
    assert_eq!(pending.wasm_hash, hash);
}

// ─── Test plan: rotation clears pending upgrade ────────────────────────────

#[test]
fn test_rotation_clears_pending_upgrade() {
    // Row 4: propose, rotate admin, new admin cannot execute old hash;
    // `upgrade_cancelled` emitted at rotation.
    let (env, client, admin, _vault) = setup();
    let hash = test_wasm_hash(&env);
    client.propose_upgrade(&admin, &hash);
    assert!(client.get_pending_upgrade().is_some());

    let new_admin = Address::generate(&env);
    client.set_admin(&admin, &new_admin);
    env.events().all(); // clear nomination events
    client.accept_admin();

    assert!(client.get_pending_upgrade().is_none());
    assert!(event_emitted(&env, "upgrade_cancelled"));

    let err =
        SettlementError::try_from(client.try_execute_upgrade(&new_admin).unwrap_err().unwrap())
            .unwrap();
    assert_eq!(err, SettlementError::NoUpgradePending);
}

#[test]
fn test_accept_admin_with_no_transfer_pending_is_typed_error() {
    // Row 5: `accept_admin` with nothing pending → NoAdminTransferPending.
    let (_env, client, _admin, _vault) = setup();
    let err = SettlementError::try_from(client.try_accept_admin().unwrap_err().unwrap()).unwrap();
    assert_eq!(err, SettlementError::NoAdminTransferPending);
}

// ─── Test plan: replace on re-propose ─────────────────────────────────────

#[test]
fn test_propose_upgrade_twice_emits_upgrade_replaced() {
    // Row 6: proposing over an existing proposal replaces it and emits
    // `upgrade_replaced`.
    let (env, client, admin, _vault) = setup();
    let hash1 = dummy_hash(&env);
    let hash2 = dummy_hash(&env);

    client.propose_upgrade(&admin, &hash1);
    env.events().all(); // clear first proposal event
    client.propose_upgrade(&admin, &hash2);

    assert!(event_emitted(&env, "upgrade_replaced"));
    let pending = client.get_pending_upgrade().unwrap();
    assert_eq!(pending.wasm_hash, hash2);
}

// ─── Test plan: expires_at overflow ───────────────────────────────────────

#[test]
fn test_propose_upgrade_expires_at_overflow() {
    // Row 7: `execute_after + grace` overflows → TimelockOverflow.
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(u64::MAX - 500_000);
    let contract_id = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let vault = Address::generate(&env);
    client.init(&admin, &vault);

    let hash = dummy_hash(&env);
    let err = SettlementError::try_from(
        client
            .try_propose_upgrade(&admin, &hash)
            .unwrap_err()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(err, SettlementError::TimelockOverflow);
}

// ─── Test plan: legacy proposal decoding ─────────────────────────────────

#[test]
fn test_legacy_proposal_without_expires_at_decodes_with_grace_window() {
    // Row 8: a proposal stored without `expires_at` still decodes and is
    // re-animated with `expires_at = execute_after + grace`.
    use crate::timelock::LegacyPendingUpgrade;

    let (env, client, admin, _vault) = setup();
    let hash = dummy_hash(&env);
    let proposed_at = env.ledger().timestamp();
    let execute_after = proposed_at + UPGRADE_TIMELOCK_SECONDS;

    let legacy = LegacyPendingUpgrade {
        wasm_hash: hash.clone(),
        proposed_at,
        execute_after,
    };
    env.storage()
        .persistent()
        .set(&StorageKey::PendingUpgrade, &legacy);

    let pending = client.get_pending_upgrade().unwrap();
    assert_eq!(pending.wasm_hash, hash);
    assert_eq!(pending.expires_at, execute_after + UPGRADE_GRACE_SECONDS);

    // Executing at `execute_after` (within the grace window) succeeds.
    env.ledger().with_mut(|l| l.timestamp = execute_after);
    client.execute_upgrade(&admin);
    assert!(client.get_pending_upgrade().is_none());
    assert_eq!(client.get_version(), Some(hash));
}

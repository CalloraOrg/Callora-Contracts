//! Timelock state and storage helpers for developer balance migrations and
//! contract upgrades.

use soroban_sdk::{contracttype, Address, BytesN, Env, Map, Symbol, TryFromVal, Val};

use crate::{StorageKey, PERSISTENT_BUMP_AMOUNT, PERSISTENT_BUMP_THRESHOLD};

/// Mandatory delay between proposing and executing a balance migration.
pub const DEVELOPER_MIGRATION_TIMELOCK_SECONDS: u64 = 86_400;

/// Mandatory delay between proposing and executing a contract upgrade (48 h).
pub const UPGRADE_TIMELOCK_SECONDS: u64 = 172_800;

/// Grace window after `execute_after` during which a proposal may still be
/// executed (7 days). After `execute_after + UPGRADE_GRACE_SECONDS` the
/// proposal expires and can no longer be executed.
pub const UPGRADE_GRACE_SECONDS: u64 = 604_800;

/// Proposal bump constants reused from the developer migration pattern.
const PROPOSAL_BUMP_THRESHOLD: u32 = 17_280 * 30; // ~30 days
const PROPOSAL_BUMP_AMOUNT: u32 = 17_280 * 60; // ~60 days

/// Immutable snapshot stored for a pending timelocked WASM upgrade.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PendingUpgrade {
    /// 32-byte WASM hash proposed for installation.
    pub wasm_hash: BytesN<32>,
    /// Ledger timestamp at which the proposal was recorded.
    pub proposed_at: u64,
    /// Earliest ledger timestamp at which `execute_upgrade` may run.
    pub execute_after: u64,
    /// Latest ledger timestamp at which `execute_upgrade` may run
    /// (`execute_after + UPGRADE_GRACE_SECONDS`). After this the proposal
    /// expires and `execute_upgrade` rejects it with `UpgradeProposalExpired`.
    pub expires_at: u64,
}

/// Legacy representation of `PendingUpgrade` without `expires_at`, kept so
/// proposals stored before the expiry field was added still decode. A legacy
/// proposal is re-animated with `expires_at = execute_after + UPGRADE_GRACE_SECONDS`
/// (saturating at `u64::MAX` on overflow) and re-persisted on first read.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct LegacyPendingUpgrade {
    pub wasm_hash: BytesN<32>,
    pub proposed_at: u64,
    pub execute_after: u64,
}

/// Compute `proposed_at + window`, returning `None` on `u64` overflow.
pub(crate) fn saturating_deadline(proposed_at: u64, window: u64) -> Option<u64> {
    if window == 0 {
        return Some(proposed_at);
    }
    let delta = u64::MAX.checked_sub(proposed_at)?;
    if window > delta {
        return None;
    }
    proposed_at.checked_add(window)
}

/// Immutable approval snapshot stored for a pending developer migration.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PendingDeveloperMigration {
    pub from: Address,
    pub to: Address,
    pub amount: i128,
    pub proposed_at: u64,
    pub execute_after: u64,
}

/// Read a pending migration and refresh its storage lifetime.
pub(crate) fn get_pending_migration(
    env: &Env,
    from: &Address,
) -> Option<PendingDeveloperMigration> {
    let key = StorageKey::PendingDeveloperMigration(from.clone());
    if env.storage().persistent().has(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_BUMP_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }
    env.storage().persistent().get(&key)
}

/// Persist a pending migration and refresh its storage lifetime.
pub(crate) fn set_pending_migration(env: &Env, migration: &PendingDeveloperMigration) {
    let key = StorageKey::PendingDeveloperMigration(migration.from.clone());
    env.storage().persistent().set(&key, migration);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_BUMP_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
}

/// Consume a successfully executed proposal to make replay impossible.
pub(crate) fn remove_pending_migration(env: &Env, from: &Address) {
    env.storage()
        .persistent()
        .remove(&StorageKey::PendingDeveloperMigration(from.clone()));
}

// ─── PendingUpgrade helpers ──────────────────────────────────────────────────

/// Read the pending upgrade proposal, refreshing its storage lifetime.
/// Legacy proposals stored without `expires_at` are re-animated with a fresh
/// grace window (`execute_after + UPGRADE_GRACE_SECONDS`) and re-persisted.
pub(crate) fn get_pending_upgrade(env: &Env) -> Option<PendingUpgrade> {
    let key = StorageKey::PendingUpgrade;
    let raw: Val = env.storage().persistent().get(&key)?;
    let map = Map::<Symbol, Val>::try_from_val(env, &raw).ok()?;
    if map.contains_key(Symbol::new(env, "expires_at")) {
        if env.storage().persistent().has(&key) {
            env.storage().persistent().extend_ttl(
                &key,
                PROPOSAL_BUMP_THRESHOLD,
                PROPOSAL_BUMP_AMOUNT,
            );
        }
        return PendingUpgrade::try_from_val(env, &raw).ok();
    }
    let legacy = LegacyPendingUpgrade::try_from_val(env, &raw).ok()?;
    let expires_at =
        saturating_deadline(legacy.execute_after, UPGRADE_GRACE_SECONDS).unwrap_or(u64::MAX);
    let proposal = PendingUpgrade {
        wasm_hash: legacy.wasm_hash,
        proposed_at: legacy.proposed_at,
        execute_after: legacy.execute_after,
        expires_at,
    };
    set_pending_upgrade(env, &proposal);
    Some(proposal)
}

/// Persist a new upgrade proposal and refresh its storage lifetime.
pub(crate) fn set_pending_upgrade(env: &Env, proposal: &PendingUpgrade) {
    let key = StorageKey::PendingUpgrade;
    env.storage().persistent().set(&key, proposal);
    env.storage()
        .persistent()
        .extend_ttl(&key, PROPOSAL_BUMP_THRESHOLD, PROPOSAL_BUMP_AMOUNT);
}

/// Remove a pending upgrade proposal (after execution or cancellation).
pub(crate) fn clear_pending_upgrade(env: &Env) {
    env.storage()
        .persistent()
        .remove(&StorageKey::PendingUpgrade);
}

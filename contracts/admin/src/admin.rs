//! Admin lifecycle module for the Callora Admin contract.
//!
//! Implements a two-step admin transfer pattern that mirrors the rest of the
//! Callora contracts so behaviour stays consistent across the workspace:
//!
//! 1. The **current** admin calls `set_admin(env, caller, new_admin)` to
//!    nominate a successor. The pending slot is populated and the
//!    `admin_nominated` event is emitted.
//! 2. The **nominated** admin calls `accept_admin(env, caller)` to claim the
//!    role, but only inside the acceptance window opened by the timelock
//!    (issue #1045). Storage is updated, the pending slot is cleared, and the
//!    `admin_changed` event is emitted with both the previous and new admin
//!    so indexers can record the full handover in a single event.
//! 3. Either of the current admin (step 1 replays the nomination, replacing
//!    any prior pending admin) or a dedicated `cancel_admin_transfer` call
//!    may revoke the pending slot before step 2 completes — that path emits
//!    `admin_cancelled`.
//!
//! The two-step pattern prevents accidental role transfers to an unreachable
//! or mistyped address and gives the nominee an explicit acceptance window
//! during which they can refuse the role.
//!
//! # Timelock (issue #1045)
//!
//! A nomination does not become acceptable immediately. `set_admin` stamps the
//! pending record with:
//!
//! * `eta` = `now + `[`ROTATION_DELAY_SECS`] — the earliest ledger timestamp at
//!   which the nominee may accept, and
//! * `expires_at` = `eta + `[`ROTATION_GRACE_SECS`] — the last timestamp at
//!   which they still may.
//!
//! The delay is the point of the mechanism: a stolen or coerced admin key
//! cannot hand the role away in one transaction, because the nomination is
//! public for [`ROTATION_DELAY_SECS`] before it can take effect and the real
//! admin can [`cancel_admin_transfer`] it at any moment in that window.
//! The expiry is the other half: a nomination left lying around does not stay
//! acceptable forever, so an old, forgotten hand-over cannot be redeemed by a
//! nominee whose key later leaks.
//!
//! Both bounds fail closed — [`accept_admin`] panics rather than promoting
//! when the clock is outside the window, and neither path mutates storage
//! before the checks pass. Re-nominating restarts the clock, so an admin
//! cannot pre-warm a slot with a harmless address and swap in another one at
//! the end of the delay.
//!
//! Every nomination carries a monotonically increasing `rotation_id`, so an
//! indexer (or an operator reading events) can tell a fresh nomination from a
//! replay of an older one that was cancelled or left to expire.
//!
//! # Events
//!
//! | Function                  | Topic                | Topics                          | Data                              |
//! |---------------------------|----------------------|---------------------------------|-----------------------------------|
//! | `init`                    | `admin_init`         | `(topic, initial_admin)`        | `()`                              |
//! | `set_admin`               | `admin_nominated`    | `(topic, current_admin)`        | `(pending_admin, eta, expires_at)`|
//! | `accept_admin`            | `admin_changed`      | `(topic, new_admin)`            | `(previous_admin, new_admin)`     |
//! | `cancel_admin_transfer`   | `admin_cancelled`    | `(topic, current_admin)`        | `cancelled_pending_admin`         |
//!
//! Read-only helpers (`get_admin`, `get_pending_admin`) do **not** emit
//! events.

use soroban_sdk::{contracttype, Address, Env, Symbol};

use crate::events;

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

/// Instance storage key for the active admin address.
///
/// Mirrors the `Admin` key documented in `contracts/admin/docs/storage.md`.
/// Instance storage is appropriate because the admin is core global
/// configuration shared across the entire contract lifecycle.
const ADMIN_KEY: &str = "admin";

/// Instance storage key for the pending admin during a two-step transfer.
///
/// Mirrors the `PendingAdmin` key documented in
/// `contracts/admin/docs/storage.md`. Optional / nullable — only present
/// between `set_admin` and the matching `accept_admin` / `cancel`.
const PENDING_ADMIN_KEY: &str = "pending_admin";

/// Instance storage key for the monotonically increasing rotation counter
/// (issue #1045).
///
/// Incremented on every nomination and never reset, so a `rotation_id` is
/// unique for the life of the contract and identifies exactly one nomination.
const ROTATION_ID_KEY: &str = "rotation_id";

// ---------------------------------------------------------------------------
// Timelock parameters (issue #1045)
// ---------------------------------------------------------------------------

/// Seconds that must elapse between a nomination and the earliest moment it
/// can be accepted. 48 hours.
///
/// Deliberately a compile-time constant rather than admin-settable state: a
/// timelock an attacker with the admin key could shorten to zero is not a
/// timelock. Changing it requires a contract upgrade, which is itself an
/// admin-gated, publicly visible operation.
pub const ROTATION_DELAY_SECS: u64 = 172_800;

/// Seconds after the ETA during which the nominee may still accept. 7 days.
///
/// After this the nomination is dead and must be re-issued, so a stale
/// hand-over cannot be redeemed months later by a key that has since leaked.
pub const ROTATION_GRACE_SECS: u64 = 604_800;

// ---------------------------------------------------------------------------
// Pending rotation record
// ---------------------------------------------------------------------------

/// A nomination awaiting acceptance (issue #1045).
///
/// Replaces the bare `Address` that used to live in the pending slot. The
/// timestamps are absolute ledger seconds, resolved once at nomination time,
/// so acceptance never has to re-derive them from a mutable delay.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PendingRotation {
    /// Address nominated to become the next admin.
    pub new_admin: Address,
    /// Ledger timestamp at which the nomination was made.
    pub proposed_at: u64,
    /// Earliest ledger timestamp at which [`accept_admin`] may succeed.
    pub eta: u64,
    /// Ledger timestamp after which the nomination is dead.
    pub expires_at: u64,
    /// Unique, monotonically increasing id for this nomination.
    pub rotation_id: u64,
}

// ---------------------------------------------------------------------------
// TTL bookkeeping
// ---------------------------------------------------------------------------

/// Number of ledgers to extend instance storage TTL by on every admin write.
///
/// Soroban archives ledger entries (and the contract instance with them)
/// after a bounded period of inactivity. Bumping on every admin write
/// keeps the admin and pending slots well within the live window.
pub const BUMP_AMOUNT: u32 = 10_000;

/// Minimum TTL (in ledgers) required on the instance entry before we
/// trigger a bump. Below this threshold the instance is at risk of
/// archival before the next caller arrives.
pub const LIFETIME_THRESHOLD: u32 = 1_000;

// ---------------------------------------------------------------------------
// Error strings
// ---------------------------------------------------------------------------

const ERR_ALREADY_INITIALIZED: &str = "admin contract already initialized";
const ERR_NOT_INITIALIZED: &str = "admin contract not initialized";
const ERR_UNAUTHORIZED: &str = "unauthorized: caller is not admin";
const ERR_UNAUTHORIZED_PENDING: &str = "unauthorized: caller is not pending admin";
const ERR_NO_PENDING_ADMIN: &str = "no pending admin transfer";
/// Issue #1045: acceptance attempted before the timelock elapsed.
const ERR_TIMELOCK_NOT_ELAPSED: &str = "admin rotation timelock has not elapsed";
/// Issue #1045: acceptance attempted after the nomination expired.
const ERR_NOMINATION_EXPIRED: &str = "admin rotation nomination has expired";

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

/// Initialize the admin contract with an initial admin.
///
/// Can only be called once — re-initialization panics to prevent an attacker
/// from clobbering the admin slot after deploy.
///
/// # Arguments
/// * `env` — Soroban environment.
/// * `admin` — Address to install as the first admin.
///
/// # Auth
/// No `require_auth` is invoked: `init` is intended to be called once
/// during deployment before any user-facing identity is bound to the
/// contract, mirroring the convention used in `contracts/revenue_pool`.
///
/// # Panics
/// * `ERR_ALREADY_INITIALIZED` — `init` has been called before.
///
/// # Events
/// Emits `admin_init` with `(admin_init, admin)` topics and `()` as the
/// data payload. The initial admin identity is carried in topic[1], so
/// there is no redundant address in `data`.
pub fn init(env: &Env, admin: &Address) {
    if env.storage().instance().has(&Symbol::new(env, ADMIN_KEY)) {
        panic!("{}", ERR_ALREADY_INITIALIZED);
    }
    let inst = env.storage().instance();
    inst.set(&Symbol::new(env, ADMIN_KEY), admin);
    inst.extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);

    env.events()
        .publish((events::event_admin_init(env), admin), ());
}

// ---------------------------------------------------------------------------
// View — current and pending admin
// ---------------------------------------------------------------------------

/// Return the active admin address, or `None` if the contract has not been
/// initialized yet.
///
/// This is the authoritative lookup used by every other Callora contract
/// that integrates with admin lifecycle hooks.
pub fn get_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&Symbol::new(env, ADMIN_KEY))
}

/// Return the pending admin address, or `None` if no transfer is in progress.
///
/// Unchanged signature across the issue #1045 timelock work: the pending slot
/// now holds a [`PendingRotation`], and this view projects the address out of
/// it so existing integrations keep compiling and returning the same thing.
/// Callers that need the schedule should use [`get_pending_rotation`].
pub fn get_pending_admin(env: &Env) -> Option<Address> {
    get_pending_rotation(env).map(|rotation| rotation.new_admin)
}

/// Return the full pending rotation record, or `None` if no transfer is in
/// progress (issue #1045).
///
/// Exposes the timelock schedule — `eta`, `expires_at` and `rotation_id` — so
/// operators and indexers can see exactly when a nomination becomes
/// acceptable and when it dies, without replaying events.
pub fn get_pending_rotation(env: &Env) -> Option<PendingRotation> {
    env.storage()
        .instance()
        .get(&Symbol::new(env, PENDING_ADMIN_KEY))
}

/// Return the id of the most recent nomination, or `0` if none has ever been
/// made (issue #1045).
///
/// Monotonic for the life of the contract, so a caller can tell a fresh
/// nomination from a replay of one that was cancelled or expired.
pub fn get_rotation_id(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&Symbol::new(env, ROTATION_ID_KEY))
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Read the active admin or panic with [`ERR_NOT_INITIALIZED`].
///
/// Internal helper used by [`set_admin`] and [`cancel_admin_transfer`]
/// which both assume initialization. Promotion scenarios can use
/// [`get_admin`] directly when absence is a valid state (e.g. before init).
fn require_admin_addr(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&Symbol::new(env, ADMIN_KEY))
        .expect(ERR_NOT_INITIALIZED)
}

// ---------------------------------------------------------------------------
// Two-step admin rotation
// ---------------------------------------------------------------------------

/// Nominate a new admin. Only the **current** admin may call.
///
/// On success the pending slot is populated with `new_admin`. Calling
/// `set_admin` again before `accept_admin` simply replaces the pending
/// value (and emits another `admin_nominated` event), so the current admin
/// can correct an address typo without first cancelling.
///
/// # Arguments
/// * `env` — Soroban environment.
/// * `caller` — Must be the current admin; must authorize.
/// * `new_admin` — Address to nominate as the next admin. May be any
///   address; the contract does not validate the format here because
///   Soroban `Address` is already constrained by the host.
///
/// # Auth
/// `caller.require_auth()` is invoked **before** any storage read or write.
///
/// # Panics
/// * `ERR_NOT_INITIALIZED` — `init` has not been called yet.
/// * `ERR_UNAUTHORIZED` — `caller` is not the current admin.
///
/// # Timelock (issue #1045)
/// The nomination is stamped with `eta = now + `[`ROTATION_DELAY_SECS`] and
/// `expires_at = eta + `[`ROTATION_GRACE_SECS`]. Re-nominating **restarts**
/// the clock rather than inheriting the previous schedule — otherwise an
/// admin could park a harmless address in the slot, wait out the delay, and
/// then swap in an attacker-controlled address for immediate acceptance.
///
/// # Events
/// Emits exactly one `admin_nominated` event with
/// `(admin_nominated, caller)` topics and `(new_admin, eta, expires_at)` as
/// data, so an indexer learns the acceptance window from the nomination
/// itself.
pub fn set_admin(env: &Env, caller: &Address, new_admin: &Address) {
    caller.require_auth();

    // Authorization is resolved before anything is read for the write path or
    // written: an unauthorized caller changes no state and learns nothing
    // about the current rotation beyond the fact that they are not the admin.
    let current = require_admin_addr(env);
    if caller != &current {
        panic!("{}", ERR_UNAUTHORIZED);
    }

    let now = env.ledger().timestamp();
    // Saturating rather than wrapping: a clock near u64::MAX must not produce
    // an `eta` in the past, which would silently defeat the timelock.
    let eta = now.saturating_add(ROTATION_DELAY_SECS);
    let expires_at = eta.saturating_add(ROTATION_GRACE_SECS);

    let inst = env.storage().instance();
    let rotation_id = get_rotation_id(env).saturating_add(1);
    inst.set(&Symbol::new(env, ROTATION_ID_KEY), &rotation_id);
    inst.set(
        &Symbol::new(env, PENDING_ADMIN_KEY),
        &PendingRotation {
            new_admin: new_admin.clone(),
            proposed_at: now,
            eta,
            expires_at,
            rotation_id,
        },
    );
    inst.extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);

    env.events().publish(
        (events::event_admin_nominated(env), caller),
        (new_admin.clone(), eta, expires_at),
    );
}

/// Complete the admin transfer. Only the **pending** admin may call.
///
/// Promotes the pending admin to the active role, clears the pending slot
/// atomically, and emits a single `admin_changed` event describing the
/// before/after state for indexers.
///
/// # Arguments
/// * `env` — Soroban environment.
/// * `caller` — Must equal the pending admin address; must authorize.
///
/// # Auth
/// `caller.require_auth()` is invoked first; the address check follows.
///
/// # Panics
/// * `ERR_NO_PENDING_ADMIN` — no transfer is in progress.
/// * `ERR_UNAUTHORIZED_PENDING` — caller is not the pending admin.
/// * `ERR_NOMINATION_EXPIRED` — the acceptance window has closed (#1045).
/// * `ERR_TIMELOCK_NOT_ELAPSED` — the timelock has not elapsed yet (#1045).
///
/// The identity check runs **before** the timing checks, so an address that
/// is not the nominee cannot use the error it gets back to probe where the
/// rotation is in its schedule.
///
/// # Events
/// Emits exactly one `admin_changed` event with
/// `(admin_changed, caller)` topics — at this point `caller` is the
/// incoming admin who is about to become active — and the data payload is
/// `(previous_admin, new_admin)` so an indexer can record the full
/// handover from a single event.
pub fn accept_admin(env: &Env, caller: &Address) {
    caller.require_auth();

    let inst = env.storage().instance();
    let rotation: PendingRotation = inst
        .get(&Symbol::new(env, PENDING_ADMIN_KEY))
        .expect(ERR_NO_PENDING_ADMIN);

    // Order matters (issue #1045): identity first, schedule second. Every
    // check below is read-only, so any rejection leaves the pending slot,
    // the admin slot and the rotation counter exactly as they were found.
    if caller != &rotation.new_admin {
        panic!("{}", ERR_UNAUTHORIZED_PENDING);
    }

    let now = env.ledger().timestamp();
    if now > rotation.expires_at {
        panic!("{}", ERR_NOMINATION_EXPIRED);
    }
    if now < rotation.eta {
        panic!("{}", ERR_TIMELOCK_NOT_ELAPSED);
    }

    let previous_admin: Address = inst
        .get(&Symbol::new(env, ADMIN_KEY))
        .expect(ERR_NOT_INITIALIZED);

    let pending = rotation.new_admin;
    inst.set(&Symbol::new(env, ADMIN_KEY), &pending);
    // Clearing the pending slot in the same call is what makes a second
    // `accept_admin` with the same nomination fail with ERR_NO_PENDING_ADMIN
    // instead of re-running the promotion.
    inst.remove(&Symbol::new(env, PENDING_ADMIN_KEY));
    inst.extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);

    env.events().publish(
        (events::event_admin_changed(env), caller),
        (previous_admin, pending),
    );
}

/// Cancel a pending admin transfer. Only the **current** admin may call.
///
/// Removes the pending slot and emits `admin_cancelled` so indexers know
/// the prior nomination is no longer valid.
///
/// Callable at any point in the nomination's life (issue #1045) — before the
/// ETA, inside the acceptance window, or after expiry. Cancelling during the
/// timelock delay is the escape hatch the delay exists for: an admin who sees
/// a nomination they did not intend can revoke it before it can be accepted.
/// Cancelling after expiry is bookkeeping — the nomination is already dead —
/// but it clears the slot and emits the event that says so.
///
/// # Arguments
/// * `env` — Soroban environment.
/// * `caller` — Must be the current admin; must authorize.
///
/// # Auth
/// `caller.require_auth()` is invoked first.
///
/// # Panics
/// * `ERR_NOT_INITIALIZED` — `init` has not been called.
/// * `ERR_UNAUTHORIZED` — caller is not the current admin.
/// * `ERR_NO_PENDING_ADMIN` — there is no pending transfer to cancel.
///
/// # Events
/// Emits exactly one `admin_cancelled` event with
/// `(admin_cancelled, caller)` topics and the previously-pending admin
/// address as data so an indexer can record which specific address had
/// its nomination revoked.
pub fn cancel_admin_transfer(env: &Env, caller: &Address) {
    caller.require_auth();

    let current = require_admin_addr(env);
    if caller != &current {
        panic!("{}", ERR_UNAUTHORIZED);
    }

    let inst = env.storage().instance();
    let rotation: PendingRotation = inst
        .get(&Symbol::new(env, PENDING_ADMIN_KEY))
        .expect(ERR_NO_PENDING_ADMIN);

    inst.remove(&Symbol::new(env, PENDING_ADMIN_KEY));
    inst.extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);

    // Event data stays the bare address it has always been, so existing
    // indexers keep working; the schedule was already published with the
    // nomination.
    env.events().publish(
        (events::event_admin_cancelled(env), &current),
        rotation.new_admin,
    );
}

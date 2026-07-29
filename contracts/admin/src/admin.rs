//! Admin lifecycle module for the Callora Admin contract.
//!
//! Implements a two-step admin transfer pattern that mirrors the rest of the
//! Callora contracts so behaviour stays consistent across the workspace:
//!
//! 1. The **current** admin calls `set_admin(env, caller, new_admin)` to
//!    nominate a successor. The pending slot is populated and the
//!    `admin_nominated` event is emitted.
//! 2. The **nominated** admin calls `accept_admin(env, caller)` to claim the
//!    role. Storage is updated, the pending slot is cleared, and the
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
//! # Events
//!
//! | Function                  | Topic                | Topics                          | Data                              |
//! |---------------------------|----------------------|---------------------------------|-----------------------------------|
//! | `init`                    | `admin_init`         | `(topic, initial_admin)`        | `()`                              |
//! | `set_admin`               | `admin_nominated`    | `(topic, current_admin)`        | `pending_admin`                   |
//! | `accept_admin`            | `admin_changed`      | `(topic, new_admin)`            | `(previous_admin, new_admin)`     |
//! | `cancel_admin_transfer`   | `admin_cancelled`    | `(topic, current_admin)`        | `cancelled_pending_admin`         |
//!
//! Read-only helpers (`get_admin`, `get_pending_admin`) do **not** emit
//! events.

use soroban_sdk::{Address, Env, Symbol};

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
pub fn get_pending_admin(env: &Env) -> Option<Address> {
    env.storage()
        .instance()
        .get(&Symbol::new(env, PENDING_ADMIN_KEY))
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
/// # Events
/// Emits exactly one `admin_nominated` event with
/// `(admin_nominated, caller)` topics and `new_admin` as data.
pub fn set_admin(env: &Env, caller: &Address, new_admin: &Address) {
    caller.require_auth();

    let current = require_admin_addr(env);
    if caller != &current {
        panic!("{}", ERR_UNAUTHORIZED);
    }

    let inst = env.storage().instance();
    inst.set(&Symbol::new(env, PENDING_ADMIN_KEY), new_admin);
    inst.extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);

    env.events().publish(
        (events::event_admin_nominated(env), caller),
        new_admin.clone(),
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
    let pending: Address = inst
        .get(&Symbol::new(env, PENDING_ADMIN_KEY))
        .expect(ERR_NO_PENDING_ADMIN);

    if caller != &pending {
        panic!("{}", ERR_UNAUTHORIZED_PENDING);
    }

    let previous_admin: Address = inst
        .get(&Symbol::new(env, ADMIN_KEY))
        .expect(ERR_NOT_INITIALIZED);

    inst.set(&Symbol::new(env, ADMIN_KEY), &pending);
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
    let pending: Address = inst
        .get(&Symbol::new(env, PENDING_ADMIN_KEY))
        .expect(ERR_NO_PENDING_ADMIN);

    inst.remove(&Symbol::new(env, PENDING_ADMIN_KEY));
    inst.extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);

    env.events()
        .publish((events::event_admin_cancelled(env), &current), pending);
}

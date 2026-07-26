#![no_std]

//! # Callora Limits
//!
//! A standalone Soroban contract that maintains a registry of **per-token
//! transaction limits** for the Callora protocol.
//!
//! Each token contract address can be assigned an inclusive `[min, max]`
//! transaction band. Other Callora contracts (vault, settlement, revenue pool)
//! consult this registry to validate deposit, withdrawal, and payout amounts
//! against a single, centrally-managed source of truth instead of hard-coding
//! thresholds in each contract.
//!
//! ## What / How / Why
//!
//! * **What** — a mapping from token [`Address`] to a [`TokenLimit`] band plus
//!   admin-gated setters and permissionless read/check entrypoints.
//! * **How** — limits live in persistent storage keyed by
//!   [`StorageKey::Limit`]`(token)` with a 6-month TTL that is extended on every
//!   write. A two-step admin rotation guards all state-changing calls.
//! * **Why** — consolidating limit configuration into one auditable contract
//!   makes protocol-wide policy changes atomic and observable via events, and
//!   keeps overflow-safe validation logic in a single place.
//!
//! ## Security properties
//!
//! * `require_auth` on every state-changing entrypoint (`init`, `set_limit`,
//!   `remove_limit`, admin rotation, `upgrade`).
//! * Overflow-safe: [`CalloraLimits::check_amount`] uses no unchecked
//!   arithmetic and every discriminant comparison is saturating by construction.
//! * No raw `.unwrap()` in production paths — every fallible read returns a
//!   typed [`LimitsError`] or panics with an explicit invariant message.

#[cfg(test)]
extern crate std;

pub mod errors;
pub mod events;

use errors::LimitsError;
use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// TTL bump constants for persistent storage archival risk mitigation.
///
/// Soroban archives ledger entries after a period of inactivity.
///
/// - `BUMP_AMOUNT`: extend TTL by 3 110 400 ledgers (approx 6 months).
/// - `LIFETIME_THRESHOLD`: minimum remaining TTL that triggers a bump
///   (approx 1 day).
pub const BUMP_AMOUNT: u32 = 3_110_400;
pub const LIFETIME_THRESHOLD: u32 = 17_280;

/// Sentinel meaning "no upper bound" for a token's maximum limit.
///
/// When [`TokenLimit::max`] equals this value, [`CalloraLimits::check_amount`]
/// skips the upper-bound comparison entirely (fast path).
pub const UNLIMITED_MAX: i128 = i128::MAX;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Persistent and instance storage keys for the limits contract.
///
/// Each variant maps a logical key name to the underlying Soroban storage key,
/// avoiding accidental key collisions with raw strings.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum StorageKey {
    /// Instance: current admin [`Address`].
    Admin,
    /// Instance: pending admin [`Address`] for two-step rotation.
    PendingAdmin,
    /// Persistent: the [`TokenLimit`] band configured for a token address.
    Limit(Address),
}

/// An inclusive `[min, max]` transaction band for a single token.
///
/// An amount `a` is considered valid for the token when `min <= a <= max`.
/// Both bounds are denominated in the token's base units and are always
/// non-negative.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct TokenLimit {
    /// The token contract address this band applies to.
    pub token: Address,
    /// Inclusive minimum transaction amount (`>= 0`).
    pub min: i128,
    /// Inclusive maximum transaction amount (`>= min`). A value of
    /// [`UNLIMITED_MAX`] means there is no upper bound.
    pub max: i128,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct CalloraLimits;

#[contractimpl]
impl CalloraLimits {
    // -----------------------------------------------------------------------
    // Initialisation
    // -----------------------------------------------------------------------

    /// Initialize the limits contract with an admin address.
    ///
    /// **What.** Records the admin that is permitted to configure limits and
    /// rotate the admin role. **How.** Requires the admin's authorization and
    /// writes the address into instance storage exactly once. **Why.** A single
    /// initialization gate prevents the registry from being hijacked after
    /// deployment.
    ///
    /// Can only be called once. Subsequent calls return
    /// [`LimitsError::AlreadyInitialized`].
    ///
    /// # Parameters
    /// * `env` — Soroban environment.
    /// * `admin` — Address permitted to call admin-only entrypoints.
    ///
    /// # Errors
    /// * [`LimitsError::AlreadyInitialized`] — `init` was called more than once.
    ///
    /// # Events
    /// Emits `init` with `admin` as topic and `()` as data.
    pub fn init(env: Env, admin: Address) -> Result<(), LimitsError> {
        admin.require_auth();
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(LimitsError::AlreadyInitialized);
        }

        env.storage().instance().set(&StorageKey::Admin, &admin);

        env.events().publish((events::event_init(&env), admin), ());

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Admin helpers (internal)
    // -----------------------------------------------------------------------

    /// Read the current admin address from instance storage.
    ///
    /// Returns [`LimitsError::NotInitialized`] when no admin has been set
    /// (i.e., `init` was never called).
    fn admin(env: &Env) -> Result<Address, LimitsError> {
        env.storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(LimitsError::NotInitialized)
    }

    /// Verify that `caller` is the current admin.
    ///
    /// Consumes the caller's auth via `require_auth`, then checks instance
    /// storage for the admin address. Returns [`LimitsError::Unauthorized`]
    /// when the caller does not match.
    fn require_admin(env: &Env, caller: &Address) -> Result<(), LimitsError> {
        caller.require_auth();
        let admin = Self::admin(env)?;
        if caller != &admin {
            return Err(LimitsError::Unauthorized);
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Admin views
    // -----------------------------------------------------------------------

    /// Return the current admin address.
    ///
    /// **What.** Reads the configured admin. **Why.** Lets off-chain tooling and
    /// sibling contracts confirm who controls the limit registry.
    ///
    /// # Errors
    /// * [`LimitsError::NotInitialized`] — contract was never initialized.
    pub fn get_admin(env: Env) -> Result<Address, LimitsError> {
        Self::admin(&env)
    }

    /// Return the pending admin address for a two-step admin rotation, or
    /// `None` if no transfer is in progress.
    ///
    /// **What.** Exposes the nominee awaiting acceptance. **Why.** Allows a
    /// nominee to confirm a rotation is pending before calling
    /// [`CalloraLimits::accept_admin`].
    pub fn get_pending_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&StorageKey::PendingAdmin)
    }

    // -----------------------------------------------------------------------
    // Two-step admin rotation
    // -----------------------------------------------------------------------

    /// Nominate a new admin. Only the current admin may call.
    ///
    /// **What.** Records a pending admin. **How.** The nominee must call
    /// [`CalloraLimits::accept_admin`] to complete the transfer; until then the
    /// current admin retains full authority. **Why.** The two-step handover
    /// prevents transferring control to an unusable or mistyped address.
    ///
    /// # Parameters
    /// * `caller` — Must be the current admin; must authorize.
    /// * `new_admin` — Address of the proposed new admin.
    ///
    /// # Errors
    /// * [`LimitsError::Unauthorized`] — caller is not the current admin.
    /// * [`LimitsError::NotInitialized`] — contract not initialized.
    ///
    /// # Events
    /// Emits `admin_nominated` with `(current_admin, new_admin)`.
    pub fn set_admin(env: Env, caller: Address, new_admin: Address) -> Result<(), LimitsError> {
        Self::require_admin(&env, &caller)?;

        env.storage()
            .instance()
            .set(&StorageKey::PendingAdmin, &new_admin);

        env.events().publish(
            (
                events::event_admin_nominated(&env),
                Self::admin(&env)?,
                new_admin.clone(),
            ),
            new_admin,
        );

        Ok(())
    }

    /// Complete a pending admin transfer. Must be called by the nominated admin.
    ///
    /// **What.** Promotes the pending admin to admin. **How.** Requires the
    /// caller's auth and verifies it matches the recorded nominee before
    /// swapping the stored admin. **Why.** Only the nominee can finalize the
    /// handover, closing the two-step rotation loop.
    ///
    /// # Parameters
    /// * `caller` — Must be the pending admin; must authorize.
    ///
    /// # Errors
    /// * [`LimitsError::NotInitialized`] — contract not initialized.
    /// * Panics with `"no admin transfer pending"` — no nomination is in progress.
    /// * Panics with `"unauthorized: caller is not pending admin"` — wrong caller.
    ///
    /// # Events
    /// Emits `admin_accepted` with `(old_admin, new_admin)`.
    pub fn accept_admin(env: Env, caller: Address) -> Result<(), LimitsError> {
        caller.require_auth();

        let pending: Address = env
            .storage()
            .instance()
            .get(&StorageKey::PendingAdmin)
            .unwrap_or_else(|| panic!("no admin transfer pending"));

        if caller != pending {
            panic!("unauthorized: caller is not pending admin");
        }

        let old_admin = Self::admin(&env)?;
        let inst = env.storage().instance();
        inst.set(&StorageKey::Admin, &pending);
        inst.remove(&StorageKey::PendingAdmin);

        env.events().publish(
            (events::event_admin_accepted(&env), old_admin, pending.clone()),
            pending,
        );

        Ok(())
    }

    /// Cancel a pending admin transfer. Only the current admin may call.
    ///
    /// **What.** Clears the pending nomination. **Why.** Lets the current admin
    /// abort a rotation (e.g., a wrong nominee) before it is accepted.
    ///
    /// # Parameters
    /// * `caller` — Must be the current admin; must authorize.
    ///
    /// # Errors
    /// * [`LimitsError::Unauthorized`] — caller is not the current admin.
    /// * [`LimitsError::NotInitialized`] — contract not initialized.
    /// * Panics with `"no admin transfer pending"` — no nomination is in progress.
    ///
    /// # Events
    /// Emits `admin_cancelled` with `(admin, cancelled_pending)`.
    pub fn cancel_admin_transfer(env: Env, caller: Address) -> Result<(), LimitsError> {
        Self::require_admin(&env, &caller)?;

        let pending: Address = env
            .storage()
            .instance()
            .get(&StorageKey::PendingAdmin)
            .unwrap_or_else(|| panic!("no admin transfer pending"));

        env.storage().instance().remove(&StorageKey::PendingAdmin);

        env.events()
            .publish((events::event_admin_cancelled(&env), caller, pending), ());

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Limit configuration
    // -----------------------------------------------------------------------

    /// Create or update the `[min, max]` transaction band for a token.
    ///
    /// **What.** Stores an inclusive limit band for `token`. **How.** Validates
    /// that both bounds are non-negative and that `min <= max`, then writes the
    /// [`TokenLimit`] to persistent storage and extends its TTL. **Why.** A
    /// single admin-gated setter keeps protocol-wide limit policy consistent and
    /// auditable.
    ///
    /// Pass [`UNLIMITED_MAX`] as `max` to configure a minimum with no upper
    /// bound.
    ///
    /// # Parameters
    /// * `caller` — Must be the current admin; must authorize.
    /// * `token` — Token contract address the band applies to.
    /// * `min` — Inclusive minimum amount in token base units; must be `>= 0`.
    /// * `max` — Inclusive maximum amount in token base units; must be `>= min`.
    ///
    /// # Errors
    /// * [`LimitsError::Unauthorized`] — caller is not the current admin.
    /// * [`LimitsError::NotInitialized`] — contract not initialized.
    /// * [`LimitsError::AmountNegative`] — `min` or `max` is negative.
    /// * [`LimitsError::InvalidLimit`] — `max < min`.
    ///
    /// # Events
    /// Emits `limit_set` with `token` as topic and the full [`TokenLimit`] as
    /// data.
    pub fn set_limit(
        env: Env,
        caller: Address,
        token: Address,
        min: i128,
        max: i128,
    ) -> Result<(), LimitsError> {
        Self::require_admin(&env, &caller)?;

        if min < 0 || max < 0 {
            return Err(LimitsError::AmountNegative);
        }
        if max < min {
            return Err(LimitsError::InvalidLimit);
        }

        let limit = TokenLimit {
            token: token.clone(),
            min,
            max,
        };

        let key = StorageKey::Limit(token.clone());
        env.storage().persistent().set(&key, &limit);
        env.storage()
            .persistent()
            .extend_ttl(&key, LIFETIME_THRESHOLD, BUMP_AMOUNT);

        env.events()
            .publish((events::event_limit_set(&env), token), limit);

        Ok(())
    }

    /// Remove the configured transaction band for a token.
    ///
    /// **What.** Deletes any [`TokenLimit`] stored for `token`. **How.** Requires
    /// admin auth and removes the persistent entry; a subsequent
    /// [`CalloraLimits::check_amount`] treats the token as unrestricted.
    /// **Why.** Allows retiring a policy without leaving a stale band in place.
    ///
    /// Removing a token that has no configured limit is a no-op that still
    /// emits the `limit_removed` event for observability.
    ///
    /// # Parameters
    /// * `caller` — Must be the current admin; must authorize.
    /// * `token` — Token contract address whose band is cleared.
    ///
    /// # Errors
    /// * [`LimitsError::Unauthorized`] — caller is not the current admin.
    /// * [`LimitsError::NotInitialized`] — contract not initialized.
    ///
    /// # Events
    /// Emits `limit_removed` with `token` as topic and `()` as data.
    pub fn remove_limit(env: Env, caller: Address, token: Address) -> Result<(), LimitsError> {
        Self::require_admin(&env, &caller)?;

        env.storage()
            .persistent()
            .remove(&StorageKey::Limit(token.clone()));

        env.events()
            .publish((events::event_limit_removed(&env), token), ());

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Limit queries (read-only)
    // -----------------------------------------------------------------------

    /// Return the configured [`TokenLimit`] band for a token, if any.
    ///
    /// **What.** Reads the stored band. **Why.** Lets callers inspect the exact
    /// `[min, max]` policy before submitting a transaction. Returns `None` when
    /// no band has been configured (the token is unrestricted).
    ///
    /// # Parameters
    /// * `token` — Token contract address to look up.
    pub fn get_limit(env: Env, token: Address) -> Option<TokenLimit> {
        env.storage().persistent().get(&StorageKey::Limit(token))
    }

    /// Return whether a token has a configured transaction band.
    ///
    /// **What.** A cheap existence check. **Why.** Callers can branch on whether
    /// validation is required without deserializing the full [`TokenLimit`].
    ///
    /// # Parameters
    /// * `token` — Token contract address to test.
    pub fn has_limit(env: Env, token: Address) -> bool {
        env.storage().persistent().has(&StorageKey::Limit(token))
    }

    /// Validate `amount` against the token's configured `[min, max]` band.
    ///
    /// **What.** The primary read entrypoint sibling contracts call before
    /// moving funds. **How.** Rejects negative amounts, then — when a band is
    /// configured — enforces `min <= amount <= max`. When no band exists the
    /// amount is accepted (fast path), so tokens are unrestricted until an admin
    /// opts them in. **Why.** Centralizing the comparison keeps overflow-safe,
    /// consistently-typed validation in one audited place.
    ///
    /// This function performs no arithmetic on `amount` and therefore cannot
    /// overflow; it only compares against the stored bounds.
    ///
    /// # Parameters
    /// * `token` — Token contract address to validate against.
    /// * `amount` — Proposed transaction amount in token base units.
    ///
    /// # Errors
    /// * [`LimitsError::AmountNegative`] — `amount` is negative.
    /// * [`LimitsError::BelowMinimum`] — `amount` is below the configured `min`.
    /// * [`LimitsError::AboveMaximum`] — `amount` exceeds the configured `max`.
    pub fn check_amount(env: Env, token: Address, amount: i128) -> Result<(), LimitsError> {
        if amount < 0 {
            return Err(LimitsError::AmountNegative);
        }

        let limit: Option<TokenLimit> =
            env.storage().persistent().get(&StorageKey::Limit(token));

        match limit {
            None => Ok(()),
            Some(limit) => {
                if amount < limit.min {
                    return Err(LimitsError::BelowMinimum);
                }
                if limit.max != UNLIMITED_MAX && amount > limit.max {
                    return Err(LimitsError::AboveMaximum);
                }
                Ok(())
            }
        }
    }

    // -----------------------------------------------------------------------
    // Upgrade
    // -----------------------------------------------------------------------

    /// Admin-gated contract upgrade. Replaces the WASM with `new_wasm_hash`.
    ///
    /// **What.** Swaps the contract's executable code. **How.** Requires admin
    /// auth, then calls the deployer to install the new WASM hash. **Why.**
    /// Lets the protocol ship fixes and new policy logic without redeploying and
    /// re-registering the registry.
    ///
    /// # Parameters
    /// * `caller` — Must be the current admin; must authorize.
    /// * `new_wasm_hash` — The 32-byte hash of the replacement WASM.
    ///
    /// # Errors
    /// * [`LimitsError::Unauthorized`] — caller is not the current admin.
    /// * [`LimitsError::NotInitialized`] — contract not initialized.
    ///
    /// # Events
    /// Emits `upgraded` with the admin as topic and the new WASM hash as data.
    pub fn upgrade(env: Env, caller: Address, new_wasm_hash: BytesN<32>) -> Result<(), LimitsError> {
        Self::require_admin(&env, &caller)?;

        env.deployer()
            .update_current_contract_wasm(new_wasm_hash.clone());

        env.events().publish(
            (events::event_upgraded(&env), Self::admin(&env)?),
            new_wasm_hash,
        );

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Test modules
// ---------------------------------------------------------------------------

#[cfg(test)]
mod test;

#[cfg(test)]
mod rustdoc_tests {
    #[test]
    fn every_public_fn_in_lib_has_rustdoc() {
        let source = include_str!("lib.rs")
            .split("// ---------------------------------------------------------------------------\n// Test modules")
            .next()
            .expect("lib.rs contains test module marker");
        let lines: std::vec::Vec<&str> = source.lines().collect();

        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if !(trimmed.starts_with("pub fn ")
                || trimmed.starts_with("pub(crate) fn ")
                || trimmed.starts_with("pub(super) fn "))
            {
                continue;
            }

            let has_rustdoc = lines[..idx]
                .iter()
                .rev()
                .map(|candidate| candidate.trim_start())
                .find(|candidate| !candidate.is_empty())
                .map(|candidate| candidate.starts_with("///"))
                .unwrap_or(false);

            assert!(
                has_rustdoc,
                "public function on line {} is missing /// rustdoc: {}",
                idx + 1,
                trimmed
            );
        }
    }
}

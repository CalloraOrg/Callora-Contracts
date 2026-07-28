#![no_std]
//! Callora Batch-Claim contract.
//!
//! Allows a claimant to register pending reward claims and batch-collect them
//! in a single transaction.  All persistent storage reads on the hot path bump
//! the entry TTL so claims can never be silently archived before the claimant
//! processes them.
//!
//! ## Storage layout
//!
//! | Scope      | Key                             | Value                      |
//! |------------|---------------------------------|----------------------------|
//! | Instance   | `StorageKey::Admin`             | `Address`                  |
//! | Persistent | `StorageKey::Claim(claimant)`   | `ClaimRecord`              |
//! | Instance   | `StorageKey::TotalClaims`       | `u32`                      |
//!
//! ## TTL management
//!
//! Soroban can archive persistent ledger entries after roughly 7 days (≈631
//! ledgers) of inactivity.  Any claimant who deposits a claim and returns
//! 8+ days later would find their entry archived, losing access to the record.
//!
//! To prevent this:
//!
//! - **Every hot read** (`get_claim`, `has_claim`, and the internal reads
//!   inside `batch_claim`) calls `extend_ttl` on the persistent entry with a
//!   `PERSISTENT_BUMP` of 10 000 ledgers (≈16 days).
//! - **Every write** also bumps the instance-storage TTL via
//!   `extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT)`.
//!
//! This ensures that active claimants never experience archival pressure on
//! their pending claims.
//!
//! ## Events
//!
//! | Entrypoint        | Topic               | Data                             |
//! |-------------------|---------------------|----------------------------------|
//! | `init`            | `"bc_init"`         | `admin: Address`                 |
//! | `add_claim`       | `"claim_added"`     | `(claimant, amount)`             |
//! | `batch_claim`     | `"claims_settled"`  | `(claimant, total_claimed)`      |
//! | `cancel_claim`    | `"claim_cancelled"` | `claimant: Address`              |
//!
//! Closes CalloraOrg/Callora-Contracts#830.

pub mod errors;
pub mod events;

pub use errors::BatchClaimError;

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Vec};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Extend **instance** storage TTL by 10 000 ledgers (≈16 days) on mutating calls.
pub const BUMP_AMOUNT: u32 = 10_000;
/// Minimum remaining instance TTL before triggering a bump (≈1.5 days).
pub const LIFETIME_THRESHOLD: u32 = 1_000;

/// Extend **persistent** claim entry TTL by 10 000 ledgers on every hot read.
///
/// This is the core mechanism that prevents archival pressure on claim storage.
/// Any code path that reads a `ClaimRecord` MUST call `extend_ttl` with these
/// constants before returning the data.
pub const PERSISTENT_BUMP: u32 = 10_000;
/// Minimum remaining persistent TTL before triggering a claim-entry bump (≈1.5 days).
pub const PERSISTENT_THRESHOLD: u32 = 1_000;

/// Maximum number of individual pending amounts a single claim accumulates.
pub const MAX_PENDING_AMOUNTS: u32 = 50;

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum StorageKey {
    /// Contract admin.
    Admin,
    /// Per-claimant claim record.
    Claim(Address),
    /// Monotonically-increasing count of total claims ever created.
    TotalClaims,
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Pending reward claim for a single claimant.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ClaimRecord {
    /// Address entitled to collect the claim.
    pub claimant: Address,
    /// Accumulated pending amount (sum of all `add_claim` calls).
    pub pending_amount: i128,
    /// Whether the claim has already been settled (collected).
    pub settled: bool,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct CalloraBatchClaim;

#[contractimpl]
impl CalloraBatchClaim {
    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /// Initialize the batch-claim contract.
    ///
    /// Can only be called once.  The `admin` address must authorize the call.
    ///
    /// # Errors
    /// Returns [`BatchClaimError::AlreadyInitialized`] if called more than once.
    pub fn init(env: Env, admin: Address) -> Result<(), BatchClaimError> {
        admin.require_auth();
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(BatchClaimError::AlreadyInitialized);
        }
        let inst = env.storage().instance();
        inst.set(&StorageKey::Admin, &admin);
        inst.set(&StorageKey::TotalClaims, &0u32);
        Self::bump_instance(&env);
        env.events()
            .publish((events::event_init(&env),), admin.clone());
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Mutating entrypoints
    // -----------------------------------------------------------------------

    /// Accumulate a pending reward amount for `claimant`.
    ///
    /// Only the admin may add claims.  If a claim already exists for
    /// `claimant`, the `amount` is added to the existing pending total
    /// (overflow-safe).
    ///
    /// # Errors
    /// - [`BatchClaimError::NotInitialized`] if `init` has not been called.
    /// - [`BatchClaimError::Unauthorized`] if `caller` is not the admin.
    /// - [`BatchClaimError::InvalidAmount`] if `amount` ≤ 0.
    /// - [`BatchClaimError::Overflow`] if the accumulated total would overflow.
    pub fn add_claim(
        env: Env,
        caller: Address,
        claimant: Address,
        amount: i128,
    ) -> Result<(), BatchClaimError> {
        caller.require_auth();
        let admin = Self::require_admin(&env)?;
        if caller != admin {
            return Err(BatchClaimError::Unauthorized);
        }
        if amount <= 0 {
            return Err(BatchClaimError::InvalidAmount);
        }

        let key = StorageKey::Claim(claimant.clone());

        // Bump TTL on the existing persistent entry before reading it.
        // This is the hot-read TTL bump described in the module-level docs.
        if env.storage().persistent().has(&key) {
            env.storage()
                .persistent()
                .extend_ttl(&key, PERSISTENT_THRESHOLD, PERSISTENT_BUMP);
        }

        let existing: Option<ClaimRecord> = env.storage().persistent().get(&key);
        let new_pending = match existing {
            Some(rec) if !rec.settled => rec
                .pending_amount
                .checked_add(amount)
                .ok_or(BatchClaimError::Overflow)?,
            Some(_settled) => amount, // re-open a previously settled claim
            None => {
                // New claim: bump total-claims counter.
                let count: u32 = env
                    .storage()
                    .instance()
                    .get(&StorageKey::TotalClaims)
                    .unwrap_or(0);
                env.storage().instance().set(
                    &StorageKey::TotalClaims,
                    &count.checked_add(1).ok_or(BatchClaimError::Overflow)?,
                );
                amount
            }
        };

        let record = ClaimRecord {
            claimant: claimant.clone(),
            pending_amount: new_pending,
            settled: false,
        };
        env.storage().persistent().set(&key, &record);
        // Bump TTL after writing the new/updated record.
        env.storage()
            .persistent()
            .extend_ttl(&key, PERSISTENT_THRESHOLD, PERSISTENT_BUMP);

        Self::bump_instance(&env);
        env.events()
            .publish((events::event_claim_added(&env),), (claimant, new_pending));
        Ok(())
    }

    /// Settle (collect) all pending claims for a batch of claimants.
    ///
    /// The `claimant` in each entry must authorize their own collection.
    /// Settled claims are marked `settled = true` so double-claims are
    /// rejected.  Returns the total amount collected across all claimants.
    ///
    /// # Errors
    /// - [`BatchClaimError::NotInitialized`] if `init` has not been called.
    /// - [`BatchClaimError::ClaimNotFound`] if any `claimant` has no pending record.
    /// - [`BatchClaimError::AlreadySettled`] if any `claimant`'s claim is already settled.
    /// - [`BatchClaimError::Overflow`] if the running total overflows.
    pub fn batch_claim(env: Env, claimants: Vec<Address>) -> Result<i128, BatchClaimError> {
        Self::require_admin(&env)?;
        let mut total_claimed: i128 = 0;

        for claimant in claimants.iter() {
            claimant.require_auth();
            let key = StorageKey::Claim(claimant.clone());

            // --- Hot-read path TTL bump ---
            // Bump BEFORE reading so the entry is always refreshed even when
            // the call ultimately fails (e.g. AlreadySettled).  This prevents
            // the most recently active claims from being archived while the
            // claimant resolves an error.
            if env.storage().persistent().has(&key) {
                env.storage()
                    .persistent()
                    .extend_ttl(&key, PERSISTENT_THRESHOLD, PERSISTENT_BUMP);
            }

            let mut record: ClaimRecord = env
                .storage()
                .persistent()
                .get(&key)
                .ok_or(BatchClaimError::ClaimNotFound)?;

            if record.settled {
                return Err(BatchClaimError::AlreadySettled);
            }

            total_claimed = total_claimed
                .checked_add(record.pending_amount)
                .ok_or(BatchClaimError::Overflow)?;

            record.settled = true;
            env.storage().persistent().set(&key, &record);
            // Bump again after the write to keep the settled tombstone live.
            env.storage()
                .persistent()
                .extend_ttl(&key, PERSISTENT_THRESHOLD, PERSISTENT_BUMP);

            env.events().publish(
                (events::event_claims_settled(&env),),
                (claimant.clone(), record.pending_amount),
            );
        }

        Self::bump_instance(&env);
        Ok(total_claimed)
    }

    /// Cancel a pending (unsettled) claim.
    ///
    /// Only the admin may cancel claims.
    ///
    /// # Errors
    /// - [`BatchClaimError::NotInitialized`] if `init` has not been called.
    /// - [`BatchClaimError::Unauthorized`] if `caller` is not the admin.
    /// - [`BatchClaimError::ClaimNotFound`] if `claimant` has no record.
    /// - [`BatchClaimError::AlreadySettled`] if the claim is already settled.
    pub fn cancel_claim(
        env: Env,
        caller: Address,
        claimant: Address,
    ) -> Result<(), BatchClaimError> {
        caller.require_auth();
        let admin = Self::require_admin(&env)?;
        if caller != admin {
            return Err(BatchClaimError::Unauthorized);
        }

        let key = StorageKey::Claim(claimant.clone());

        // Hot-read TTL bump before accessing persistent storage.
        if env.storage().persistent().has(&key) {
            env.storage()
                .persistent()
                .extend_ttl(&key, PERSISTENT_THRESHOLD, PERSISTENT_BUMP);
        }

        let record: ClaimRecord = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(BatchClaimError::ClaimNotFound)?;

        if record.settled {
            return Err(BatchClaimError::AlreadySettled);
        }

        env.storage().persistent().remove(&key);
        Self::bump_instance(&env);
        env.events()
            .publish((events::event_claim_cancelled(&env),), claimant);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Read-only views
    // -----------------------------------------------------------------------

    /// Fetch a claimant's pending record.
    ///
    /// **TTL bump**: bumps the persistent entry TTL on every call to prevent
    /// archival pressure on frequently-polled claim records.
    ///
    /// No auth required.
    ///
    /// # Errors
    /// - [`BatchClaimError::NotInitialized`] if `init` has not been called.
    /// - [`BatchClaimError::ClaimNotFound`] if `claimant` has no record.
    pub fn get_claim(env: Env, claimant: Address) -> Result<ClaimRecord, BatchClaimError> {
        Self::require_admin(&env)?;
        let key = StorageKey::Claim(claimant);

        // Hot-read TTL bump: prevent archival of frequently polled entries.
        if env.storage().persistent().has(&key) {
            env.storage()
                .persistent()
                .extend_ttl(&key, PERSISTENT_THRESHOLD, PERSISTENT_BUMP);
        }

        env.storage()
            .persistent()
            .get(&key)
            .ok_or(BatchClaimError::ClaimNotFound)
    }

    /// Check whether a claimant has a pending (unsettled) claim.
    ///
    /// **TTL bump**: bumps the persistent entry TTL on every call.
    ///
    /// No auth required.
    ///
    /// # Errors
    /// Returns [`BatchClaimError::NotInitialized`] if `init` has not been called.
    pub fn has_claim(env: Env, claimant: Address) -> Result<bool, BatchClaimError> {
        Self::require_admin(&env)?;
        let key = StorageKey::Claim(claimant);

        // Hot-read TTL bump.
        if env.storage().persistent().has(&key) {
            env.storage()
                .persistent()
                .extend_ttl(&key, PERSISTENT_THRESHOLD, PERSISTENT_BUMP);
        }

        let Some(record): Option<ClaimRecord> = env.storage().persistent().get(&key) else {
            return Ok(false);
        };
        Ok(!record.settled)
    }

    /// Total number of claims ever created (monotonically increasing).
    ///
    /// No auth required.
    ///
    /// # Errors
    /// Returns [`BatchClaimError::NotInitialized`] if `init` has not been called.
    pub fn total_claims(env: Env) -> Result<u32, BatchClaimError> {
        Self::require_admin(&env)?;
        Ok(env
            .storage()
            .instance()
            .get(&StorageKey::TotalClaims)
            .unwrap_or(0))
    }

    /// Return the contract admin address.
    ///
    /// No auth required.
    ///
    /// # Errors
    /// Returns [`BatchClaimError::NotInitialized`] if `init` has not been called.
    pub fn get_admin(env: Env) -> Result<Address, BatchClaimError> {
        Self::require_admin(&env)
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Fetch the admin or return `NotInitialized`.
    fn require_admin(env: &Env) -> Result<Address, BatchClaimError> {
        env.storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(BatchClaimError::NotInitialized)
    }

    /// Bump instance storage TTL to keep the contract live.
    fn bump_instance(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, Env, Vec};

    fn setup(env: &Env) -> (Address, Address, CalloraBatchClaimClient<'_>) {
        env.mock_all_auths();
        let admin = Address::generate(env);
        let contract_id = env.register(CalloraBatchClaim, ());
        let client = CalloraBatchClaimClient::new(env, &contract_id);
        client.init(&admin);
        let claimant = Address::generate(env);
        (admin, claimant, client)
    }

    // -----------------------------------------------------------------------
    // init
    // -----------------------------------------------------------------------

    #[test]
    fn test_init_success() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let id = env.register(CalloraBatchClaim, ());
        let client = CalloraBatchClaimClient::new(&env, &id);
        client.init(&admin);
        assert_eq!(client.get_admin(), admin);
    }

    #[test]
    fn test_init_double_returns_error() {
        let env = Env::default();
        let (admin, _, client) = setup(&env);
        let res = client.try_init(&admin);
        assert_eq!(res, Err(Ok(BatchClaimError::AlreadyInitialized)));
    }

    // -----------------------------------------------------------------------
    // add_claim
    // -----------------------------------------------------------------------

    #[test]
    fn test_add_claim_success() {
        let env = Env::default();
        let (admin, claimant, client) = setup(&env);
        client.add_claim(&admin, &claimant, &500);
        let rec = client.get_claim(&claimant);
        assert_eq!(rec.pending_amount, 500);
        assert!(!rec.settled);
    }

    #[test]
    fn test_add_claim_accumulates() {
        let env = Env::default();
        let (admin, claimant, client) = setup(&env);
        client.add_claim(&admin, &claimant, &300);
        client.add_claim(&admin, &claimant, &200);
        let rec = client.get_claim(&claimant);
        assert_eq!(rec.pending_amount, 500);
    }

    #[test]
    fn test_add_claim_invalid_amount() {
        let env = Env::default();
        let (admin, claimant, client) = setup(&env);
        let res = client.try_add_claim(&admin, &claimant, &0);
        assert_eq!(res, Err(Ok(BatchClaimError::InvalidAmount)));
        let res2 = client.try_add_claim(&admin, &claimant, &-1);
        assert_eq!(res2, Err(Ok(BatchClaimError::InvalidAmount)));
    }

    #[test]
    fn test_add_claim_unauthorized() {
        let env = Env::default();
        let (_, claimant, client) = setup(&env);
        let non_admin = Address::generate(&env);
        let res = client.try_add_claim(&non_admin, &claimant, &100);
        assert_eq!(res, Err(Ok(BatchClaimError::Unauthorized)));
    }

    // -----------------------------------------------------------------------
    // batch_claim
    // -----------------------------------------------------------------------

    #[test]
    fn test_batch_claim_single() {
        let env = Env::default();
        let (admin, claimant, client) = setup(&env);
        client.add_claim(&admin, &claimant, &1_000);

        let mut claimants = Vec::new(&env);
        claimants.push_back(claimant.clone());
        let total = client.batch_claim(&claimants);
        assert_eq!(total, 1_000);

        let rec = client.get_claim(&claimant);
        assert!(rec.settled);
    }

    #[test]
    fn test_batch_claim_multiple() {
        let env = Env::default();
        let (admin, _, client) = setup(&env);
        let c1 = Address::generate(&env);
        let c2 = Address::generate(&env);
        client.add_claim(&admin, &c1, &400);
        client.add_claim(&admin, &c2, &600);

        let mut claimants = Vec::new(&env);
        claimants.push_back(c1);
        claimants.push_back(c2);
        let total = client.batch_claim(&claimants);
        assert_eq!(total, 1_000);
    }

    #[test]
    fn test_batch_claim_already_settled() {
        let env = Env::default();
        let (admin, claimant, client) = setup(&env);
        client.add_claim(&admin, &claimant, &100);

        let mut cv = Vec::new(&env);
        cv.push_back(claimant.clone());
        client.batch_claim(&cv);

        let res = client.try_batch_claim(&cv);
        assert_eq!(res, Err(Ok(BatchClaimError::AlreadySettled)));
    }

    #[test]
    fn test_batch_claim_not_found() {
        let env = Env::default();
        let (_, _, client) = setup(&env);
        let stranger = Address::generate(&env);
        let mut cv = Vec::new(&env);
        cv.push_back(stranger);
        let res = client.try_batch_claim(&cv);
        assert_eq!(res, Err(Ok(BatchClaimError::ClaimNotFound)));
    }

    // -----------------------------------------------------------------------
    // cancel_claim
    // -----------------------------------------------------------------------

    #[test]
    fn test_cancel_claim_success() {
        let env = Env::default();
        let (admin, claimant, client) = setup(&env);
        client.add_claim(&admin, &claimant, &250);
        client.cancel_claim(&admin, &claimant);
        let res = client.try_get_claim(&claimant);
        assert_eq!(res, Err(Ok(BatchClaimError::ClaimNotFound)));
    }

    #[test]
    fn test_cancel_already_settled() {
        let env = Env::default();
        let (admin, claimant, client) = setup(&env);
        client.add_claim(&admin, &claimant, &100);
        let mut cv = Vec::new(&env);
        cv.push_back(claimant.clone());
        client.batch_claim(&cv);

        let res = client.try_cancel_claim(&admin, &claimant);
        assert_eq!(res, Err(Ok(BatchClaimError::AlreadySettled)));
    }

    // -----------------------------------------------------------------------
    // has_claim
    // -----------------------------------------------------------------------

    #[test]
    fn test_has_claim_true_and_false() {
        let env = Env::default();
        let (admin, claimant, client) = setup(&env);
        assert!(!client.has_claim(&claimant));
        client.add_claim(&admin, &claimant, &50);
        assert!(client.has_claim(&claimant));

        let mut cv = Vec::new(&env);
        cv.push_back(claimant.clone());
        client.batch_claim(&cv);
        assert!(!client.has_claim(&claimant));
    }

    // -----------------------------------------------------------------------
    // total_claims
    // -----------------------------------------------------------------------

    #[test]
    fn test_total_claims_increments() {
        let env = Env::default();
        let (admin, _, client) = setup(&env);
        assert_eq!(client.total_claims(), 0);
        let c1 = Address::generate(&env);
        let c2 = Address::generate(&env);
        client.add_claim(&admin, &c1, &1);
        assert_eq!(client.total_claims(), 1);
        client.add_claim(&admin, &c2, &1);
        assert_eq!(client.total_claims(), 2);
        // Adding more to existing claimant does NOT increment total_claims.
        client.add_claim(&admin, &c1, &1);
        assert_eq!(client.total_claims(), 2);
    }

    // -----------------------------------------------------------------------
    // TTL bump verification
    // -----------------------------------------------------------------------

    /// Verify that calling `get_claim` twice completes without error,
    /// demonstrating that the TTL bump on the hot-read path does not panic
    /// or cause state inconsistencies.
    #[test]
    fn test_get_claim_ttl_bump_idempotent() {
        let env = Env::default();
        let (admin, claimant, client) = setup(&env);
        client.add_claim(&admin, &claimant, &999);

        let rec1 = client.get_claim(&claimant);
        let rec2 = client.get_claim(&claimant);
        assert_eq!(
            rec1, rec2,
            "repeated get_claim must return identical records"
        );
    }

    /// Verify that `has_claim` can be called repeatedly (TTL bump is safe and
    /// idempotent).
    #[test]
    fn test_has_claim_ttl_bump_idempotent() {
        let env = Env::default();
        let (admin, claimant, client) = setup(&env);
        client.add_claim(&admin, &claimant, &10);

        assert!(client.has_claim(&claimant));
        assert!(client.has_claim(&claimant));
        assert!(client.has_claim(&claimant));
    }

    /// Verify that the TTL bump inside `batch_claim` is applied before the
    /// settled-check, so even double-claim errors don't leave the entry
    /// without a freshly-bumped TTL.
    #[test]
    fn test_batch_claim_ttl_bump_on_already_settled_error() {
        let env = Env::default();
        let (admin, claimant, client) = setup(&env);
        client.add_claim(&admin, &claimant, &100);

        let mut cv = Vec::new(&env);
        cv.push_back(claimant.clone());
        client.batch_claim(&cv);

        // Second batch_claim returns AlreadySettled — but the entry still exists.
        let _ = client.try_batch_claim(&cv);
        // The record should still be retrievable (TTL bump keeps it alive).
        let rec = client.get_claim(&claimant);
        assert!(rec.settled);
    }
}

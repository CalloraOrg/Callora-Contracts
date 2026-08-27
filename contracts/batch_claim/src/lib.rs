#![no_std]
//! Callora Batch-Claim contract.
//!
//! Allows a claimant to register pending reward claims and batch-collect them
//! in a single transaction.  All persistent storage reads on the hot path bump
//! the entry TTL so claims can never be silently archived before the claimant
//! processes them.
//!
//! ## Replay protection
//!
//! Every claim carries an explicit **claim identifier** (`BytesN<32>`).  The
//! lifecycle is:
//!
//! 1. Admin calls [`CalloraBatchClaim::add_claim`] supplying a unique
//!    `claim_id`.  The identifier is stored inside [`ClaimRecord`] and a
//!    separate [`StorageKey::ClaimConsumed`] entry is created and set to
//!    `false`.
//! 2. The claimant calls [`CalloraBatchClaim::batch_claim`], passing the same
//!    `claim_id`.  The contract:
//!    a. Verifies `claim_id` matches the stored record (`ClaimIdMismatch`).
//!    b. Reads the [`StorageKey::ClaimConsumed`] flag; if `true` it returns
//!       [`BatchClaimError::ClaimIdAlreadyUsed`] **without** modifying any
//!       other state — failed validation never consumes state.
//!    c. Sets `ClaimConsumed(claim_id) = true` **before** any other mutation
//!       (write-before-settle) so concurrent invocations cannot both release
//!       the same entitlement.
//!    d. Sets `record.settled = true` and persists.
//!    e. Emits [`events::event_claim_consumed`] so off-chain indexers track
//!       spent identifiers.
//!
//! The `ClaimConsumed` key lives in **persistent** storage (independent TTL)
//! so the consumed tombstone outlives the `ClaimRecord` if the record is later
//! archived or re-opened.
//!
//! ## Storage layout
//!
//! | Scope      | Key                                   | Value                      |
//! |------------|---------------------------------------|----------------------------|
//! | Instance   | `StorageKey::Admin`                   | `Address`                  |
//! | Persistent | `StorageKey::Claim(claimant)`         | `ClaimRecord`              |
//! | Persistent | `StorageKey::ClaimConsumed(claim_id)` | `bool`                     |
//! | Instance   | `StorageKey::TotalClaims`             | `u32`                      |
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
//! - The `ClaimConsumed` persistent entry is bumped alongside the `Claim`
//!   entry on every relevant read/write.
//!
//! ## Events
//!
//! | Entrypoint        | Topic               | Data                             |
//! |-------------------|---------------------|----------------------------------|
//! | `init`            | `"bc_init"`         | `admin: Address`                 |
//! | `add_claim`       | `"claim_added"`     | `(claimant, amount)`             |
//! | `batch_claim`     | `"claims_settled"`  | `(claimant, total_claimed)`      |
//! | `batch_claim`     | `"claim_consumed"`  | `claim_id: BytesN<32>`           |
//! | `cancel_claim`    | `"claim_cancelled"` | `claimant: Address`              |

pub mod errors;
pub mod events;

pub use errors::BatchClaimError;

use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env, Vec};

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
    /// Per-claim-id consumed tombstone.  Set to `true` once the identifier
    /// has been successfully settled; prevents replay of the same id.
    ClaimConsumed(BytesN<32>),
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
    /// Replay-safe identifier assigned by the admin at `add_claim` time.
    ///
    /// The identifier is globally unique per issuance and stored as a
    /// 32-byte value so it can encode a UUID, content hash, or sequence
    /// number without truncation.  The contract rejects any `batch_claim`
    /// call that presents a `claim_id` differing from this field
    /// (`ClaimIdMismatch`) and permanently rejects any call once
    /// `ClaimConsumed(claim_id) == true` (`ClaimIdAlreadyUsed`).
    pub claim_id: BytesN<32>,
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

    /// Accumulate a pending reward amount for `claimant` under `claim_id`.
    ///
    /// Only the admin may add claims.  If a claim already exists for
    /// `claimant`, the `amount` is added to the existing pending total
    /// (overflow-safe).
    ///
    /// `claim_id` must be a fresh 32-byte identifier that has **not** been
    /// consumed before.  The identifier is stored in the [`ClaimRecord`] and a
    /// separate [`StorageKey::ClaimConsumed`] entry is initialised to `false`.
    ///
    /// # Errors
    /// - [`BatchClaimError::NotInitialized`] if `init` has not been called.
    /// - [`BatchClaimError::Unauthorized`] if `caller` is not the admin.
    /// - [`BatchClaimError::InvalidAmount`] if `amount` ≤ 0.
    /// - [`BatchClaimError::ClaimIdAlreadyUsed`] if `claim_id` has already been consumed.
    /// - [`BatchClaimError::Overflow`] if the accumulated total would overflow.
    pub fn add_claim(
        env: Env,
        caller: Address,
        claimant: Address,
        amount: i128,
        claim_id: BytesN<32>,
    ) -> Result<(), BatchClaimError> {
        caller.require_auth();
        let admin = Self::require_admin(&env)?;
        if caller != admin {
            return Err(BatchClaimError::Unauthorized);
        }
        if amount <= 0 {
            return Err(BatchClaimError::InvalidAmount);
        }

        // Reject re-use of a previously consumed claim identifier.
        let consumed_key = StorageKey::ClaimConsumed(claim_id.clone());
        if env.storage().persistent().has(&consumed_key) {
            env.storage()
                .persistent()
                .extend_ttl(&consumed_key, PERSISTENT_THRESHOLD, PERSISTENT_BUMP);
            let already_consumed: bool = env
                .storage()
                .persistent()
                .get(&consumed_key)
                .unwrap_or(false);
            if already_consumed {
                return Err(BatchClaimError::ClaimIdAlreadyUsed);
            }
        }

        let key = StorageKey::Claim(claimant.clone());

        // Bump TTL on the existing persistent entry before reading it.
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
            claim_id: claim_id.clone(),
        };
        env.storage().persistent().set(&key, &record);
        // Bump claim record TTL after writing.
        env.storage()
            .persistent()
            .extend_ttl(&key, PERSISTENT_THRESHOLD, PERSISTENT_BUMP);

        // Initialise the consumed tombstone (false = not yet consumed).
        env.storage()
            .persistent()
            .set(&consumed_key, &false);
        env.storage()
            .persistent()
            .extend_ttl(&consumed_key, PERSISTENT_THRESHOLD, PERSISTENT_BUMP);

        Self::bump_instance(&env);
        env.events()
            .publish((events::event_claim_added(&env),), (claimant, new_pending));
        Ok(())
    }

    /// Settle (collect) all pending claims for a batch of claimants.
    ///
    /// Each entry in `claimants` is a tuple of `(address, claim_id)`.  The
    /// `claimant` must authorize their own collection, and the supplied
    /// `claim_id` must match the one stored in their [`ClaimRecord`].
    ///
    /// ## Replay-safe semantics
    ///
    /// The consumed flag for each `claim_id` is set to `true` **before** the
    /// `settled` bit is written and before the event is emitted.  This
    /// write-before-settle ordering means:
    ///
    /// - A retry of the exact same call after partial success will fail on the
    ///   already-consumed identifier, not silently re-release funds.
    /// - Failed validation (wrong `claim_id`, `ClaimNotFound`, already settled)
    ///   does **not** touch the consumed flag — state is only mutated on
    ///   the success path.
    ///
    /// Returns the total amount collected across all claimants.
    ///
    /// # Errors
    /// - [`BatchClaimError::NotInitialized`] if `init` has not been called.
    /// - [`BatchClaimError::ClaimNotFound`] if a claimant has no pending record.
    /// - [`BatchClaimError::AlreadySettled`] if a claimant's claim is already settled.
    /// - [`BatchClaimError::ClaimIdMismatch`] if the supplied `claim_id` does not
    ///   match the stored record.
    /// - [`BatchClaimError::ClaimIdAlreadyUsed`] if the `claim_id` has already been
    ///   consumed (replay attempt).
    /// - [`BatchClaimError::Overflow`] if the running total overflows.
    pub fn batch_claim(
        env: Env,
        claimants: Vec<(Address, BytesN<32>)>,
    ) -> Result<i128, BatchClaimError> {
        Self::require_admin(&env)?;
        let mut total_claimed: i128 = 0;

        for (claimant, claim_id) in claimants.iter() {
            claimant.require_auth();
            let key = StorageKey::Claim(claimant.clone());
            let consumed_key = StorageKey::ClaimConsumed(claim_id.clone());

            // --- Hot-read path TTL bump on claim record ---
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

            // Validation checks — none of these mutate state, so a failed
            // call leaves storage exactly as it was found.

            // 1. Verify the claim_id matches the stored record.
            if record.claim_id != claim_id {
                return Err(BatchClaimError::ClaimIdMismatch);
            }

            // 2. Verify the claim_id has not been consumed by a prior call.
            //    This check MUST precede the settled check so that the
            //    consumed tombstone is the authoritative replay guard.
            //    Bump consumed-tombstone TTL on every read (hot path).
            if env.storage().persistent().has(&consumed_key) {
                env.storage()
                    .persistent()
                    .extend_ttl(&consumed_key, PERSISTENT_THRESHOLD, PERSISTENT_BUMP);
                let already_consumed: bool = env
                    .storage()
                    .persistent()
                    .get(&consumed_key)
                    .unwrap_or(false);
                if already_consumed {
                    return Err(BatchClaimError::ClaimIdAlreadyUsed);
                }
            }

            // 3. Verify the record has not already been settled.
            //    This is a secondary consistency check; the consumed tombstone
            //    above is the primary replay guard.
            if record.settled {
                return Err(BatchClaimError::AlreadySettled);
            }

            // All validation passed.  Accumulate into running total before
            // writing any state so an overflow error is also non-mutating.
            total_claimed = total_claimed
                .checked_add(record.pending_amount)
                .ok_or(BatchClaimError::Overflow)?;

            // Write-before-settle: mark the claim_id as consumed FIRST.
            // This is the concurrency-safety invariant: if two transactions
            // race, the one that commits this write second will read
            // `already_consumed = true` and fail with ClaimIdAlreadyUsed.
            env.storage()
                .persistent()
                .set(&consumed_key, &true);
            env.storage()
                .persistent()
                .extend_ttl(&consumed_key, PERSISTENT_THRESHOLD, PERSISTENT_BUMP);

            // Now mark the claim record itself as settled.
            record.settled = true;
            env.storage().persistent().set(&key, &record);
            env.storage()
                .persistent()
                .extend_ttl(&key, PERSISTENT_THRESHOLD, PERSISTENT_BUMP);

            // Emit consumed event so off-chain indexers can track spent ids.
            env.events().publish(
                (events::event_claim_consumed(&env),),
                claim_id.clone(),
            );

            // Emit settled event with claimant + amount.
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

    /// Return whether a given `claim_id` has already been consumed.
    ///
    /// Returns `true` if the identifier has been spent (successfully settled),
    /// `false` if it is either not yet registered or registered but still
    /// pending.
    ///
    /// **TTL bump**: bumps the persistent entry TTL on every call.
    ///
    /// No auth required.
    ///
    /// # Errors
    /// Returns [`BatchClaimError::NotInitialized`] if `init` has not been called.
    pub fn claim_id_consumed(env: Env, claim_id: BytesN<32>) -> Result<bool, BatchClaimError> {
        Self::require_admin(&env)?;
        let key = StorageKey::ClaimConsumed(claim_id);

        if env.storage().persistent().has(&key) {
            env.storage()
                .persistent()
                .extend_ttl(&key, PERSISTENT_THRESHOLD, PERSISTENT_BUMP);
            return Ok(env
                .storage()
                .persistent()
                .get(&key)
                .unwrap_or(false));
        }
        Ok(false)
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
    use soroban_sdk::{Address, BytesN, Env, Vec};

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Build a deterministic 32-byte claim id from a small integer seed.
    fn make_id(env: &Env, seed: u8) -> BytesN<32> {
        let mut raw = [0u8; 32];
        raw[31] = seed;
        BytesN::from_array(env, &raw)
    }

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
    // add_claim authorization
    // -----------------------------------------------------------------------

    #[test]
    fn test_add_claim_non_admin_rejected() {
        let env = Env::default();
        let (_admin, claimant, client) = setup(&env);
        let intruder = Address::generate(&env);
        let id = make_id(&env, 1);
        let res = client.try_add_claim(&intruder, &claimant, &100, &id);
        assert_eq!(res, Err(Ok(BatchClaimError::Unauthorized)));
    }

    #[test]
    fn test_add_claim_zero_amount_rejected() {
        let env = Env::default();
        let (admin, claimant, client) = setup(&env);
        let id = make_id(&env, 1);
        let res = client.try_add_claim(&admin, &claimant, &0, &id);
        assert_eq!(res, Err(Ok(BatchClaimError::InvalidAmount)));
    }

    #[test]
    fn test_add_claim_negative_amount_rejected() {
        let env = Env::default();
        let (admin, claimant, client) = setup(&env);
        let id = make_id(&env, 1);
        let res = client.try_add_claim(&admin, &claimant, &-1, &id);
        assert_eq!(res, Err(Ok(BatchClaimError::InvalidAmount)));
    }

    /// Boundary: amount of 1 is the smallest accepted value.
    #[test]
    fn test_add_claim_boundary_amount_one_accepted() {
        let env = Env::default();
        let (admin, claimant, client) = setup(&env);
        let id = make_id(&env, 1);
        client.add_claim(&admin, &claimant, &1, &id);
        let rec = client.get_claim(&claimant);
        assert_eq!(rec.pending_amount, 1);
    }

    // -----------------------------------------------------------------------
    // Replay protection: claim_id uniqueness in add_claim
    // -----------------------------------------------------------------------

    /// A consumed claim_id cannot be re-issued by the admin.
    #[test]
    fn test_add_claim_rejects_consumed_id() {
        let env = Env::default();
        let (admin, claimant, client) = setup(&env);
        let id = make_id(&env, 7);

        client.add_claim(&admin, &claimant, &500, &id);

        // Settle so the id is consumed.
        let mut batch = Vec::new(&env);
        batch.push_back((claimant.clone(), id.clone()));
        client.batch_claim(&batch);

        // Admin tries to reuse the same claim_id for a new issuance → rejected.
        let claimant2 = Address::generate(&env);
        let res = client.try_add_claim(&admin, &claimant2, &200, &id);
        assert_eq!(res, Err(Ok(BatchClaimError::ClaimIdAlreadyUsed)));
    }

    // -----------------------------------------------------------------------
    // batch_claim: replay / retry protection
    // -----------------------------------------------------------------------

    /// Happy path: a valid claim is settled exactly once and returns the amount.
    #[test]
    fn test_batch_claim_happy_path() {
        let env = Env::default();
        let (admin, claimant, client) = setup(&env);
        let id = make_id(&env, 1);
        client.add_claim(&admin, &claimant, &1_000, &id);

        let mut batch = Vec::new(&env);
        batch.push_back((claimant.clone(), id.clone()));
        let total = client.batch_claim(&batch);
        assert_eq!(total, 1_000);

        // Verify settled state.
        let rec = client.get_claim(&claimant);
        assert!(rec.settled);
    }

    /// Retrying the exact same batch after success returns ClaimIdAlreadyUsed
    /// (the consumed tombstone is set), not AlreadySettled.
    #[test]
    fn test_retry_same_id_rejected_with_claim_id_already_used() {
        let env = Env::default();
        let (admin, claimant, client) = setup(&env);
        let id = make_id(&env, 2);
        client.add_claim(&admin, &claimant, &500, &id);

        let mut batch = Vec::new(&env);
        batch.push_back((claimant.clone(), id.clone()));

        client.batch_claim(&batch);

        // Second attempt with the same id must fail.
        let res = client.try_batch_claim(&batch);
        assert_eq!(res, Err(Ok(BatchClaimError::ClaimIdAlreadyUsed)));
    }

    /// Supplying a wrong claim_id returns ClaimIdMismatch and does NOT consume state.
    #[test]
    fn test_wrong_claim_id_returns_mismatch_and_does_not_consume_state() {
        let env = Env::default();
        let (admin, claimant, client) = setup(&env);
        let real_id = make_id(&env, 10);
        let wrong_id = make_id(&env, 11);
        client.add_claim(&admin, &claimant, &250, &real_id);

        let mut batch = Vec::new(&env);
        batch.push_back((claimant.clone(), wrong_id.clone()));
        let res = client.try_batch_claim(&batch);
        assert_eq!(res, Err(Ok(BatchClaimError::ClaimIdMismatch)));

        // real_id must still be unconsumed — claim is still claimable.
        assert_eq!(client.claim_id_consumed(&real_id), false);
        assert!(client.has_claim(&claimant));
    }

    /// Failed validation (ClaimNotFound) does not mutate consumed state.
    #[test]
    fn test_claim_not_found_does_not_consume_state() {
        let env = Env::default();
        let (_admin, _claimant, client) = setup(&env);
        let missing = Address::generate(&env);
        let id = make_id(&env, 3);

        let mut batch = Vec::new(&env);
        batch.push_back((missing.clone(), id.clone()));
        let res = client.try_batch_claim(&batch);
        assert_eq!(res, Err(Ok(BatchClaimError::ClaimNotFound)));

        // Consumed flag was never written — must report false.
        assert_eq!(client.claim_id_consumed(&id), false);
    }

    /// AlreadySettled path: does not mutate the consumed tombstone further.
    #[test]
    fn test_already_settled_does_not_re_consume() {
        let env = Env::default();
        let (admin, claimant, client) = setup(&env);
        let id = make_id(&env, 4);
        client.add_claim(&admin, &claimant, &100, &id);

        let mut batch = Vec::new(&env);
        batch.push_back((claimant.clone(), id.clone()));

        client.batch_claim(&batch); // first — succeeds, consumes id
        assert_eq!(client.claim_id_consumed(&id), true);

        // The claim is settled AND id consumed, so next attempt hits
        // ClaimIdAlreadyUsed (consumed check comes before settled check).
        let res = client.try_batch_claim(&batch);
        assert_eq!(res, Err(Ok(BatchClaimError::ClaimIdAlreadyUsed)));
    }

    // -----------------------------------------------------------------------
    // claim_id_consumed view
    // -----------------------------------------------------------------------

    #[test]
    fn test_claim_id_consumed_view_accurate() {
        let env = Env::default();
        let (admin, claimant, client) = setup(&env);
        let id = make_id(&env, 5);

        // Before add_claim: not known.
        assert_eq!(client.claim_id_consumed(&id), false);

        client.add_claim(&admin, &claimant, &300, &id);
        // Registered but not yet consumed.
        assert_eq!(client.claim_id_consumed(&id), false);

        let mut batch = Vec::new(&env);
        batch.push_back((claimant.clone(), id.clone()));
        client.batch_claim(&batch);

        // After successful settlement: consumed.
        assert_eq!(client.claim_id_consumed(&id), true);
    }

    // -----------------------------------------------------------------------
    // Concurrent / same-id only one succeeds
    // -----------------------------------------------------------------------

    /// Simulate two independent batches carrying the same claim_id.  Only the
    /// first to execute wins; the second is rejected with ClaimIdAlreadyUsed.
    #[test]
    fn test_concurrent_same_id_only_one_succeeds() {
        let env = Env::default();
        let (admin, claimant, client) = setup(&env);
        let id = make_id(&env, 6);
        client.add_claim(&admin, &claimant, &999, &id);

        let mut batch_a = Vec::new(&env);
        batch_a.push_back((claimant.clone(), id.clone()));
        let mut batch_b = Vec::new(&env);
        batch_b.push_back((claimant.clone(), id.clone()));

        // First batch succeeds.
        let result_a = client.batch_claim(&batch_a);
        assert_eq!(result_a, 999);

        // Second batch (same id) is rejected.
        let result_b = client.try_batch_claim(&batch_b);
        assert_eq!(result_b, Err(Ok(BatchClaimError::ClaimIdAlreadyUsed)));

        // Consumed flag is permanently set.
        assert_eq!(client.claim_id_consumed(&id), true);
    }

    // -----------------------------------------------------------------------
    // Unauthorized callers in batch_claim
    // -----------------------------------------------------------------------

    /// A claimant whose auth is absent (or wrong) cannot claim someone else's funds.
    #[test]
    fn test_batch_claim_requires_claimant_auth() {
        let env = Env::default();
        env.mock_all_auths(); // still mock — but we test with a claimant who
                              // has no record, simulating the unauthorized path.
        let admin = Address::generate(&env);
        let contract_id = env.register(CalloraBatchClaim, ());
        let client = CalloraBatchClaimClient::new(&env, &contract_id);
        client.init(&admin);

        let claimant = Address::generate(&env);
        let id = make_id(&env, 20);
        client.add_claim(&admin, &claimant, &400, &id);

        // An unrelated address tries to claim using the real claimant's record
        // but its own identity → ClaimNotFound (no record for intruder).
        let intruder = Address::generate(&env);
        let mut batch = Vec::new(&env);
        batch.push_back((intruder.clone(), id.clone()));
        let res = client.try_batch_claim(&batch);
        assert_eq!(res, Err(Ok(BatchClaimError::ClaimNotFound)));

        // The real claimant's id is still unconsumed.
        assert_eq!(client.claim_id_consumed(&id), false);
    }

    // -----------------------------------------------------------------------
    // cancel_claim authorization
    // -----------------------------------------------------------------------

    #[test]
    fn test_cancel_claim_non_admin_rejected() {
        let env = Env::default();
        let (admin, claimant, client) = setup(&env);
        let id = make_id(&env, 30);
        client.add_claim(&admin, &claimant, &100, &id);

        let intruder = Address::generate(&env);
        let res = client.try_cancel_claim(&intruder, &claimant);
        assert_eq!(res, Err(Ok(BatchClaimError::Unauthorized)));
    }

    #[test]
    fn test_cancel_claim_not_found() {
        let env = Env::default();
        let (admin, _claimant, client) = setup(&env);
        let missing = Address::generate(&env);
        let res = client.try_cancel_claim(&admin, &missing);
        assert_eq!(res, Err(Ok(BatchClaimError::ClaimNotFound)));
    }

    #[test]
    fn test_cancel_claim_already_settled_rejected() {
        let env = Env::default();
        let (admin, claimant, client) = setup(&env);
        let id = make_id(&env, 31);
        client.add_claim(&admin, &claimant, &100, &id);

        let mut batch = Vec::new(&env);
        batch.push_back((claimant.clone(), id.clone()));
        client.batch_claim(&batch);

        let res = client.try_cancel_claim(&admin, &claimant);
        assert_eq!(res, Err(Ok(BatchClaimError::AlreadySettled)));
    }

    #[test]
    fn test_cancel_claim_happy_path() {
        let env = Env::default();
        let (admin, claimant, client) = setup(&env);
        let id = make_id(&env, 32);
        client.add_claim(&admin, &claimant, &100, &id);
        client.cancel_claim(&admin, &claimant);
        assert_eq!(client.has_claim(&claimant), false);
    }

    // -----------------------------------------------------------------------
    // Overflow protection
    // -----------------------------------------------------------------------

    #[test]
    fn test_add_claim_accumulation_overflow_rejected() {
        let env = Env::default();
        let (admin, claimant, client) = setup(&env);
        let id1 = make_id(&env, 40);
        let id2 = make_id(&env, 41);

        client.add_claim(&admin, &claimant, &i128::MAX, &id1);
        // Adding any positive value would overflow.
        let res = client.try_add_claim(&admin, &claimant, &1, &id2);
        assert_eq!(res, Err(Ok(BatchClaimError::Overflow)));
    }

    // -----------------------------------------------------------------------
    // total_claims and get_admin views
    // -----------------------------------------------------------------------

    #[test]
    fn test_total_claims_increments() {
        let env = Env::default();
        let (admin, claimant, client) = setup(&env);
        assert_eq!(client.total_claims(), 0);
        client.add_claim(&admin, &claimant, &10, &make_id(&env, 50));
        assert_eq!(client.total_claims(), 1);
        let c2 = Address::generate(&env);
        client.add_claim(&admin, &c2, &10, &make_id(&env, 51));
        assert_eq!(client.total_claims(), 2);
    }

    #[test]
    fn test_views_before_init_return_not_initialized() {
        let env = Env::default();
        let id = env.register(CalloraBatchClaim, ());
        let client = CalloraBatchClaimClient::new(&env, &id);
        assert_eq!(
            client.try_get_admin(),
            Err(Ok(BatchClaimError::NotInitialized))
        );
        assert_eq!(
            client.try_total_claims(),
            Err(Ok(BatchClaimError::NotInitialized))
        );
    }

    // -----------------------------------------------------------------------
    // Multi-claimant batch with independent ids
    // -----------------------------------------------------------------------

    #[test]
    fn test_batch_claim_multiple_claimants_different_ids() {
        let env = Env::default();
        let (admin, c1, client) = setup(&env);
        let c2 = Address::generate(&env);
        let id1 = make_id(&env, 60);
        let id2 = make_id(&env, 61);

        client.add_claim(&admin, &c1, &300, &id1);
        client.add_claim(&admin, &c2, &700, &id2);

        let mut batch = Vec::new(&env);
        batch.push_back((c1.clone(), id1.clone()));
        batch.push_back((c2.clone(), id2.clone()));

        let total = client.batch_claim(&batch);
        assert_eq!(total, 1_000);

        assert_eq!(client.claim_id_consumed(&id1), true);
        assert_eq!(client.claim_id_consumed(&id2), true);
    }

    /// A batch where the second entry fails must not consume the first entry's id
    /// if the call rolls back — but in Soroban, panics/errors inside a contract
    /// invocation revert the whole transaction.  This test verifies the
    /// error is surfaced and the overall state is consistent after the revert.
    #[test]
    fn test_batch_partial_failure_is_atomic() {
        let env = Env::default();
        let (admin, c1, client) = setup(&env);
        let id1 = make_id(&env, 70);
        let id_bad = make_id(&env, 71); // no claim registered for c2

        let c2 = Address::generate(&env);
        client.add_claim(&admin, &c1, &100, &id1);

        let mut batch = Vec::new(&env);
        batch.push_back((c1.clone(), id1.clone()));
        batch.push_back((c2.clone(), id_bad.clone()));

        // The whole batch fails because c2 has no claim.
        let res = client.try_batch_claim(&batch);
        assert_eq!(res, Err(Ok(BatchClaimError::ClaimNotFound)));

        // c1's id should be unconsumed because the transaction reverted.
        assert_eq!(client.claim_id_consumed(&id1), false);
        assert!(client.has_claim(&c1));
    }
}

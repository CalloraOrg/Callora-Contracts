#![no_std]

#[cfg(test)]
extern crate std;

pub mod errors;
pub mod events;

use callora_storage_migration::{
    layout_hash, StorageMigrationError, StorageMigrationValidator, LEGACY_VERSION,
};
use errors::CheckpointError;
use soroban_sdk::{
    contract, contractimpl, contracttype, xdr::ToXdr, Address, Bytes, BytesN, Env, Symbol, Vec,
};

/// Storage-layout version recorded at init. Bump whenever the checkpoint
/// storage schema changes.
const STORAGE_MIGRATION_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum number of checkpoint records that can be created in a single
/// [`CalloraCheckpoint::batch_create_checkpoints`] call.
pub const MAX_BATCH_SIZE: u32 = 50;

/// Maximum number of checkpoint records returned per page from
/// [`CalloraCheckpoint::get_checkpoints_range`].
pub const MAX_PAGE_SIZE: u32 = 100;

/// Maximum number of checkpoint records a single
/// [`CalloraCheckpoint::prune_before`] call may remove, regardless of the
/// caller-supplied `max_remove`. Keeps pruning within a predictable budget.
pub const MAX_PRUNE_BATCH: u32 = 100;

/// TTL bump constants for persistent storage archival risk mitigation.
/// Soroban archives ledger entries after ~7 days (631 ledgers) of inactivity.
///
/// - `BUMP_AMOUNT`: extend TTL by 3 110 400 ledgers (approx 6 months).
/// - `LIFETIME_THRESHOLD`: minimum TTL before triggering a bump (approx 1 day).
pub const BUMP_AMOUNT: u32 = 3_110_400;
pub const LIFETIME_THRESHOLD: u32 = 17_280;

/// Approximate number of ledgers per day on Stellar (5 seconds per ledger).
pub const LEDGERS_PER_DAY: u32 = 17_280;

/// Threshold at which instance storage TTL is extended (~30 days).
///
/// Admin, pending-admin, and checkpoint counters live in instance storage;
/// the entrypoint-level [`CalloraCheckpoint::bump_instance`] helper keeps the
/// instance from archiving independently of the persistent record policy.
pub const INSTANCE_BUMP_THRESHOLD: u32 = LEDGERS_PER_DAY * 30;

/// Target TTL when instance storage is extended (~60 days).
pub const INSTANCE_BUMP_AMOUNT: u32 = LEDGERS_PER_DAY * 60;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Persistent and instance storage keys for the checkpoint contract.
///
/// Each variant maps a logical key name to the underlying Soroban storage
/// key, avoiding accidental key collisions with raw strings.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum StorageKey {
    /// Instance: current admin [`Address`].
    Admin,
    /// Instance: pending admin [`Address`] for two-step rotation.
    PendingAdmin,
    /// Instance: next sequential checkpoint ID to assign (`u64`).
    NextCheckpointId,
    /// Instance: running SHA-256 digest chain head over every checkpoint
    /// included in the chain (`BytesN<32>`).
    DigestHead,
    /// Instance: chain head at the last pruned checkpoint (`BytesN<32>`).
    ///
    /// A consumer holding retained records can fold them from this value to
    /// reproduce [`StorageKey::DigestHead`].
    DigestAtBoundary,
    /// Instance: first checkpoint ID that has not been pruned (`u64`).
    ///
    /// Monotonic; every ID below this boundary is gone from storage.
    PrunedBefore,
    /// Instance: first checkpoint ID included in the digest chain (`u64`).
    ///
    /// Checkpoints written before the digest feature existed (i.e. before a
    /// migration finalized by [`CalloraCheckpoint::migrate`]) are excluded from
    /// the chain; the chain starts at the first post-upgrade checkpoint.
    ChainStartId,
    /// Persistent: an individual checkpoint record indexed by `u64` ID.
    Checkpoint(u64),
    /// Instance: cached count of total checkpoints (`u64`), kept in
    /// sync with `NextCheckpointId` for efficient `get_checkpoint_count`.
    CheckpointCount,
    /// Persistent: ordered list of checkpoint IDs for a given subject [`Address`].
    ///
    /// Maintained as an append-only `Vec<u64>` in persistent storage so that
    /// [`CalloraCheckpoint::get_checkpoints_for_subject`] can return a
    /// subject's history in creation order without scanning the full range.
    SubjectIndex(Address),
}

/// A single immutable audit checkpoint recording a balance snapshot at a point in time.
///
/// Once written, a [`CheckpointRecord`] is never updated. It provides an
/// append-only audit trail suitable for compliance, reconciliation, and
/// dispute resolution.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct CheckpointRecord {
    /// Globally-unique sequential checkpoint identifier.
    pub id: u64,
    /// The address whose balance is being snapshotted (e.g., a developer).
    pub subject: Address,
    /// The token contract address this balance is denominated in (e.g., USDC).
    pub token: Address,
    /// The snapshotted balance in token base units.
    pub balance: i128,
    /// Ledger timestamp (seconds) at which this checkpoint was created.
    pub timestamp: u64,
    /// Free-form metadata tag attached by the admin (e.g.,
    /// "monthly-close", "pre-migration"). Soroban
    /// [`Symbol`] values are protocol-limited to 32 characters.
    pub metadata: Symbol,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct CalloraCheckpoint;

#[contractimpl]
impl CalloraCheckpoint {
    fn storage_layout_hash(env: &Env) -> BytesN<32> {
        layout_hash(
            env,
            "checkpoint:v1:instance:Admin:Address,PendingAdmin:Address,NextCheckpointId:u64,CheckpointCount:u64;persistent:Checkpoint(u64):CheckpointRecord{id:u64,subject:Address,token:Address,balance:i128,timestamp:u64,metadata:Symbol},SubjectIndex(Address):Vec<u64>",
        )
    }

    fn migration_error(error: StorageMigrationError) -> CheckpointError {
        match error {
            StorageMigrationError::NotInitialized => CheckpointError::NotInitialized,
            StorageMigrationError::AlreadyInitialized => CheckpointError::AlreadyInitialized,
            StorageMigrationError::WasmHashZero => CheckpointError::WasmHashZero,
            StorageMigrationError::VersionSkip => CheckpointError::VersionSkip,
            StorageMigrationError::RollbackNotAllowed => CheckpointError::RollbackNotAllowed,
            StorageMigrationError::BackupMissing => CheckpointError::BackupMissing,
            StorageMigrationError::SchemaMismatch => CheckpointError::SchemaMismatch,
            StorageMigrationError::SilentLayoutChange => CheckpointError::SilentLayoutChange,
            StorageMigrationError::UnauthorizedUpgradeState => {
                CheckpointError::UnauthorizedUpgradeState
            }
            StorageMigrationError::StorageUnreadable => CheckpointError::StorageUnreadable,
            StorageMigrationError::TooManyProbeKeys => CheckpointError::StorageUnreadable,
            StorageMigrationError::Unauthorized => CheckpointError::Unauthorized,
        }
    }

    fn ensure_migration_finalized(env: &Env) -> Result<(), CheckpointError> {
        if StorageMigrationValidator::authorized_upgrade(env).is_some() {
            return Err(CheckpointError::MigrationPending);
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // TTL helpers
    // -----------------------------------------------------------------------

    /// Extend the instance storage TTL.
    ///
    /// Called on every entrypoint — including read-only views — so the
    /// instance (admin, counters) never archives due to inactivity. Persistent
    /// checkpoint records keep their own independent policy (`BUMP_AMOUNT`).
    fn bump_instance(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    // -----------------------------------------------------------------------
    // Initialisation
    // -----------------------------------------------------------------------

    /// Initialize the checkpoint contract with an admin address.
    ///
    /// Can only be called once. Subsequent calls return
    /// [`CheckpointError::AlreadyInitialized`].
    ///
    /// # Parameters
    /// * `env` - Soroban environment.
    /// * `admin` - Address permitted to call admin-only entrypoints.
    ///
    /// # Errors
    /// * [`CheckpointError::AlreadyInitialized`] -- `init` was called more than once.
    ///
    /// # Events
    /// Emits `init` with `admin` as topic and `()` as data.
    pub fn init(env: Env, admin: Address) -> Result<(), CheckpointError> {
        admin.require_auth();
        Self::bump_instance(&env);
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(CheckpointError::AlreadyInitialized);
        }

        let inst = env.storage().instance();
        inst.set(&StorageKey::Admin, &admin);
        inst.set(&StorageKey::NextCheckpointId, &0u64);
        inst.set(&StorageKey::CheckpointCount, &0u64);
        inst.set(&StorageKey::DigestHead, &Self::zero_digest(&env));
        inst.set(&StorageKey::DigestAtBoundary, &Self::zero_digest(&env));
        inst.set(&StorageKey::PrunedBefore, &1u64);
        inst.set(&StorageKey::ChainStartId, &1u64);
        StorageMigrationValidator::record_deployed_schema(
            &env,
            STORAGE_MIGRATION_VERSION,
            &Self::storage_layout_hash(&env),
        )
        .map_err(Self::migration_error)?;

        env.events().publish((events::event_init(&env), admin), ());

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Admin helpers (internal)
    // -----------------------------------------------------------------------

    /// Read the current admin address from instance storage.
    ///
    /// Returns [`CheckpointError::NotInitialized`] when no admin has been set
    /// (i.e., `init` was never called).
    fn admin(env: &Env) -> Result<Address, CheckpointError> {
        env.storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(CheckpointError::NotInitialized)
    }

    /// Verify that `caller` is the current admin.
    ///
    /// Consumes the caller's auth via `require_auth`, then checks instance
    /// storage for the admin address. Returns [`CheckpointError::Unauthorized`]
    /// when the caller does not match.
    fn require_admin(env: &Env, caller: &Address) -> Result<(), CheckpointError> {
        caller.require_auth();
        Self::bump_instance(env);
        let admin = Self::admin(env)?;
        if caller != &admin {
            return Err(CheckpointError::Unauthorized);
        }
        Ok(())
    }

    /// Atomically increment the sequential checkpoint ID counter and return
    /// the value that should be assigned to the newly created checkpoint.
    ///
    /// Uses checked arithmetic. Always updates both `NextCheckpointId` and
    /// `CheckpointCount` to keep them in sync.
    fn next_checkpoint_id(env: &Env) -> Result<u64, CheckpointError> {
        let current: u64 = env
            .storage()
            .instance()
            .get(&StorageKey::NextCheckpointId)
            .unwrap_or(0);

        let next = current.checked_add(1).ok_or(CheckpointError::Overflow)?;

        env.storage()
            .instance()
            .set(&StorageKey::NextCheckpointId, &next);
        env.storage()
            .instance()
            .set(&StorageKey::CheckpointCount, &next);

        Ok(next)
    }

    // -----------------------------------------------------------------------
    // Digest chain helpers
    // -----------------------------------------------------------------------

    /// The zero digest used as the chain seed before any checkpoint is hashed.
    fn zero_digest(env: &Env) -> BytesN<32> {
        BytesN::from_array(env, &[0u8; 32])
    }

    /// Read the current chain head, falling back to the zero seed for
    /// contracts upgraded before the digest keys existed.
    fn digest_head(env: &Env) -> BytesN<32> {
        env.storage()
            .instance()
            .get(&StorageKey::DigestHead)
            .unwrap_or_else(|| Self::zero_digest(env))
    }

    /// Read the chain head at the prune boundary.
    fn digest_at_boundary(env: &Env) -> BytesN<32> {
        env.storage()
            .instance()
            .get(&StorageKey::DigestAtBoundary)
            .unwrap_or_else(|| Self::zero_digest(env))
    }

    /// Read the monotonic prune boundary. ID 1 is the default for contracts
    /// that predate the pruning feature.
    fn pruned_before(env: &Env) -> u64 {
        env.storage()
            .instance()
            .get(&StorageKey::PrunedBefore)
            .unwrap_or(1)
    }

    /// Read the first checkpoint ID included in the chain.
    fn chain_start_id(env: &Env) -> u64 {
        env.storage()
            .instance()
            .get(&StorageKey::ChainStartId)
            .unwrap_or(1)
    }

    /// Fold one record into a chain head: `sha256(prev_head || xdr(record))`.
    ///
    /// The XDR encoding is the canonical `ScVal` encoding of the
    /// [`CheckpointRecord`], so consumers can reproduce the chain off-chain
    /// with the same preimage.
    fn hash_record(env: &Env, prev_head: &BytesN<32>, record: &CheckpointRecord) -> BytesN<32> {
        let mut preimage = Bytes::new(env);
        preimage.append(&Bytes::from(prev_head));
        preimage.append(&record.clone().to_xdr(env));
        env.crypto().sha256(&preimage).into()
    }

    /// Advance the persistent chain head with a newly created record.
    fn append_to_digest(env: &Env, record: &CheckpointRecord) {
        let head = Self::hash_record(env, &Self::digest_head(env), record);
        env.storage().instance().set(&StorageKey::DigestHead, &head);
    }

    /// Append `checkpoint_id` to the persistent per-subject index for `subject`.
    ///
    /// Creates the index list on first use. After writing, the entry's TTL is
    /// extended to [`BUMP_AMOUNT`] ledgers so the index survives as long as
    /// individual checkpoint records.
    fn append_to_subject_index(env: &Env, subject: &Address, checkpoint_id: u64) {
        let key = StorageKey::SubjectIndex(subject.clone());
        let mut ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Vec::new(env));
        ids.push_back(checkpoint_id);
        env.storage().persistent().set(&key, &ids);
        env.storage()
            .persistent()
            .extend_ttl(&key, LIFETIME_THRESHOLD, BUMP_AMOUNT);
    }

    /// Drop pruned IDs in `[from_id, to_id)` from a subject's index.
    ///
    /// Trimming the index keeps offset-based subject paging dense: a page
    /// never comes back empty merely because it landed on pruned IDs.
    fn remove_pruned_ids_from_subject_index(
        env: &Env,
        subject: &Address,
        from_id: u64,
        to_id: u64,
    ) {
        let key = StorageKey::SubjectIndex(subject.clone());
        if let Some(ids) = env.storage().persistent().get::<_, Vec<u64>>(&key) {
            let mut kept = Vec::new(env);
            for id in ids.iter() {
                if id < from_id || id >= to_id {
                    kept.push_back(id);
                }
            }
            if kept.is_empty() {
                env.storage().persistent().remove(&key);
            } else {
                env.storage().persistent().set(&key, &kept);
                env.storage()
                    .persistent()
                    .extend_ttl(&key, LIFETIME_THRESHOLD, BUMP_AMOUNT);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Admin views
    // -----------------------------------------------------------------------

    /// Return the current admin address.
    ///
    /// # Errors
    /// * [`CheckpointError::NotInitialized`] -- contract was never initialized.
    pub fn get_admin(env: Env) -> Result<Address, CheckpointError> {
        Self::bump_instance(&env);
        Self::admin(&env)
    }

    /// Return the pending admin address for a two-step admin rotation, or
    /// `None` if no transfer is in progress.
    pub fn get_pending_admin(env: Env) -> Option<Address> {
        Self::bump_instance(&env);
        env.storage().instance().get(&StorageKey::PendingAdmin)
    }

    // -----------------------------------------------------------------------
    // Two-step admin rotation
    // -----------------------------------------------------------------------

    /// Nominate a new admin. Only the current admin may call.
    ///
    /// The nominee must call [`CalloraCheckpoint::accept_admin`] to complete
    /// the transfer. Until then the current admin retains full authority.
    ///
    /// # Parameters
    /// * `caller` -- Must be the current admin; must authorize.
    /// * `new_admin` -- Address of the proposed new admin.
    ///
    /// # Errors
    /// * [`CheckpointError::Unauthorized`] -- caller is not the current admin.
    /// * [`CheckpointError::NotInitialized`] -- contract not initialized.
    ///
    /// # Events
    /// Emits `admin_nominated` with `(current_admin, new_admin)`.
    pub fn set_admin(env: Env, caller: Address, new_admin: Address) -> Result<(), CheckpointError> {
        Self::require_admin(&env, &caller)?;
        Self::ensure_migration_finalized(&env)?;

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
    /// # Errors
    /// * [`CheckpointError::NotInitialized`] -- contract not initialized.
    /// * [`CheckpointError::NoAdminTransferPending`] -- no nomination is in progress.
    /// * [`CheckpointError::Unauthorized`] -- caller is not the pending admin.
    ///
    /// # Events
    /// Emits `admin_accepted` with `(old_admin, new_admin)`.
    pub fn accept_admin(env: Env, caller: Address) -> Result<(), CheckpointError> {
        caller.require_auth();
        Self::bump_instance(&env);
        Self::ensure_migration_finalized(&env)?;

        let pending: Address = env
            .storage()
            .instance()
            .get(&StorageKey::PendingAdmin)
            .ok_or(CheckpointError::NoAdminTransferPending)?;

        if caller != pending {
            return Err(CheckpointError::Unauthorized);
        }

        let old_admin = Self::admin(&env)?;
        let inst = env.storage().instance();
        inst.set(&StorageKey::Admin, &pending);
        inst.remove(&StorageKey::PendingAdmin);

        env.events().publish(
            (
                events::event_admin_accepted(&env),
                old_admin,
                pending.clone(),
            ),
            pending,
        );

        Ok(())
    }

    /// Cancel a pending admin transfer. Only the current admin may call.
    ///
    /// # Errors
    /// * [`CheckpointError::Unauthorized`] -- caller is not the current admin.
    /// * [`CheckpointError::NotInitialized`] -- contract not initialized.
    /// * [`CheckpointError::NoAdminTransferPending`] -- no nomination is in progress.
    ///
    /// # Events
    /// Emits `admin_cancelled` with the cancelled pending admin.
    pub fn cancel_admin_transfer(env: Env, caller: Address) -> Result<(), CheckpointError> {
        Self::require_admin(&env, &caller)?;
        Self::ensure_migration_finalized(&env)?;

        let pending: Address = env
            .storage()
            .instance()
            .get(&StorageKey::PendingAdmin)
            .ok_or(CheckpointError::NoAdminTransferPending)?;

        env.storage().instance().remove(&StorageKey::PendingAdmin);

        env.events()
            .publish((events::event_admin_cancelled(&env), caller, pending), ());

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Checkpoint creation
    // -----------------------------------------------------------------------

    /// Create a single immutable audit checkpoint recording a balance snapshot.
    ///
    /// The checkpoint is assigned a unique sequential ID, written to persistent
    /// storage with a 6-month TTL, and emitted as an event. Once written,
    /// checkpoints are **never updated** -- the log is append-only.
    ///
    /// # Parameters
    /// * `caller` -- Must be the current admin; must authorize.
    /// * `subject` -- The address whose balance is being snapshotted.
    /// * `token` -- The token contract address (e.g., USDC).
    /// * `balance` -- The snapshotted balance in token base units. Must be >= 0.
    /// * `metadata` -- A free-form tag (e.g., "monthly", "audit-q3").
    ///   Soroban [`Symbol`] values are protocol-limited to 32 characters.
    ///
    /// # Returns
    /// The `u64` ID assigned to the newly created checkpoint.
    ///
    /// # Errors
    /// * [`CheckpointError::Unauthorized`] -- caller is not the current admin.
    /// * [`CheckpointError::NotInitialized`] -- contract not initialized.
    /// * [`CheckpointError::AmountNegative`] -- balance is negative.
    /// * [`CheckpointError::Overflow`] -- ID counter overflowed `u64`.
    ///
    /// # Events
    /// Emits `checkpoint_created` with `subject` as topic and the full
    /// [`CheckpointRecord`] as data.
    pub fn create_checkpoint(
        env: Env,
        caller: Address,
        subject: Address,
        token: Address,
        balance: i128,
        metadata: Symbol,
    ) -> Result<u64, CheckpointError> {
        Self::require_admin(&env, &caller)?;
        Self::ensure_migration_finalized(&env)?;

        if balance < 0 {
            return Err(CheckpointError::AmountNegative);
        }

        let id = Self::next_checkpoint_id(&env)?;
        let timestamp = env.ledger().timestamp();

        let record = CheckpointRecord {
            id,
            subject: subject.clone(),
            token: token.clone(),
            balance,
            timestamp,
            metadata: metadata.clone(),
        };

        let key = StorageKey::Checkpoint(id);
        env.storage().persistent().set(&key, &record);
        env.storage()
            .persistent()
            .extend_ttl(&key, LIFETIME_THRESHOLD, BUMP_AMOUNT);

        // Maintain the per-subject index so get_checkpoints_for_subject can
        // return this subject's history without a full-range scan.
        Self::append_to_subject_index(&env, &subject, id);
        Self::append_to_digest(&env, &record);

        env.events()
            .publish((events::event_checkpoint_created(&env), subject), record);

        Ok(id)
    }

    /// Create multiple immutable audit checkpoints in a single atomic transaction.
    ///
    /// All validation runs **before** any checkpoint is written -- a failure on any
    /// item leaves the contract state unchanged.
    ///
    /// # Parameters
    /// * `caller` -- Must be the current admin; must authorize.
    /// * `items` -- Vector of `(subject, token, balance, metadata)` tuples.
    ///   1 to [`MAX_BATCH_SIZE`] entries.
    ///
    /// # Returns
    /// A vector of `u64` checkpoint IDs in the same order as `items`.
    ///
    /// # Errors
    /// * [`CheckpointError::BatchEmpty`] -- `items` is empty.
    /// * [`CheckpointError::BatchTooLarge`] -- `items` exceeds [`MAX_BATCH_SIZE`].
    /// * [`CheckpointError::Unauthorized`] -- caller is not the current admin.
    /// * [`CheckpointError::NotInitialized`] -- contract not initialized.
    /// * [`CheckpointError::AmountNegative`] -- any balance is negative.
    /// * [`CheckpointError::Overflow`] -- ID counter overflowed `u64`.
    ///
    /// # Events
    /// Emits one `checkpoint_created` event per item.
    pub fn batch_create_checkpoints(
        env: Env,
        caller: Address,
        items: Vec<(Address, Address, i128, Symbol)>,
    ) -> Result<Vec<u64>, CheckpointError> {
        Self::require_admin(&env, &caller)?;
        Self::ensure_migration_finalized(&env)?;

        let n = items.len();
        if n == 0 {
            return Err(CheckpointError::BatchEmpty);
        }
        if n > MAX_BATCH_SIZE {
            return Err(CheckpointError::BatchTooLarge);
        }

        // Validate all items before touching state.
        for item in items.iter() {
            let (_, _, balance, _metadata) = item;
            if balance < 0 {
                return Err(CheckpointError::AmountNegative);
            }
        }

        let mut ids: Vec<u64> = Vec::new(&env);
        let timestamp = env.ledger().timestamp();

        for item in items.iter() {
            let (subject, token, balance, metadata) = item;
            let id = Self::next_checkpoint_id(&env)?;

            let record = CheckpointRecord {
                id,
                subject: subject.clone(),
                token: token.clone(),
                balance,
                timestamp,
                metadata: metadata.clone(),
            };

            let key = StorageKey::Checkpoint(id);
            env.storage().persistent().set(&key, &record);
            env.storage()
                .persistent()
                .extend_ttl(&key, LIFETIME_THRESHOLD, BUMP_AMOUNT);

            // Maintain the per-subject index and digest chain for each item,
            // in the same order as `items`.
            Self::append_to_subject_index(&env, &subject, id);
            Self::append_to_digest(&env, &record);

            env.events().publish(
                (events::event_checkpoint_created(&env), subject.clone()),
                record,
            );

            ids.push_back(id);
        }

        Ok(ids)
    }

    // -----------------------------------------------------------------------
    // Pruning
    // -----------------------------------------------------------------------

    /// Permanently remove old checkpoint records, retaining a verifiable
    /// digest chain over everything ever created.
    ///
    /// Records with `id < id` are removed, at most [`MAX_PRUNE_BATCH`] (and at
    /// most `max_remove`) per call. The boundary only advances forward, so
    /// repeated calls with the same `id` progressively finish a large prune.
    ///
    /// Alongside the records, the per-subject indexes are trimmed so offset
    /// paging stays dense. The chain head itself is never rewritten: each
    /// pruned chain record is folded into [`StorageKey::DigestAtBoundary`]
    /// instead, which lets a consumer holding records from `PrunedBefore`
    /// onward reproduce [`StorageKey::DigestHead`] by continuing the fold.
    ///
    /// Checkpoints created before the digest feature existed are outside the
    /// chain: the chain starts at the first post-upgrade checkpoint recorded
    /// in [`StorageKey::ChainStartId`]. Pruning such legacy records removes
    /// them without changing either digest.
    ///
    /// # Parameters
    /// * `caller` -- Must be the current admin; must authorize.
    /// * `id` -- Exclusive upper bound of the range to prune. Must satisfy
    ///   `PrunedBefore <= id <= latest_id + 1`.
    /// * `max_remove` -- Maximum records to remove in this call; capped at
    ///   [`MAX_PRUNE_BATCH`].
    ///
    /// # Errors
    /// * [`CheckpointError::Unauthorized`] -- caller is not the current admin.
    /// * [`CheckpointError::NotInitialized`] -- contract not initialized.
    /// * [`CheckpointError::MigrationPending`] -- an upgrade is not finalized.
    /// * [`CheckpointError::PruneOutOfRange`] -- `id` is backwards or past the
    ///   latest checkpoint.
    /// * [`CheckpointError::InvalidMaxRemove`] -- `max_remove` is zero.
    ///
    /// # Events
    /// Emits `checkpoints_pruned` with `(from_id, to_id)` as topics and the
    /// digest at the new boundary as data.
    pub fn prune_before(
        env: Env,
        caller: Address,
        id: u64,
        max_remove: u32,
    ) -> Result<(), CheckpointError> {
        Self::require_admin(&env, &caller)?;
        Self::ensure_migration_finalized(&env)?;

        if max_remove == 0 {
            return Err(CheckpointError::InvalidMaxRemove);
        }

        let from_id = Self::pruned_before(&env);
        let latest_id: u64 = env
            .storage()
            .instance()
            .get(&StorageKey::NextCheckpointId)
            .unwrap_or(0);
        let upper_bound = latest_id.saturating_add(1);
        if id < from_id || id > upper_bound {
            return Err(CheckpointError::PruneOutOfRange);
        }

        let effective_max = u64::from(max_remove.min(MAX_PRUNE_BATCH));
        let to_id = id.min(from_id.saturating_add(effective_max));

        let chain_start = Self::chain_start_id(&env);
        let mut boundary = Self::digest_at_boundary(&env);
        let mut touched_subjects: Vec<Address> = Vec::new(&env);

        for current_id in from_id..to_id {
            let key = StorageKey::Checkpoint(current_id);
            if let Some(record) = env.storage().persistent().get::<_, CheckpointRecord>(&key) {
                if current_id >= chain_start {
                    boundary = Self::hash_record(&env, &boundary, &record);
                }
                if !touched_subjects
                    .iter()
                    .any(|subject| subject == record.subject)
                {
                    touched_subjects.push_back(record.subject.clone());
                }
                env.storage().persistent().remove(&key);
            }
        }

        for subject in touched_subjects.iter() {
            Self::remove_pruned_ids_from_subject_index(&env, &subject, from_id, to_id);
        }

        env.storage()
            .instance()
            .set(&StorageKey::PrunedBefore, &to_id);
        env.storage()
            .instance()
            .set(&StorageKey::DigestAtBoundary, &boundary);

        env.events().publish(
            (events::event_checkpoints_pruned(&env), from_id, to_id),
            boundary,
        );

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Checkpoint queries
    // -----------------------------------------------------------------------

    /// Return a single checkpoint record by its unique ID.
    ///
    /// Reading a checkpoint bumps its persistent TTL (buffer #26): audit
    /// records that are queried often but rarely rewritten would otherwise
    /// keep counting down to archival on the strength of their original
    /// write-time bump alone. A read extends the same [`LIFETIME_THRESHOLD`]
    /// / [`BUMP_AMOUNT`] window used at write time.
    ///
    /// # Errors
    /// * [`CheckpointError::CheckpointNotFound`] -- no checkpoint exists with
    ///   the requested ID.
    /// * [`CheckpointError::CheckpointPruned`] -- the ID existed but was pruned.
    pub fn get_checkpoint(env: Env, id: u64) -> Result<CheckpointRecord, CheckpointError> {
        Self::bump_instance(&env);
        Self::ensure_migration_finalized(&env)?;
        let key = StorageKey::Checkpoint(id);
        let record = env.storage().persistent().get(&key).ok_or_else(|| {
            if id >= 1 && id < Self::pruned_before(&env) {
                CheckpointError::CheckpointPruned
            } else {
                CheckpointError::CheckpointNotFound
            }
        })?;

        env.storage()
            .persistent()
            .extend_ttl(&key, LIFETIME_THRESHOLD, BUMP_AMOUNT);

        Ok(record)
    }

    /// Return a paginated range of checkpoint records.
    ///
    /// Retrieves up to `limit` checkpoints starting from `start_id`
    /// (inclusive). The result never exceeds [`MAX_PAGE_SIZE`] entries
    /// regardless of the requested limit.
    ///
    /// Checkpoint IDs are sequential starting at 1, so a query with
    /// `start_id = 1, limit = 50` returns the first 50 checkpoints
    /// (or fewer if fewer exist).
    ///
    /// Every record returned has its persistent TTL bumped (buffer #26),
    /// same as [`CalloraCheckpoint::get_checkpoint`].
    ///
    /// # Parameters
    /// * `start_id` -- The first checkpoint ID to retrieve (inclusive, 1-based).
    /// * `limit` -- Maximum number of records to return. Capped at
    ///   [`MAX_PAGE_SIZE`].
    ///
    /// # Errors
    /// * [`CheckpointError::InvalidPageSize`] -- `limit` is zero.
    pub fn get_checkpoints_range(
        env: Env,
        start_id: u64,
        limit: u32,
    ) -> Result<Vec<CheckpointRecord>, CheckpointError> {
        Self::bump_instance(&env);
        Self::ensure_migration_finalized(&env)?;
        if limit == 0 {
            return Err(CheckpointError::InvalidPageSize);
        }

        let effective_limit = limit.min(MAX_PAGE_SIZE);
        let count = Self::get_checkpoint_count(env.clone())?;

        // If start exceeds total count, return empty.
        if start_id > count {
            return Ok(Vec::new(&env));
        }

        let end_id = start_id
            .checked_add(effective_limit as u64)
            .ok_or(CheckpointError::Overflow)?
            .min(count.saturating_add(1));

        let mut result: Vec<CheckpointRecord> = Vec::new(&env);
        for id in start_id..end_id {
            let key = StorageKey::Checkpoint(id);
            if let Some(record) = env.storage().persistent().get(&key) {
                env.storage()
                    .persistent()
                    .extend_ttl(&key, LIFETIME_THRESHOLD, BUMP_AMOUNT);
                result.push_back(record);
            }
        }

        Ok(result)
    }

    /// Return a paginated list of checkpoint records for a specific subject.
    ///
    /// The result is ordered by creation order (ascending checkpoint ID).
    /// Returns an empty [`Vec`] when the subject has no recorded checkpoints.
    ///
    /// Reading the subject index bumps its persistent TTL. Every checkpoint
    /// record returned also has its TTL bumped (buffer #26), consistent with
    /// [`CalloraCheckpoint::get_checkpoint`] and
    /// [`CalloraCheckpoint::get_checkpoints_range`].
    ///
    /// # Parameters
    /// * `subject` -- The address whose checkpoint history is requested.
    /// * `start` -- Zero-based offset into the subject's ID list. Pass `0`
    ///   to begin from the earliest checkpoint.
    /// * `limit` -- Maximum number of records to return. Capped at
    ///   [`MAX_PAGE_SIZE`].
    ///
    /// # Errors
    /// * [`CheckpointError::InvalidPageSize`] -- `limit` is zero.
    pub fn get_checkpoints_for_subject(
        env: Env,
        subject: Address,
        start: u32,
        limit: u32,
    ) -> Result<Vec<CheckpointRecord>, CheckpointError> {
        Self::bump_instance(&env);
        Self::ensure_migration_finalized(&env)?;
        if limit == 0 {
            return Err(CheckpointError::InvalidPageSize);
        }

        let index_key = StorageKey::SubjectIndex(subject.clone());

        // No index entry means the subject has no checkpoints yet — return empty.
        let all_ids: Vec<u64> = match env.storage().persistent().get(&index_key) {
            Some(ids) => ids,
            None => return Ok(Vec::new(&env)),
        };

        // Bump the index entry's TTL on every read so it stays alive as long
        // as the checkpoints it references.
        env.storage()
            .persistent()
            .extend_ttl(&index_key, LIFETIME_THRESHOLD, BUMP_AMOUNT);

        let total = all_ids.len();
        if start >= total || total == 0 {
            return Ok(Vec::new(&env));
        }

        let effective_limit = limit.min(MAX_PAGE_SIZE);
        let end = (start.saturating_add(effective_limit)).min(total);

        let mut result: Vec<CheckpointRecord> = Vec::new(&env);
        for idx in start..end {
            let id = all_ids.get(idx).unwrap();
            let key = StorageKey::Checkpoint(id);
            if let Some(record) = env.storage().persistent().get(&key) {
                env.storage()
                    .persistent()
                    .extend_ttl(&key, LIFETIME_THRESHOLD, BUMP_AMOUNT);
                result.push_back(record);
            }
        }

        Ok(result)
    }

    /// Return the total number of checkpoints that have been created.
    ///
    /// This is an O(1) instance-storage read. It tracks the highest ID
    /// assigned so far (which is equal to the count since IDs are sequential
    /// starting at 1).
    pub fn get_checkpoint_count(env: Env) -> Result<u64, CheckpointError> {
        Self::bump_instance(&env);
        Self::ensure_migration_finalized(&env)?;
        Ok(env
            .storage()
            .instance()
            .get(&StorageKey::CheckpointCount)
            .unwrap_or(0))
    }

    /// Return the most recent checkpoint ID.
    ///
    /// Returns `0` when no checkpoints have been created yet.
    pub fn get_latest_checkpoint_id(env: Env) -> Result<u64, CheckpointError> {
        Self::bump_instance(&env);
        Self::ensure_migration_finalized(&env)?;
        Ok(env
            .storage()
            .instance()
            .get(&StorageKey::NextCheckpointId)
            .unwrap_or(0))
    }

    /// Return the most recently created checkpoint record, or `None` when
    /// no checkpoints exist.
    ///
    /// This is a convenience wrapper around
    /// [`CalloraCheckpoint::get_latest_checkpoint_id`] and
    /// [`CalloraCheckpoint::get_checkpoint`], and so also bumps the
    /// returned record's persistent TTL (buffer #26).
    pub fn get_latest_checkpoint(env: Env) -> Result<Option<CheckpointRecord>, CheckpointError> {
        Self::bump_instance(&env);
        Self::ensure_migration_finalized(&env)?;
        let latest_id = Self::get_latest_checkpoint_id(env.clone())?;
        if latest_id == 0 || latest_id < Self::pruned_before(&env) {
            return Ok(None);
        }
        Ok(Some(Self::get_checkpoint(env, latest_id)?))
    }

    /// Return the current digest chain head over all chain checkpoints.
    ///
    /// Before any checkpoint is created this is the zero 32-byte digest.
    /// Consumers can reproduce it by folding the records from
    /// [`CalloraCheckpoint::get_digest_at_boundary`] forward, starting at
    /// `max(`[`CalloraCheckpoint::get_pruned_before`],
    /// [`CalloraCheckpoint::get_chain_start_id`])`.
    pub fn get_digest_head(env: Env) -> Result<BytesN<32>, CheckpointError> {
        Self::bump_instance(&env);
        Self::ensure_migration_finalized(&env)?;
        Ok(Self::digest_head(&env))
    }

    /// Return the chain head as of the last pruned chain checkpoint.
    ///
    /// This is the seed for an off-chain recomputation of
    /// [`CalloraCheckpoint::get_digest_head`] from retained records.
    pub fn get_digest_at_boundary(env: Env) -> Result<BytesN<32>, CheckpointError> {
        Self::bump_instance(&env);
        Self::ensure_migration_finalized(&env)?;
        Ok(Self::digest_at_boundary(&env))
    }

    /// Return the first checkpoint ID that has not been pruned.
    ///
    /// Monotonic: all IDs below the returned value have been removed.
    pub fn get_pruned_before(env: Env) -> Result<u64, CheckpointError> {
        Self::bump_instance(&env);
        Self::ensure_migration_finalized(&env)?;
        Ok(Self::pruned_before(&env))
    }

    /// Return the first checkpoint ID included in the digest chain.
    ///
    /// Equals 1 for contracts initialized with digest support. After a
    /// migration it is the first ID assigned post-upgrade; earlier checkpoints
    /// are readable/prunable but are not part of the chain.
    pub fn get_chain_start_id(env: Env) -> Result<u64, CheckpointError> {
        Self::bump_instance(&env);
        Self::ensure_migration_finalized(&env)?;
        Ok(Self::chain_start_id(&env))
    }

    // -----------------------------------------------------------------------
    // TTL management (buffer top-up)
    // -----------------------------------------------------------------------

    /// Extend the TTL of a single checkpoint record.
    ///
    /// Allows the admin to "top up" the persistent storage lifetime of a
    /// checkpoint so that important audit records do not expire. After the
    /// call the checkpoint's TTL is reset to [`BUMP_AMOUNT`] ledgers
    /// (â‰ˆ6 months).
    ///
    /// # Parameters
    /// * `caller` -- Must be the current admin; must authorize.
    /// * `checkpoint_id` -- The ID of the checkpoint to bump.
    ///
    /// # Errors
    /// * [`CheckpointError::NotInitialized`] -- contract not initialized.
    /// * [`CheckpointError::Unauthorized`] -- caller is not the current admin.
    /// * [`CheckpointError::CheckpointNotFound`] -- no checkpoint with the
    ///   given ID exists.
    pub fn bump_checkpoint_ttl(
        env: Env,
        caller: Address,
        checkpoint_id: u64,
    ) -> Result<(), CheckpointError> {
        Self::require_admin(&env, &caller)?;
        Self::ensure_migration_finalized(&env)?;

        let key = StorageKey::Checkpoint(checkpoint_id);
        if !env.storage().persistent().has(&key) {
            return Err(
                if checkpoint_id >= 1 && checkpoint_id < Self::pruned_before(&env) {
                    CheckpointError::CheckpointPruned
                } else {
                    CheckpointError::CheckpointNotFound
                },
            );
        }

        env.storage()
            .persistent()
            .extend_ttl(&key, LIFETIME_THRESHOLD, BUMP_AMOUNT);

        Ok(())
    }

    /// Extend the TTL of a range of checkpoint records.
    ///
    /// Convenience wrapper around [`CalloraCheckpoint::bump_checkpoint_ttl`]
    /// for bulk operations. Iterates from `start_id` to `end_id` (inclusive)
    /// and bumps every checkpoint that exists. Missing IDs in the range are
    /// silently skipped so callers can use the current checkpoint count as
    /// `end_id` without worrying about gaps.
    ///
    /// # Parameters
    /// * `caller` -- Must be the current admin; must authorize.
    /// * `start_id` -- First checkpoint ID to bump (inclusive, 1-based).
    /// * `end_id` -- Last checkpoint ID to bump (inclusive).
    ///
    /// # Errors
    /// * [`CheckpointError::NotInitialized`] -- contract not initialized.
    /// * [`CheckpointError::Unauthorized`] -- caller is not the current admin.
    /// * [`CheckpointError::InvalidPageSize`] -- `start_id` is greater than `end_id`.
    pub fn bump_checkpoints_ttl_range(
        env: Env,
        caller: Address,
        start_id: u64,
        end_id: u64,
    ) -> Result<(), CheckpointError> {
        Self::require_admin(&env, &caller)?;
        Self::ensure_migration_finalized(&env)?;

        if start_id > end_id {
            return Err(CheckpointError::InvalidPageSize);
        }

        for id in start_id..=end_id {
            let key = StorageKey::Checkpoint(id);
            if env.storage().persistent().has(&key) {
                env.storage()
                    .persistent()
                    .extend_ttl(&key, LIFETIME_THRESHOLD, BUMP_AMOUNT);
            }
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Upgrade
    // -----------------------------------------------------------------------

    /// Authorize an admin-gated upgrade and replace the contract WASM.
    ///
    /// # Parameters
    /// * `caller` -- Must be the current admin; must authorize.
    /// * `new_wasm_hash` -- The 32-byte hash of the replacement WASM.
    /// * `target_version` -- Storage-layout version implemented by the replacement.
    ///
    /// # Errors
    /// * [`CheckpointError::Unauthorized`] -- caller is not the current admin.
    /// * [`CheckpointError::NotInitialized`] -- contract not initialized.
    /// * [`CheckpointError::VersionSkip`] -- target skips a migration version.
    /// * [`CheckpointError::SchemaMismatch`] -- recorded and current layouts differ.
    /// * [`CheckpointError::MigrationPending`] -- an earlier upgrade is not finalized.
    ///
    /// # Events
    /// Emits `upgraded` with the admin as topic and the new WASM hash as data.
    pub fn upgrade(
        env: Env,
        caller: Address,
        new_wasm_hash: BytesN<32>,
        target_version: u32,
    ) -> Result<(), CheckpointError> {
        Self::require_admin(&env, &caller)?;
        Self::ensure_migration_finalized(&env)?;

        let current_layout_hash = Self::storage_layout_hash(&env);
        let expected_source_layout_hash =
            if StorageMigrationValidator::current_version(&env) == LEGACY_VERSION {
                callora_storage_migration::zero_layout_hash(&env)
            } else {
                current_layout_hash.clone()
            };
        StorageMigrationValidator::validate_before_upgrade(
            &env,
            target_version,
            &current_layout_hash,
            &expected_source_layout_hash,
            &new_wasm_hash,
            false,
        )
        .map_err(Self::migration_error)?;

        env.deployer()
            .update_current_contract_wasm(new_wasm_hash.clone());

        env.events().publish(
            (events::event_upgraded(&env), Self::admin(&env)?),
            new_wasm_hash,
        );

        Ok(())
    }

    /// Finalize an authorized upgrade in the new WASM before business calls resume.
    ///
    /// # Parameters
    /// * `caller` -- Must be the current admin; must authorize.
    /// * `target_version` -- Version authorized by [`CalloraCheckpoint::upgrade`].
    /// * `new_layout_hash` -- Hash of the storage schema implemented by this WASM.
    ///
    /// # Errors
    /// * [`CheckpointError::Unauthorized`] -- caller is not the current admin.
    /// * [`CheckpointError::UnauthorizedUpgradeState`] -- no matching upgrade is authorized.
    /// * [`CheckpointError::SchemaMismatch`] -- supplied hash is not this WASM's layout.
    /// * [`CheckpointError::StorageUnreadable`] -- storage validation failed.
    pub fn migrate(
        env: Env,
        caller: Address,
        target_version: u32,
        new_layout_hash: BytesN<32>,
    ) -> Result<(), CheckpointError> {
        Self::require_admin(&env, &caller)?;

        if new_layout_hash != Self::storage_layout_hash(&env) {
            return Err(CheckpointError::SchemaMismatch);
        }

        let (authorized_version, wasm_hash) = StorageMigrationValidator::authorized_upgrade(&env)
            .ok_or(CheckpointError::UnauthorizedUpgradeState)?;
        if authorized_version != target_version {
            return Err(CheckpointError::UnauthorizedUpgradeState);
        }

        StorageMigrationValidator::finalize_migration(
            &env,
            target_version,
            &new_layout_hash,
            &wasm_hash,
        )
        .map_err(Self::migration_error)?;

        env.storage()
            .instance()
            .remove(&callora_storage_migration::StorageKey::SmAuthorizedUpgrade);

        // Checkpoints written before the digest feature existed are not part
        // of the chain. Seed the new keys here so the chain starts at the
        // first checkpoint created after this migration.
        let inst = env.storage().instance();
        if !inst.has(&StorageKey::DigestHead) {
            inst.set(&StorageKey::DigestHead, &Self::zero_digest(&env));
            inst.set(&StorageKey::DigestAtBoundary, &Self::zero_digest(&env));
            inst.set(&StorageKey::PrunedBefore, &1u64);
            let latest_id: u64 = inst.get(&StorageKey::NextCheckpointId).unwrap_or(0);
            inst.set(&StorageKey::ChainStartId, &latest_id.saturating_add(1));
        }

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

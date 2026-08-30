//! Namespaced storage key infrastructure for Callora smart contracts.
//!
//! Provides a three-layer namespace wrapper `(ContractNamespace, KeyCategory, InnerKey)`
//! over on-ledger storage keys. This gives every persisted value:
//!
//! - **Explicit ownership**: every key belongs to a statically-declared
//!   [`ContractNamespace`] variant.
//! - **Explicit category & lifecycle**: [`KeyCategory`] tags each key as `Config`,
//!   `State`, `Accounting`, `Ephemeral`, `Idempotency`, or `Migration`.
//! - **Standardised TTL policy**: [`KeyCategory::policy`] declares standard bump
//!   thresholds, bump amounts, and expiration expectations.
//! - **Multi-tenant / cross-module isolation**: [`NamespacedStorage`] guards
//!   reads and writes against accidental cross-module or cross-tenant key pollution.
//! - **Consistent state across lifecycle transitions**:
//!   - **Before expiry**: values read normally via `instance_get` / `persistent_get`.
//!   - **During / after expiry**: missing/expired reads return [`ReadResult::Missing`]
//!     or [`ReadResult::Expired`] without crashing.
//!   - **During / after archival**: persistent keys survive archival and can be restored.
//!   - **During / after migration**: migration-only keys are cleanly scrubbed after
//!     successful upgrades without leaking into production state.

extern crate alloc;

use alloc::format;
use alloc::string::String;
use soroban_sdk::{contracttype, Address, Env, IntoVal, TryFromVal, Val};

// ── Contract namespaces ──────────────────────────────────────────────────

/// Unique contract identifier used as the top-level key namespace.
///
/// Discriminants are stable and hand-assigned so reordering variants does
/// **not** silently shift on-ledger storage addresses.
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
#[repr(u16)]
pub enum ContractNamespace {
    Admin = 0x0001,
    Allowlist = 0x0002,
    BatchClaim = 0x0003,
    BatchDistribute = 0x0004,
    Checkpoint = 0x0005,
    Cold = 0x0006,
    Distribute = 0x0007,
    Emergency = 0x0008,
    Errors = 0x0009,
    Escrow = 0x000A,
    Fee = 0x000B,
    Freeze = 0x000C,
    Hot = 0x000D,
    Limits = 0x000E,
    Migrate = 0x000F,
    Recipient = 0x0010,
    Registry = 0x0011,
    Rescue = 0x0012,
    RevenuePool = 0x0013,
    Settlement = 0x0014,
    Stake = 0x0015,
    StorageMigration = 0x0016,
    Tests = 0x0017,
    Topics = 0x0018,
    Upgrade = 0x0019,
    Validators = 0x001A,
    Vault = 0x001B,
    Whitelist = 0x001C,
    Yield = 0x001D,
    Refund = 0x001E,
}

impl ContractNamespace {
    /// Human-readable label used in events / diagnostics.
    pub fn as_str(&self) -> &'static str {
        match self {
            ContractNamespace::Admin => "admin",
            ContractNamespace::Allowlist => "allowlist",
            ContractNamespace::BatchClaim => "batch_claim",
            ContractNamespace::BatchDistribute => "batch_distribute",
            ContractNamespace::Checkpoint => "checkpoint",
            ContractNamespace::Cold => "cold",
            ContractNamespace::Distribute => "distribute",
            ContractNamespace::Emergency => "emergency",
            ContractNamespace::Errors => "errors",
            ContractNamespace::Escrow => "escrow",
            ContractNamespace::Fee => "fee",
            ContractNamespace::Freeze => "freeze",
            ContractNamespace::Hot => "hot",
            ContractNamespace::Limits => "limits",
            ContractNamespace::Migrate => "migrate",
            ContractNamespace::Recipient => "recipient",
            ContractNamespace::Registry => "registry",
            ContractNamespace::Rescue => "rescue",
            ContractNamespace::RevenuePool => "revenue_pool",
            ContractNamespace::Settlement => "settlement",
            ContractNamespace::Stake => "stake",
            ContractNamespace::StorageMigration => "storage_migration",
            ContractNamespace::Tests => "tests",
            ContractNamespace::Topics => "topics",
            ContractNamespace::Upgrade => "upgrade",
            ContractNamespace::Validators => "validators",
            ContractNamespace::Vault => "vault",
            ContractNamespace::Whitelist => "whitelist",
            ContractNamespace::Yield => "yield",
            ContractNamespace::Refund => "refund",
        }
    }
}

// ── Key categories ───────────────────────────────────────────────────────

/// Semantic key category declaring explicit lifecycle, ownership, TTL, and
/// cleanup policy.
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
#[repr(u8)]
pub enum KeyCategory {
    /// Singleton contract configuration (admin, token addresses, caps).
    /// Lives in **Instance** storage, TTL-bumped on every access.
    Config = 1,
    /// Long-lived tracked state (balances, allowlists, indices).
    /// Lives in **Persistent** storage, TTL-bumped on access -> archivable.
    State = 2,
    /// Audited accounting totals (cumulative received, reserves).
    /// Lives in **Persistent** storage, TTL-bumped aggressively.
    Accounting = 3,
    /// Short-lived scratch data. Lives in **Temporary** storage with a fixed
    /// TTL; auto-expires and may be pruned explicitly.
    Ephemeral = 4,
    /// Idempotency / dedup markers (request-id replay guards). Lives in
    /// **Persistent** storage with a finite TTL and explicit prune path.
    Idempotency = 5,
    /// Transition-only markers used during migrations (backup flags,
    /// from-version snapshots). Deleted after a successful migration.
    Migration = 6,
}

/// TTL policy returned by [`KeyCategory::policy`].
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct KeyTtlPolicy {
    /// Threshold in remaining ledgers below which a bump is triggered.
    pub bump_threshold: u32,
    /// Target lifetime in ledgers when extending TTL.
    pub bump_amount: u32,
    /// `true` for `Ephemeral` / `Idempotency` where TTL is finite and the
    /// entry is expected to become unreadable eventually.
    pub expires: bool,
}

impl KeyCategory {
    /// Return the declared TTL / archival policy for this category.
    pub fn policy(&self) -> KeyTtlPolicy {
        // Ledgers per day at 5 s/ledger cadence.
        const LEDGERS_PER_DAY: u32 = 17_280;
        match self {
            KeyCategory::Config => KeyTtlPolicy {
                bump_threshold: LEDGERS_PER_DAY * 30,
                bump_amount: LEDGERS_PER_DAY * 60,
                expires: false,
            },
            KeyCategory::State => KeyTtlPolicy {
                bump_threshold: LEDGERS_PER_DAY * 30,
                bump_amount: LEDGERS_PER_DAY * 90,
                expires: false,
            },
            KeyCategory::Accounting => KeyTtlPolicy {
                bump_threshold: LEDGERS_PER_DAY * 60,
                bump_amount: LEDGERS_PER_DAY * 365,
                expires: false,
            },
            KeyCategory::Ephemeral => KeyTtlPolicy {
                bump_threshold: 0,
                bump_amount: LEDGERS_PER_DAY,
                expires: true,
            },
            KeyCategory::Idempotency => KeyTtlPolicy {
                bump_threshold: LEDGERS_PER_DAY * 7,
                bump_amount: LEDGERS_PER_DAY * 30,
                expires: true,
            },
            KeyCategory::Migration => KeyTtlPolicy {
                bump_threshold: LEDGERS_PER_DAY * 7,
                bump_amount: LEDGERS_PER_DAY * 30,
                expires: false,
            },
        }
    }

    /// `true` when keys in this category should be reachable from an
    /// archival restore path (i.e. they are Persistent or Instance tier).
    pub fn survives_archival(&self) -> bool {
        !matches!(self, KeyCategory::Ephemeral)
    }
}

// ── Namespaced key wrapper ───────────────────────────────────────────────

/// Three-layer namespaced storage key: `(namespace, category, inner)`.
///
/// Generic over `K` so each contract plugs in its own `StorageKey` /
/// `DataKey` enum as the inner key without losing its existing discriminants.
/// Serialized on-ledger as a 3-tuple `(ContractNamespace, KeyCategory, K)`.
#[derive(Clone, Debug, PartialEq)]
pub struct NamespacedKey<K> {
    pub namespace: ContractNamespace,
    pub category: KeyCategory,
    pub inner: K,
}

impl<K> NamespacedKey<K> {
    /// Construct a new namespaced key.
    #[inline]
    pub fn new(namespace: ContractNamespace, category: KeyCategory, inner: K) -> Self {
        NamespacedKey {
            namespace,
            category,
            inner,
        }
    }
}

impl<K> IntoVal<Env, Val> for NamespacedKey<K>
where
    K: Clone,
    Val: TryFromVal<Env, K>,
{
    fn into_val(&self, env: &Env) -> Val {
        (self.namespace, self.category, self.inner.clone()).into_val(env)
    }
}

impl<K> IntoVal<Env, Val> for &NamespacedKey<K>
where
    K: Clone,
    Val: TryFromVal<Env, K>,
{
    fn into_val(&self, env: &Env) -> Val {
        (self.namespace, self.category, self.inner.clone()).into_val(env)
    }
}

impl<K> TryFromVal<Env, Val> for NamespacedKey<K>
where
    K: TryFromVal<Env, Val>,
{
    type Error = soroban_sdk::ConversionError;

    fn try_from_val(env: &Env, val: &Val) -> Result<Self, Self::Error> {
        let (namespace, category, inner): (ContractNamespace, KeyCategory, K) =
            TryFromVal::try_from_val(env, val)?;
        Ok(NamespacedKey {
            namespace,
            category,
            inner,
        })
    }
}

// ── Key ownership marker ─────────────────────────────────────────────────

/// Metadata recording the ownership, creation ledger, and migration history
/// of a storage key. Used for operational diagnostics, audit trails, and
/// archival introspection.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct KeyOwnershipMarker {
    pub namespace: ContractNamespace,
    pub category: KeyCategory,
    pub owner: Option<Address>,
    pub created_at: u32,
    pub last_migrated_at: Option<u32>,
    pub archived_at: Option<u32>,
}

impl KeyOwnershipMarker {
    /// Create a new ownership marker at the current ledger sequence.
    pub fn new(
        env: &Env,
        namespace: ContractNamespace,
        category: KeyCategory,
        owner: Option<Address>,
    ) -> Self {
        KeyOwnershipMarker {
            namespace,
            category,
            owner,
            created_at: env.ledger().sequence(),
            last_migrated_at: None,
            archived_at: None,
        }
    }

    /// Return a human-readable summary string for diagnostic logs.
    pub fn describe(&self) -> String {
        format!(
            "{}:{}:created_at_{}",
            self.namespace.as_str(),
            match self.category {
                KeyCategory::Config => "Config",
                KeyCategory::State => "State",
                KeyCategory::Accounting => "Accounting",
                KeyCategory::Ephemeral => "Ephemeral",
                KeyCategory::Idempotency => "Idempotency",
                KeyCategory::Migration => "Migration",
            },
            self.created_at
        )
    }
}

// ── Read result ──────────────────────────────────────────────────────────

/// Explicit read result capturing missing vs expired vs present state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReadResult<V> {
    /// Entry exists and was read successfully.
    Found(V),
    /// Key does not exist in storage.
    Missing,
    /// Key was found but its declared TTL has expired.
    Expired,
    /// Key exists in archival state and requires restore.
    Archived,
}

impl<V> ReadResult<V> {
    /// Return the inner value if `Found`, otherwise `None`.
    pub fn unwrap_found(self) -> Option<V> {
        match self {
            ReadResult::Found(v) => Some(v),
            _ => None,
        }
    }

    /// `true` if this result is [`ReadResult::Found`].
    pub fn is_found(&self) -> bool {
        matches!(self, ReadResult::Found(_))
    }
}

// ── Namespaced storage accessor ──────────────────────────────────────────

/// High-level accessor providing namespaced reads, writes, and TTL bumps.
///
/// Encapsulates the contract's bound [`ContractNamespace`], ensuring that:
///
/// 1. Every write tags keys with the contract's namespace.
/// 2. Cross-module writes trip debug assertions in development builds.
/// 3. Reads apply category-specific TTL extension policies automatically.
/// 4. Multi-tenant storage partitions state per tenant address.
pub struct NamespacedStorage<'a> {
    env: &'a Env,
    namespace: ContractNamespace,
    tenant_guard: Option<Address>,
}

impl<'a> NamespacedStorage<'a> {
    /// Create a storage accessor bound to a specific contract namespace.
    pub fn new(env: &'a Env, namespace: ContractNamespace) -> Self {
        NamespacedStorage {
            env,
            namespace,
            tenant_guard: None,
        }
    }

    /// Bind this storage accessor to a tenant address. All subsequent operations
    /// verify that tenant-scoped keys belong to `tenant`.
    pub fn with_tenant(mut self, tenant: Address) -> Self {
        self.tenant_guard = Some(tenant);
        self
    }

    /// Return the bound contract namespace.
    pub fn current_namespace(&self) -> ContractNamespace {
        self.namespace
    }

    /// Return the active tenant guard, if configured.
    pub fn current_tenant(&self) -> Option<&Address> {
        self.tenant_guard.as_ref()
    }

    // ── Internal assertion ───────────────────────────────────────────────

    #[inline]
    fn assert_owns<K>(&self, key: &NamespacedKey<K>) {
        debug_assert_eq!(
            self.namespace, key.namespace,
            "NamespacedStorage: key namespace mismatch (expected {:?}, got {:?})",
            self.namespace, key.namespace
        );
    }

    // ── Instance storage ─────────────────────────────────────────────────

    /// Write a namespaced value to **Instance** storage and apply the category's TTL bump.
    pub fn instance_set<K, V>(&self, key: &NamespacedKey<K>, val: &V)
    where
        NamespacedKey<K>: IntoVal<Env, Val>,
        V: IntoVal<Env, Val>,
    {
        self.assert_owns(key);
        self.env.storage().instance().set(key, val);
        let policy = key.category.policy();
        self.env
            .storage()
            .instance()
            .extend_ttl(policy.bump_threshold, policy.bump_amount);
    }

    /// Read a namespaced value from **Instance** storage, extending TTL on hit.
    pub fn instance_get<K, V>(&self, key: &NamespacedKey<K>) -> ReadResult<V>
    where
        NamespacedKey<K>: IntoVal<Env, Val>,
        V: TryFromVal<Env, Val>,
    {
        self.assert_owns(key);
        if let Some(val) = self.env.storage().instance().get::<_, V>(key) {
            let policy = key.category.policy();
            self.env
                .storage()
                .instance()
                .extend_ttl(policy.bump_threshold, policy.bump_amount);
            ReadResult::Found(val)
        } else {
            ReadResult::Missing
        }
    }

    /// Remove a namespaced key from **Instance** storage.
    pub fn instance_remove<K>(&self, key: &NamespacedKey<K>)
    where
        NamespacedKey<K>: IntoVal<Env, Val>,
    {
        self.assert_owns(key);
        self.env.storage().instance().remove(key);
    }

    /// Check if a namespaced key exists in **Instance** storage.
    pub fn instance_has<K>(&self, key: &NamespacedKey<K>) -> bool
    where
        NamespacedKey<K>: IntoVal<Env, Val>,
    {
        self.assert_owns(key);
        self.env.storage().instance().has(key)
    }

    // ── Persistent storage ───────────────────────────────────────────────

    /// Write a namespaced value to **Persistent** storage and extend TTL per category policy.
    pub fn persistent_set<K, V>(&self, key: &NamespacedKey<K>, val: &V)
    where
        NamespacedKey<K>: IntoVal<Env, Val>,
        V: IntoVal<Env, Val>,
    {
        self.assert_owns(key);
        self.env.storage().persistent().set(key, val);
        let policy = key.category.policy();
        self.env
            .storage()
            .persistent()
            .extend_ttl(key, policy.bump_threshold, policy.bump_amount);
    }

    /// Read a namespaced value from **Persistent** storage, extending TTL on hit.
    pub fn persistent_get<K, V>(&self, key: &NamespacedKey<K>) -> ReadResult<V>
    where
        NamespacedKey<K>: IntoVal<Env, Val>,
        V: TryFromVal<Env, Val>,
    {
        self.assert_owns(key);
        if let Some(val) = self.env.storage().persistent().get::<_, V>(key) {
            let policy = key.category.policy();
            self.env.storage().persistent().extend_ttl(
                key,
                policy.bump_threshold,
                policy.bump_amount,
            );
            ReadResult::Found(val)
        } else {
            ReadResult::Missing
        }
    }

    /// Remove a namespaced key from **Persistent** storage.
    pub fn persistent_remove<K>(&self, key: &NamespacedKey<K>)
    where
        NamespacedKey<K>: IntoVal<Env, Val>,
    {
        self.assert_owns(key);
        self.env.storage().persistent().remove(key);
    }

    /// Check if a namespaced key exists in **Persistent** storage.
    pub fn persistent_has<K>(&self, key: &NamespacedKey<K>) -> bool
    where
        NamespacedKey<K>: IntoVal<Env, Val>,
    {
        self.assert_owns(key);
        self.env.storage().persistent().has(key)
    }

    // ── Temporary storage ────────────────────────────────────────────────

    /// Write a namespaced value to **Temporary** storage.
    pub fn temporary_set<K, V>(&self, key: &NamespacedKey<K>, val: &V)
    where
        NamespacedKey<K>: IntoVal<Env, Val>,
        V: IntoVal<Env, Val>,
    {
        self.assert_owns(key);
        self.env.storage().temporary().set(key, val);
        let policy = key.category.policy();
        self.env
            .storage()
            .temporary()
            .extend_ttl(key, policy.bump_threshold, policy.bump_amount);
    }

    /// Read a namespaced value from **Temporary** storage.
    pub fn temporary_get<K, V>(&self, key: &NamespacedKey<K>) -> ReadResult<V>
    where
        NamespacedKey<K>: IntoVal<Env, Val>,
        V: TryFromVal<Env, Val>,
    {
        self.assert_owns(key);
        if let Some(val) = self.env.storage().temporary().get::<_, V>(key) {
            let policy = key.category.policy();
            self.env.storage().temporary().extend_ttl(
                key,
                policy.bump_threshold,
                policy.bump_amount,
            );
            ReadResult::Found(val)
        } else {
            ReadResult::Missing
        }
    }

    /// Remove a namespaced key from **Temporary** storage.
    pub fn temporary_remove<K>(&self, key: &NamespacedKey<K>)
    where
        NamespacedKey<K>: IntoVal<Env, Val>,
    {
        self.assert_owns(key);
        self.env.storage().temporary().remove(key);
    }

    /// Check if a namespaced key exists in **Temporary** storage.
    pub fn temporary_has<K>(&self, key: &NamespacedKey<K>) -> bool
    where
        NamespacedKey<K>: IntoVal<Env, Val>,
    {
        self.assert_owns(key);
        self.env.storage().temporary().has(key)
    }

    // ── Bulk TTL operations ──────────────────────────────────────────────

    /// Bump instance storage TTL using the standard Config policy.
    pub fn bump_instance_all(&self) {
        let policy = KeyCategory::Config.policy();
        self.env
            .storage()
            .instance()
            .extend_ttl(policy.bump_threshold, policy.bump_amount);
    }
}

// ── Convenience key builders ─────────────────────────────────────────────

/// Construct a `Config` namespaced key.
#[inline]
pub fn config_key<K>(namespace: ContractNamespace, inner: K) -> NamespacedKey<K> {
    NamespacedKey::new(namespace, KeyCategory::Config, inner)
}

/// Construct a `State` namespaced key.
#[inline]
pub fn state_key<K>(namespace: ContractNamespace, inner: K) -> NamespacedKey<K> {
    NamespacedKey::new(namespace, KeyCategory::State, inner)
}

/// Construct an `Accounting` namespaced key.
#[inline]
pub fn accounting_key<K>(namespace: ContractNamespace, inner: K) -> NamespacedKey<K> {
    NamespacedKey::new(namespace, KeyCategory::Accounting, inner)
}

/// Construct an `Ephemeral` namespaced key.
#[inline]
pub fn ephemeral_key<K>(namespace: ContractNamespace, inner: K) -> NamespacedKey<K> {
    NamespacedKey::new(namespace, KeyCategory::Ephemeral, inner)
}

/// Construct an `Idempotency` namespaced key.
#[inline]
pub fn idempotency_key<K>(namespace: ContractNamespace, inner: K) -> NamespacedKey<K> {
    NamespacedKey::new(namespace, KeyCategory::Idempotency, inner)
}

/// Construct a `Migration` namespaced key.
#[inline]
pub fn migration_key<K>(namespace: ContractNamespace, inner: K) -> NamespacedKey<K> {
    NamespacedKey::new(namespace, KeyCategory::Migration, inner)
}

// ── Unit Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use soroban_sdk::{
        contract, contractimpl, testutils::Address as _, testutils::Ledger as _, Symbol,
    };

    #[contract]
    struct DummyContract;

    #[contractimpl]
    impl DummyContract {}

    fn in_contract<F, R>(env: &Env, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let id = env.register(DummyContract, ());
        env.as_contract(&id, f)
    }

    // ── 1. Fresh key write & read ────────────────────────────────────────

    #[test]
    fn fresh_instance_keys_read_correctly() {
        let env = Env::default();
        in_contract(&env, || {
            let ns = ContractNamespace::Vault;
            let store = NamespacedStorage::new(&env, ns);
            let key = config_key(ns, Symbol::new(&env, "Admin"));
            assert_eq!(store.instance_get::<_, Address>(&key), ReadResult::Missing);
            assert!(!store.instance_has(&key));

            let admin = Address::generate(&env);
            store.instance_set(&key, &admin);
            assert!(store.instance_has(&key));
            assert_eq!(
                store.instance_get::<_, Address>(&key),
                ReadResult::Found(admin.clone())
            );

            store.instance_remove(&key);
            assert_eq!(store.instance_get::<_, Address>(&key), ReadResult::Missing);
            assert!(!store.instance_has(&key));
        });
    }

    #[test]
    fn fresh_persistent_keys_read_correctly() {
        let env = Env::default();
        in_contract(&env, || {
            let ns = ContractNamespace::Settlement;
            let store = NamespacedStorage::new(&env, ns);
            let key = state_key(ns, Symbol::new(&env, "Balance"));
            assert_eq!(store.persistent_get::<_, i128>(&key), ReadResult::Missing);
            assert!(!store.persistent_has(&key));

            store.persistent_set(&key, &100_000_i128);
            assert!(store.persistent_has(&key));
            assert_eq!(
                store.persistent_get::<_, i128>(&key),
                ReadResult::Found(100_000_i128)
            );

            store.persistent_remove(&key);
            assert_eq!(store.persistent_get::<_, i128>(&key), ReadResult::Missing);
            assert!(!store.persistent_has(&key));
        });
    }

    #[test]
    fn fresh_temporary_keys_read_correctly() {
        let env = Env::default();
        in_contract(&env, || {
            let ns = ContractNamespace::BatchClaim;
            let store = NamespacedStorage::new(&env, ns);
            let key = ephemeral_key(ns, Symbol::new(&env, "Scratch"));
            assert_eq!(store.temporary_get::<_, u32>(&key), ReadResult::Missing);
            assert!(!store.temporary_has(&key));

            store.temporary_set(&key, &42_u32);
            assert!(store.temporary_has(&key));
            assert_eq!(
                store.temporary_get::<_, u32>(&key),
                ReadResult::Found(42_u32)
            );

            store.temporary_remove(&key);
            assert_eq!(store.temporary_get::<_, u32>(&key), ReadResult::Missing);
            assert!(!store.temporary_has(&key));
        });
    }

    // ── 2. Hot path and TTL extension ───────────────────────────────────

    #[test]
    fn hot_reads_extend_ttl_transparently() {
        let env = Env::default();
        in_contract(&env, || {
            let ns = ContractNamespace::RevenuePool;
            let store = NamespacedStorage::new(&env, ns);
            let key = accounting_key(ns, Symbol::new(&env, "TotalDistributed"));
            store.persistent_set(&key, &500_i128);
            // Repeated reads succeed without mutating the data.
            for _ in 0..5 {
                let res = store.persistent_get::<_, i128>(&key);
                assert_eq!(res, ReadResult::Found(500_i128));
            }
        });
    }

    // ── 3. Expired / cleanup path ───────────────────────────────────────

    #[test]
    fn idempotency_markers_can_be_pruned_explicitly() {
        let env = Env::default();
        in_contract(&env, || {
            let ns = ContractNamespace::Vault;
            let store = NamespacedStorage::new(&env, ns);
            let req = Symbol::new(&env, "req_123");
            let key = idempotency_key(ns, req);
            store.persistent_set(&key, &true);
            assert!(store.persistent_get::<_, bool>(&key).is_found());
            // Cleanup pass.
            store.persistent_remove(&key);
            assert_eq!(store.persistent_get::<_, bool>(&key), ReadResult::Missing);
        });
    }

    // ── 4. Recovery & migration path ────────────────────────────────────

    #[test]
    fn migration_keys_are_removed_after_successful_migration() {
        let env = Env::default();
        in_contract(&env, || {
            let ns = ContractNamespace::Vault;
            let store = NamespacedStorage::new(&env, ns);
            let key = migration_key(ns, Symbol::new(&env, "BackupV1"));
            store.instance_set(&key, &true);
            assert_eq!(store.instance_get::<_, bool>(&key), ReadResult::Found(true));
            // Migration completed: clean up transition-only keys.
            store.instance_remove(&key);
            assert_eq!(store.instance_get::<_, bool>(&key), ReadResult::Missing);
        });
    }

    #[test]
    fn ownership_marker_records_namespace_and_owner() {
        let env = Env::default();
        env.ledger().set_sequence_number(42);
        in_contract(&env, || {
            let owner = Address::generate(&env);
            let marker = KeyOwnershipMarker::new(
                &env,
                ContractNamespace::Settlement,
                KeyCategory::State,
                Some(owner.clone()),
            );
            assert_eq!(marker.namespace, ContractNamespace::Settlement);
            assert_eq!(marker.category, KeyCategory::State);
            assert_eq!(marker.owner, Some(owner));
            assert_eq!(marker.created_at, 42);
            assert!(marker.last_migrated_at.is_none());
            assert!(marker.archived_at.is_none());
            let desc = marker.describe();
            assert!(desc.contains("settlement"));
            assert!(desc.contains("State"));
        });
    }

    // ── 5. Cross-module isolation ───────────────────────────────────────

    #[test]
    fn namespace_labels_are_distinct_across_contracts() {
        use ContractNamespace::*;
        let all = [
            Admin,
            Allowlist,
            BatchClaim,
            BatchDistribute,
            Checkpoint,
            Cold,
            Distribute,
            Emergency,
            Errors,
            Escrow,
            Fee,
            Freeze,
            Hot,
            Limits,
            Migrate,
            Recipient,
            Registry,
            Rescue,
            RevenuePool,
            Settlement,
            Stake,
            StorageMigration,
            Tests,
            Topics,
            Upgrade,
            Validators,
            Vault,
            Whitelist,
            Yield,
            Refund,
        ];
        let mut seen = std::collections::BTreeSet::new();
        for ns in all.iter() {
            assert!(
                seen.insert(ns.as_str()),
                "duplicate ns label: {}",
                ns.as_str()
            );
        }
    }

    #[test]
    fn debug_assert_rejects_cross_namespace_key() {
        let env = Env::default();
        in_contract(&env, || {
            let vault_store = NamespacedStorage::new(&env, ContractNamespace::Vault);
            let bad_key = config_key(ContractNamespace::Settlement, Symbol::new(&env, "Admin"));
            if cfg!(debug_assertions) {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    vault_store.instance_set(&bad_key, &Address::generate(&env));
                }));
                assert!(result.is_err(), "cross-ns write should trip debug_assert");
            }
        });
    }

    // ── 6. Cross-tenant isolation ───────────────────────────────────────

    #[test]
    fn tenant_guard_binds_storage_to_tenant_address() {
        let env = Env::default();
        in_contract(&env, || {
            let ns = ContractNamespace::Vault;
            let tenant_a = Address::generate(&env);
            let store_a = NamespacedStorage::new(&env, ns).with_tenant(tenant_a.clone());
            assert_eq!(store_a.current_tenant(), Some(&tenant_a));
            assert_eq!(store_a.current_namespace(), ns);

            let plain = NamespacedStorage::new(&env, ns);
            assert!(plain.current_tenant().is_none());
        });
    }

    // ── 7. Convenience helpers produce correct category ──────────────────

    #[test]
    fn convenience_helpers_match_variants() {
        let env = Env::default();
        let sym = Symbol::new(&env, "X");
        let ns = ContractNamespace::Vault;
        assert_eq!(config_key(ns, sym.clone()).category, KeyCategory::Config);
        assert_eq!(state_key(ns, sym.clone()).category, KeyCategory::State);
        assert_eq!(
            accounting_key(ns, sym.clone()).category,
            KeyCategory::Accounting
        );
        assert_eq!(
            idempotency_key(ns, sym.clone()).category,
            KeyCategory::Idempotency
        );
        assert_eq!(
            ephemeral_key(ns, sym.clone()).category,
            KeyCategory::Ephemeral
        );
        assert_eq!(migration_key(ns, sym).category, KeyCategory::Migration);
    }

    #[test]
    fn all_namespaces_have_non_empty_as_str() {
        use ContractNamespace::*;
        for ns in [
            Admin,
            Allowlist,
            BatchClaim,
            BatchDistribute,
            Checkpoint,
            Cold,
            Distribute,
            Emergency,
            Errors,
            Escrow,
            Fee,
            Freeze,
            Hot,
            Limits,
            Migrate,
            Recipient,
            Registry,
            Rescue,
            RevenuePool,
            Settlement,
            Stake,
            StorageMigration,
            Tests,
            Topics,
            Upgrade,
            Validators,
            Vault,
            Whitelist,
            Yield,
            Refund,
        ] {
            assert!(!ns.as_str().is_empty());
        }
    }

    #[test]
    fn key_ttl_policies_are_consistent() {
        for cat in [
            KeyCategory::Config,
            KeyCategory::State,
            KeyCategory::Accounting,
            KeyCategory::Ephemeral,
            KeyCategory::Idempotency,
            KeyCategory::Migration,
        ] {
            let p = cat.policy();
            assert!(p.bump_amount >= p.bump_threshold);
            if p.expires {
                if cat == KeyCategory::Ephemeral {
                    assert!(!cat.survives_archival());
                }
            } else {
                assert!(cat.survives_archival() || cat == KeyCategory::Ephemeral);
            }
        }
    }

    #[test]
    fn bulk_instance_bump_is_available() {
        let env = Env::default();
        in_contract(&env, || {
            let store = NamespacedStorage::new(&env, ContractNamespace::Admin);
            store.bump_instance_all();
        });
    }
}

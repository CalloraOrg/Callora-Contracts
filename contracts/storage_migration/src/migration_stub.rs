//! Shared migration stub helpers for Callora Soroban contracts.
//!
//! Provides plain-Rust building blocks for contracts that manage single-shot
//! legacy-to-current state reshapes and upgrade guards.

use soroban_sdk::{Address, BytesN, Env, IntoVal, Symbol, TryFromVal, Val};

use crate::error::StorageMigrationError;
use crate::{zero_layout_hash, StorageMigrationValidator};

/// Semantic error variants for generic migration stubs.
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum StubError {
    /// The contract has not been initialized.
    NotInitialized,
    /// Initialization was attempted more than once.
    AlreadyInitialized,
    /// The caller is not the configured administrator.
    Unauthorized,
    /// No legacy state was found in storage.
    LegacyStateMissing,
    /// The legacy balance or field value is invalid (negative).
    InvalidLegacyBalance,
    /// The supplied expected version does not match the stored version.
    VersionMismatch,
    /// The requested target version is not expected_version + 1.
    InvalidTargetVersion,
    /// Migration has already been performed for this state.
    AlreadyMigrated,
    /// No upgrade has been authorized for the requested version.
    UpgradeNotAuthorized,
    /// Arithmetic overflow detected during version increment.
    Overflow,
    /// Initial version exceeds the permitted ceiling.
    InvalidInitialVersion,
    /// The supplied WASM hash is all-zero bytes.
    WasmHashZero,
}

/// Storage-key mapping for a migration contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationKeys<K> {
    pub admin: K,
    pub version: K,
    pub legacy: K,
    pub current: K,
    pub auth_upgrade: K,
}

impl<K> MigrationKeys<K> {
    pub const fn new(admin: K, version: K, legacy: K, current: K, auth_upgrade: K) -> Self {
        Self {
            admin,
            version,
            legacy,
            current,
            auth_upgrade,
        }
    }

    /// Check administrator authorization for `caller`.
    pub fn require_admin(&self, env: &Env, caller: &Address) -> Result<Address, StubError>
    where
        K: IntoVal<Env, Val>,
    {
        require_admin(env, &self.admin, caller)
    }

    /// Initialize the migration stub.
    pub fn init(
        &self,
        env: &Env,
        admin: &Address,
        initial_version: u32,
        max_initial_version: Option<u32>,
    ) -> Result<(), StubError>
    where
        K: IntoVal<Env, Val>,
    {
        init(
            env,
            &self.admin,
            &self.version,
            admin,
            initial_version,
            max_initial_version,
        )
    }

    /// Reshape legacy data into current data and advance version.
    pub fn migrate<L, C, F>(
        &self,
        env: &Env,
        caller: &Address,
        expected_version: u32,
        target_version: u32,
        opts: &MigrateOptions,
        reshape: F,
    ) -> Result<C, StubError>
    where
        K: IntoVal<Env, Val>,
        L: TryFromVal<Env, Val>,
        C: IntoVal<Env, Val> + TryFromVal<Env, Val> + Clone,
        F: FnOnce(L) -> Result<C, StubError>,
    {
        migrate(
            env,
            self,
            caller,
            expected_version,
            target_version,
            opts,
            reshape,
        )
    }

    /// Authorize a contract upgrade for the current version.
    pub fn authorize_upgrade(
        &self,
        env: &Env,
        caller: &Address,
        target_version: u32,
        wasm_hash: &BytesN<32>,
        opts: &AuthorizeOptions,
    ) -> Result<(), StubError>
    where
        K: IntoVal<Env, Val>,
    {
        authorize_upgrade(env, self, caller, target_version, wasm_hash, opts)
    }

    /// Read the reshaped current state.
    pub fn get_current<C>(&self, env: &Env) -> Option<C>
    where
        K: IntoVal<Env, Val>,
        C: TryFromVal<Env, Val>,
    {
        get_current(env, &self.current)
    }

    /// Read the stored migration version.
    pub fn version(&self, env: &Env) -> Option<u32>
    where
        K: IntoVal<Env, Val>,
    {
        version(env, &self.version)
    }

    /// Check if a WASM hash is authorized for the current version.
    pub fn is_upgrade_authorized(&self, env: &Env, wasm_hash: &BytesN<32>) -> bool
    where
        K: IntoVal<Env, Val>,
    {
        is_upgrade_authorized(env, &self.version, &self.auth_upgrade, wasm_hash)
    }

    /// Validate an upgrade before swapping WASM.
    pub fn validate_before_upgrade(
        &self,
        env: &Env,
        caller: &Address,
        target_version: u32,
        wasm_hash: &BytesN<32>,
        allow_rollback: bool,
    ) -> Result<(), StorageMigrationError>
    where
        K: IntoVal<Env, Val>,
    {
        validate_before_upgrade(
            env,
            &self.admin,
            caller,
            target_version,
            wasm_hash,
            allow_rollback,
        )
    }
}

/// Options configuring `migrate` behavior.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MigrateOptions {
    /// Optional instance TTL extension parameters `(threshold, extend_to)`.
    pub ttl_extension: Option<(u32, u32)>,
    /// Whether to delete the legacy state key after successful reshape.
    pub remove_legacy: bool,
    /// Optional event topic to emit on migration success.
    pub event_topic: Option<Symbol>,
}

/// Options configuring `authorize_upgrade` behavior.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AuthorizeOptions {
    /// Whether to reject an all-zero WASM hash.
    pub reject_zero_hash: bool,
    /// Optional event topic to emit on authorization success.
    pub event_topic: Option<Symbol>,
}

/// Verify that `caller` is the stored administrator.
pub fn require_admin<K>(env: &Env, admin_key: &K, caller: &Address) -> Result<Address, StubError>
where
    K: IntoVal<Env, Val>,
{
    let admin: Address = env
        .storage()
        .instance()
        .get(admin_key)
        .ok_or(StubError::NotInitialized)?;
    if caller != &admin {
        return Err(StubError::Unauthorized);
    }
    Ok(admin)
}

/// Initialize the migration stub.
pub fn init<K>(
    env: &Env,
    admin_key: &K,
    version_key: &K,
    admin: &Address,
    initial_version: u32,
    max_initial_version: Option<u32>,
) -> Result<(), StubError>
where
    K: IntoVal<Env, Val>,
{
    admin.require_auth();
    if env.storage().instance().has(admin_key) {
        return Err(StubError::AlreadyInitialized);
    }
    if let Some(max) = max_initial_version {
        if initial_version > max {
            return Err(StubError::InvalidInitialVersion);
        }
    }
    env.storage().instance().set(admin_key, admin);
    env.storage().instance().set(version_key, &initial_version);
    Ok(())
}

/// Reshape legacy data into current data and advance version.
pub fn migrate<K, L, C, F>(
    env: &Env,
    keys: &MigrationKeys<K>,
    caller: &Address,
    expected_version: u32,
    target_version: u32,
    opts: &MigrateOptions,
    reshape: F,
) -> Result<C, StubError>
where
    K: IntoVal<Env, Val>,
    L: TryFromVal<Env, Val>,
    C: IntoVal<Env, Val> + TryFromVal<Env, Val> + Clone,
    F: FnOnce(L) -> Result<C, StubError>,
{
    caller.require_auth();
    require_admin(env, &keys.admin, caller)?;

    let version: u32 = env
        .storage()
        .instance()
        .get(&keys.version)
        .ok_or(StubError::NotInitialized)?;

    if version != expected_version {
        return Err(StubError::VersionMismatch);
    }

    let next_version = expected_version.checked_add(1).ok_or(StubError::Overflow)?;

    if target_version != next_version {
        return Err(StubError::InvalidTargetVersion);
    }

    if env.storage().instance().has(&keys.current) {
        return Err(StubError::AlreadyMigrated);
    }

    if let Some((threshold, extend_to)) = opts.ttl_extension {
        env.storage().instance().extend_ttl(threshold, extend_to);
    }

    let legacy: L = env
        .storage()
        .instance()
        .get(&keys.legacy)
        .ok_or(StubError::LegacyStateMissing)?;

    let current = reshape(legacy)?;

    env.storage().instance().set(&keys.current, &current);
    env.storage().instance().set(&keys.version, &target_version);

    if opts.remove_legacy {
        env.storage().instance().remove(&keys.legacy);
    }

    if let Some(topic) = &opts.event_topic {
        env.events().publish((topic.clone(),), target_version);
    }

    Ok(current)
}

/// Authorize a contract upgrade for the current migration version.
pub fn authorize_upgrade<K>(
    env: &Env,
    keys: &MigrationKeys<K>,
    caller: &Address,
    target_version: u32,
    wasm_hash: &BytesN<32>,
    opts: &AuthorizeOptions,
) -> Result<(), StubError>
where
    K: IntoVal<Env, Val>,
{
    caller.require_auth();
    require_admin(env, &keys.admin, caller)?;

    let version: u32 = env
        .storage()
        .instance()
        .get(&keys.version)
        .ok_or(StubError::NotInitialized)?;

    if target_version != version {
        return Err(StubError::VersionMismatch);
    }

    if opts.reject_zero_hash && *wasm_hash == zero_layout_hash(env) {
        return Err(StubError::WasmHashZero);
    }

    env.storage()
        .instance()
        .set(&keys.auth_upgrade, &(target_version, wasm_hash.clone()));

    if let Some(topic) = &opts.event_topic {
        env.events()
            .publish((topic.clone(), wasm_hash.clone()), target_version);
    }

    Ok(())
}

/// Return reshaped current data, if migrated.
pub fn get_current<K, C>(env: &Env, current_key: &K) -> Option<C>
where
    K: IntoVal<Env, Val>,
    C: TryFromVal<Env, Val>,
{
    env.storage().instance().get(current_key)
}

/// Return stored migration version, if initialized.
pub fn version<K>(env: &Env, version_key: &K) -> Option<u32>
where
    K: IntoVal<Env, Val>,
{
    env.storage().instance().get(version_key)
}

/// Check if a WASM hash is authorized for the current version.
pub fn is_upgrade_authorized<K>(
    env: &Env,
    version_key: &K,
    auth_key: &K,
    wasm_hash: &BytesN<32>,
) -> bool
where
    K: IntoVal<Env, Val>,
{
    let stored_version: Option<u32> = env.storage().instance().get(version_key);
    let auth: Option<(u32, BytesN<32>)> = env.storage().instance().get(auth_key);

    match (stored_version, auth) {
        (Some(version), Some((auth_version, auth_hash))) => {
            version == auth_version && *wasm_hash == auth_hash
        }
        _ => false,
    }
}

/// Validate pending upgrade before swapping WASM.
pub fn validate_before_upgrade<K>(
    env: &Env,
    admin_key: &K,
    caller: &Address,
    target_version: u32,
    wasm_hash: &BytesN<32>,
    allow_rollback: bool,
) -> Result<(), StorageMigrationError>
where
    K: IntoVal<Env, Val>,
{
    caller.require_auth();

    let admin: Address = env
        .storage()
        .instance()
        .get(admin_key)
        .ok_or(StorageMigrationError::NotInitialized)?;
    if caller != &admin {
        return Err(StorageMigrationError::Unauthorized);
    }

    let zero = zero_layout_hash(env);
    StorageMigrationValidator::validate_before_upgrade(
        env,
        target_version,
        &zero,
        &zero,
        wasm_hash,
        allow_rollback,
    )
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{contract, contracterror, contractimpl, contracttype};

    #[contracttype]
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub enum TestKey {
        Admin,
        Version,
        Legacy,
        Current,
        Auth,
    }

    #[contracttype]
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct TestLegacy {
        pub amount: i128,
        pub tag: u64,
    }

    #[contracttype]
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct TestCurrent {
        pub amount: i128,
        pub tag: u64,
        pub extra: i128,
    }

    #[contracterror]
    #[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
    #[repr(u32)]
    pub enum TestError {
        NotInitialized = 1,
        AlreadyInitialized = 2,
        Unauthorized = 3,
        LegacyStateMissing = 4,
        InvalidLegacyBalance = 5,
        VersionMismatch = 6,
        InvalidTargetVersion = 7,
        AlreadyMigrated = 8,
        UpgradeNotAuthorized = 9,
        Overflow = 10,
        InvalidInitialVersion = 11,
        WasmHashZero = 12,
    }

    impl From<StubError> for TestError {
        fn from(e: StubError) -> Self {
            match e {
                StubError::NotInitialized => TestError::NotInitialized,
                StubError::AlreadyInitialized => TestError::AlreadyInitialized,
                StubError::Unauthorized => TestError::Unauthorized,
                StubError::LegacyStateMissing => TestError::LegacyStateMissing,
                StubError::InvalidLegacyBalance => TestError::InvalidLegacyBalance,
                StubError::VersionMismatch => TestError::VersionMismatch,
                StubError::InvalidTargetVersion => TestError::InvalidTargetVersion,
                StubError::AlreadyMigrated => TestError::AlreadyMigrated,
                StubError::UpgradeNotAuthorized => TestError::UpgradeNotAuthorized,
                StubError::Overflow => TestError::Overflow,
                StubError::InvalidInitialVersion => TestError::InvalidInitialVersion,
                StubError::WasmHashZero => TestError::WasmHashZero,
            }
        }
    }

    const KEYS: MigrationKeys<TestKey> = MigrationKeys::new(
        TestKey::Admin,
        TestKey::Version,
        TestKey::Legacy,
        TestKey::Current,
        TestKey::Auth,
    );

    #[contract]
    pub struct TestContract;

    #[contractimpl]
    impl TestContract {
        pub fn init(
            env: Env,
            admin: Address,
            initial_version: u32,
            max_version: Option<u32>,
        ) -> Result<(), TestError> {
            KEYS.init(&env, &admin, initial_version, max_version)
                .map_err(Into::into)
        }

        pub fn migrate(
            env: Env,
            caller: Address,
            expected_version: u32,
            target_version: u32,
        ) -> Result<TestCurrent, TestError> {
            KEYS.migrate(
                &env,
                &caller,
                expected_version,
                target_version,
                &MigrateOptions {
                    ttl_extension: None,
                    remove_legacy: true,
                    event_topic: Some(Symbol::new(&env, "migrated")),
                },
                |l: TestLegacy| {
                    if l.amount < 0 {
                        return Err(StubError::InvalidLegacyBalance);
                    }
                    Ok(TestCurrent {
                        amount: l.amount,
                        tag: l.tag,
                        extra: 0,
                    })
                },
            )
            .map_err(Into::into)
        }

        pub fn authorize_upgrade(
            env: Env,
            caller: Address,
            target_version: u32,
            wasm_hash: BytesN<32>,
            reject_zero: bool,
        ) -> Result<(), TestError> {
            KEYS.authorize_upgrade(
                &env,
                &caller,
                target_version,
                &wasm_hash,
                &AuthorizeOptions {
                    reject_zero_hash: reject_zero,
                    event_topic: Some(Symbol::new(&env, "upgraded")),
                },
            )
            .map_err(Into::into)
        }

        pub fn get_current(env: Env) -> Option<TestCurrent> {
            KEYS.get_current(&env)
        }

        pub fn version(env: Env) -> Option<u32> {
            KEYS.version(&env)
        }

        pub fn is_upgrade_authorized(env: Env, wasm_hash: BytesN<32>) -> bool {
            KEYS.is_upgrade_authorized(&env, &wasm_hash)
        }

        pub fn validate_before_upgrade(
            env: Env,
            caller: Address,
            target_version: u32,
            wasm_hash: BytesN<32>,
        ) -> Result<(), StorageMigrationError> {
            KEYS.validate_before_upgrade(&env, &caller, target_version, &wasm_hash, false)
        }
    }

    fn setup(env: &Env) -> (Address, Address, TestContractClient<'_>) {
        env.mock_all_auths();
        let admin = Address::generate(env);
        let contract_id = env.register(TestContract, ());
        let client = TestContractClient::new(env, &contract_id);
        (admin, contract_id, client)
    }

    #[test]
    fn helper_init_and_double_init() {
        let env = Env::default();
        let (admin, _, client) = setup(&env);
        assert_eq!(client.version(), None);
        client.init(&admin, &1, &Some(10));
        assert_eq!(client.version(), Some(1));

        // Double init
        let res = client.try_init(&admin, &2, &None);
        assert!(res.is_err());
    }

    #[test]
    fn helper_init_rejects_exceeded_max_version() {
        let env = Env::default();
        let (admin, _, client) = setup(&env);
        let res = client.try_init(&admin, &11, &Some(10));
        assert!(res.is_err());
    }

    #[test]
    fn helper_migrate_happy_path_and_options() {
        let env = Env::default();
        let (admin, contract_id, client) = setup(&env);
        client.init(&admin, &1, &None);

        env.as_contract(&contract_id, || {
            env.storage().instance().set(
                &TestKey::Legacy,
                &TestLegacy {
                    amount: 100,
                    tag: 42,
                },
            );
        });

        let current = client.migrate(&admin, &1, &2);
        assert_eq!(current.amount, 100);
        assert_eq!(client.version(), Some(2));
        assert_eq!(client.get_current(), Some(current));

        let legacy_has = env.as_contract(&contract_id, || {
            env.storage().instance().has(&TestKey::Legacy)
        });
        assert!(!legacy_has);
    }

    #[test]
    fn helper_migrate_guards() {
        let env = Env::default();
        let (admin, contract_id, client) = setup(&env);
        let stranger = Address::generate(&env);

        // Not initialized
        let res = client.try_migrate(&admin, &1, &2);
        assert!(res.is_err());

        client.init(&admin, &1, &None);

        // Unauthorized
        let res = client.try_migrate(&stranger, &1, &2);
        assert!(res.is_err());

        // Version mismatch
        let res = client.try_migrate(&admin, &0, &1);
        assert!(res.is_err());

        // Version skip
        let res = client.try_migrate(&admin, &1, &3);
        assert!(res.is_err());

        // Missing legacy
        let res = client.try_migrate(&admin, &1, &2);
        assert!(res.is_err());

        // Negative balance
        env.as_contract(&contract_id, || {
            env.storage().instance().set(
                &TestKey::Legacy,
                &TestLegacy {
                    amount: -10,
                    tag: 42,
                },
            );
        });
        let res = client.try_migrate(&admin, &1, &2);
        assert!(res.is_err());
    }

    #[test]
    fn helper_authorize_upgrade_and_views() {
        let env = Env::default();
        let (admin, _, client) = setup(&env);
        client.init(&admin, &1, &None);
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let zero = zero_layout_hash(&env);

        // Rejects zero hash when strict
        let res = client.try_authorize_upgrade(&admin, &1, &zero, &true);
        assert!(res.is_err());

        // Accepts zero hash when lenient
        client.authorize_upgrade(&admin, &1, &zero, &false);
        assert!(client.is_upgrade_authorized(&zero));
        assert!(!client.is_upgrade_authorized(&hash));

        // Authorize good hash
        client.authorize_upgrade(&admin, &1, &hash, &true);
        assert!(client.is_upgrade_authorized(&hash));
        assert!(!client.is_upgrade_authorized(&zero));
    }

    #[test]
    fn helper_validate_before_upgrade() {
        let env = Env::default();
        let (admin, contract_id, client) = setup(&env);
        client.init(&admin, &1, &None);
        let hash = BytesN::from_array(&env, &[2u8; 32]);
        client.validate_before_upgrade(&admin, &2, &hash);
        let auth = env.as_contract(&contract_id, || {
            StorageMigrationValidator::is_upgrade_authorized(&env, 2, &hash)
        });
        assert!(auth);
    }
}

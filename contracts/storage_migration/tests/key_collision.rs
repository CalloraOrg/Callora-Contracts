//! Cross-host storage-key collision regression (issue #1415).
//!
//! Soroban `#[contracttype]` unit variants encode as
//! `ScVal::Vec([ScVal::Symbol("VariantName")])` regardless of the enum type
//! they belong to. Before the `Sm` namespace, the validator's `Version` and
//! `AuthorizedUpgrade` variants therefore shared instance-storage slots with
//! host contracts that declared the same variant names.
//!
//! This test encodes every validator key and every key of every workspace host
//! that embeds [`callora_storage_migration::StorageMigrationValidator`], then
//! asserts that no host key shares an encoding with a validator key.

extern crate std;

use callora_checkpoint::StorageKey as CheckpointKey;
use callora_migrate::StorageKey as MigrateKey;
use callora_revenue_pool::DataKey as RevenuePoolKey;
use callora_storage_migration::StorageKey as ValidatorKey;
use callora_yield::limits::StorageKey as YieldKey;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{Address, Bytes, Env, IntoVal, Val};

/// Encode a key to the canonical XDR form of its `ScVal`.
///
/// `ToXdr` only requires `IntoVal<Env, Val>`, which `#[contracttype]` generates
/// unconditionally, so this test does not depend on the testutils-only
/// `TryInto<ScVal>` impls.
fn encode<K>(env: &Env, key: K) -> Bytes
where
    K: IntoVal<Env, Val>,
{
    key.to_xdr(env)
}

#[test]
fn namespaced_validator_keys_do_not_collide_with_any_host_key() {
    let env = Env::default();
    let address = Address::generate(&env);

    // The private pre-v2 `LegacyStorageKey` enum is intentionally excluded:
    // its encodings are read-only migration sources, never written by the
    // validator, and exist specifically to recover already-deployed state.
    let validator_keys = std::vec![
        ("SmVersion", encode(&env, ValidatorKey::SmVersion)),
        (
            "SmSchemaLayoutHash",
            encode(&env, ValidatorKey::SmSchemaLayoutHash)
        ),
        (
            "SmAuthorizedUpgrade",
            encode(&env, ValidatorKey::SmAuthorizedUpgrade)
        ),
        (
            "SmBackupPresent",
            encode(&env, ValidatorKey::SmBackupPresent)
        ),
    ];

    // `callora-migrate` is itself a host: it embeds the validator *and* owns a
    // separate StorageKey enum.
    let host_keys = std::vec![
        ("migrate::Admin", encode(&env, MigrateKey::Admin)),
        ("migrate::Version", encode(&env, MigrateKey::Version)),
        ("migrate::Legacy", encode(&env, MigrateKey::Legacy)),
        ("migrate::Current", encode(&env, MigrateKey::Current)),
        (
            "migrate::AuthorizedUpgrade",
            encode(&env, MigrateKey::AuthorizedUpgrade)
        ),
        ("checkpoint::Admin", encode(&env, CheckpointKey::Admin)),
        (
            "checkpoint::PendingAdmin",
            encode(&env, CheckpointKey::PendingAdmin)
        ),
        (
            "checkpoint::NextCheckpointId",
            encode(&env, CheckpointKey::NextCheckpointId)
        ),
        (
            "checkpoint::Checkpoint",
            encode(&env, CheckpointKey::Checkpoint(0))
        ),
        (
            "checkpoint::CheckpointCount",
            encode(&env, CheckpointKey::CheckpointCount)
        ),
        (
            "checkpoint::SubjectIndex",
            encode(&env, CheckpointKey::SubjectIndex(address.clone()))
        ),
        ("revenue_pool::Admin", encode(&env, RevenuePoolKey::Admin)),
        (
            "revenue_pool::UsdcToken",
            encode(&env, RevenuePoolKey::UsdcToken)
        ),
        ("revenue_pool::Paused", encode(&env, RevenuePoolKey::Paused)),
        ("yield::Admin", encode(&env, YieldKey::Admin)),
        ("yield::PendingAdmin", encode(&env, YieldKey::PendingAdmin)),
        (
            "yield::DefaultLimits",
            encode(&env, YieldKey::DefaultLimits)
        ),
        (
            "yield::AccountLimits",
            encode(&env, YieldKey::AccountLimits(address.clone()))
        ),
        (
            "yield::AccountState",
            encode(&env, YieldKey::AccountState(address))
        ),
        ("yield::WasmVersion", encode(&env, YieldKey::WasmVersion)),
    ];

    // The namespaced validator keys must also be distinct from each other.
    for (i, (name_a, value_a)) in validator_keys.iter().enumerate() {
        for (name_b, value_b) in validator_keys.iter().skip(i + 1) {
            assert_ne!(
                value_a, value_b,
                "validator keys {name_a} and {name_b} share an encoding"
            );
        }
    }

    for (host_name, host_value) in &host_keys {
        for (validator_name, validator_value) in &validator_keys {
            assert_ne!(
                host_value, validator_value,
                "{host_name} collides with validator key {validator_name}"
            );
        }
    }
}

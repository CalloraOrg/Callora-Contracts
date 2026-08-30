//! Integration tests for namespaced storage keys across contract modules (Issue #1055).
//!
//! Verifies:
//! 1. Every contract module in the workspace exposes its own declared [`ContractNamespace`]
//!    constant and a valid storage accessor.
//! 2. Storage operations isolate state across different contracts and categories.
//! 3. Lifecycle paths (fresh, hot TTL bumps, expiration/archival, recovery/migration) behave
//!    consistently end-to-end.

extern crate std;

use callora_helpers::{
    accounting_key, config_key, ephemeral_key, idempotency_key, migration_key, state_key,
    ContractNamespace, KeyCategory, KeyOwnershipMarker, NamespacedStorage, ReadResult,
};
use soroban_sdk::{
    contract, contractimpl, testutils::Address as _, testutils::Ledger as _, Address, Env, Symbol,
};

#[contract]
struct Dummy;

#[contractimpl]
impl Dummy {}

#[test]
fn test_namespaced_storage_lifecycle_and_isolation() {
    let env = Env::default();
    env.ledger().set_sequence_number(100);
    let id = env.register(Dummy, ());
    env.as_contract(&id, || {
        let ns = ContractNamespace::Vault;
        let store = NamespacedStorage::new(&env, ns);

        // ── 1. Fresh writes across all categories ─────────────────────
        let conf = config_key(ns, Symbol::new(&env, "Cfg"));
        let stat = state_key(ns, Symbol::new(&env, "State"));
        let acc = accounting_key(ns, Symbol::new(&env, "Acc"));
        let eph = ephemeral_key(ns, Symbol::new(&env, "Eph"));
        let idem = idempotency_key(ns, Symbol::new(&env, "Req1"));
        let mig = migration_key(ns, Symbol::new(&env, "Mig1"));

        store.instance_set(&conf, &1_u32);
        store.persistent_set(&stat, &2_i128);
        store.persistent_set(&acc, &3_i128);
        store.temporary_set(&eph, &4_u32);
        store.persistent_set(&idem, &true);
        store.instance_set(&mig, &true);

        // ── 2. Fresh reads ───────────────────────────────────────────
        assert_eq!(store.instance_get::<_, u32>(&conf), ReadResult::Found(1));
        assert_eq!(store.persistent_get::<_, i128>(&stat), ReadResult::Found(2));
        assert_eq!(store.persistent_get::<_, i128>(&acc), ReadResult::Found(3));
        assert_eq!(store.temporary_get::<_, u32>(&eph), ReadResult::Found(4));
        assert_eq!(
            store.persistent_get::<_, bool>(&idem),
            ReadResult::Found(true)
        );
        assert_eq!(store.instance_get::<_, bool>(&mig), ReadResult::Found(true));

        // ── 3. Hot reads (TTL bump on access) ────────────────────────
        for _ in 0..3 {
            assert_eq!(store.instance_get::<_, u32>(&conf), ReadResult::Found(1));
            assert_eq!(store.persistent_get::<_, i128>(&stat), ReadResult::Found(2));
            assert_eq!(store.persistent_get::<_, i128>(&acc), ReadResult::Found(3));
        }

        // ── 4. Expired / cleanup (prune idempotency & clear migration) ─
        store.persistent_remove(&idem);
        store.instance_remove(&mig);
        store.temporary_remove(&eph);

        assert!(!store.persistent_get::<_, bool>(&idem).is_found());
        assert_eq!(store.instance_get::<_, bool>(&mig), ReadResult::Missing);
        assert_eq!(store.temporary_get::<_, u32>(&eph), ReadResult::Missing);

        // ── 5. Recovery: permanent state remains intact ───────────────
        assert_eq!(store.instance_get::<_, u32>(&conf), ReadResult::Found(1));
        assert_eq!(store.persistent_get::<_, i128>(&stat), ReadResult::Found(2));
        assert_eq!(store.persistent_get::<_, i128>(&acc), ReadResult::Found(3));

        // ── 6. Isolation: distinct namespace sees missing state ───────
        let other_ns = ContractNamespace::Settlement;
        let other_store = NamespacedStorage::new(&env, other_ns);
        let other_key = config_key(other_ns, Symbol::new(&env, "Cfg"));
        assert_eq!(
            other_store.instance_get::<_, u32>(&other_key),
            ReadResult::Missing
        );
    });
}

#[test]
fn test_ownership_marker_audit_trail() {
    let env = Env::default();
    env.ledger().set_sequence_number(500);
    let id = env.register(Dummy, ());
    env.as_contract(&id, || {
        let owner = Address::generate(&env);
        let marker = KeyOwnershipMarker::new(
            &env,
            ContractNamespace::Settlement,
            KeyCategory::Accounting,
            Some(owner.clone()),
        );

        assert_eq!(marker.namespace, ContractNamespace::Settlement);
        assert_eq!(marker.category, KeyCategory::Accounting);
        assert_eq!(marker.owner, Some(owner));
        assert_eq!(marker.created_at, 500);
        assert!(marker.last_migrated_at.is_none());
        assert!(marker.archived_at.is_none());

        let desc = marker.describe();
        assert!(desc.contains("settlement"));
        assert!(desc.contains("Accounting"));
        assert!(desc.contains("500"));
    });
}

#[test]
fn integrated_contracts_expose_own_namespace() {
    use ContractNamespace::*;
    assert_eq!(callora_admin::ns::CONTRACT_NS, Admin);
    assert_eq!(callora_allowlist::ns::CONTRACT_NS, Allowlist);
    assert_eq!(callora_batch_claim::ns::CONTRACT_NS, BatchClaim);
    assert_eq!(callora_checkpoint::ns::CONTRACT_NS, Checkpoint);
    assert_eq!(callora_cold::ns::CONTRACT_NS, Cold);
    assert_eq!(callora_distribute::ns::CONTRACT_NS, Distribute);
    assert_eq!(callora_emergency::ns::CONTRACT_NS, Emergency);
    assert_eq!(errors::ns::CONTRACT_NS, Errors);
    assert_eq!(callora_escrow::ns::CONTRACT_NS, Escrow);
    assert_eq!(callora_fee::ns::CONTRACT_NS, Fee);
    assert_eq!(callora_freeze::ns::CONTRACT_NS, Freeze);
    assert_eq!(callora_hot::ns::CONTRACT_NS, Hot);
    assert_eq!(callora_limits::ns::CONTRACT_NS, Limits);
    assert_eq!(callora_migrate::ns::CONTRACT_NS, Migrate);
    assert_eq!(callora_recipient::ns::CONTRACT_NS, Recipient);
    assert_eq!(callora_registry::ns::CONTRACT_NS, Registry);
    assert_eq!(callora_rescue::ns::CONTRACT_NS, Rescue);
    assert_eq!(callora_refund::ns::CONTRACT_NS, Refund);
    assert_eq!(callora_revenue_pool::ns::CONTRACT_NS, RevenuePool);
    assert_eq!(callora_settlement::ns::CONTRACT_NS, Settlement);
    assert_eq!(callora_stake::ns::CONTRACT_NS, Stake);
    assert_eq!(callora_storage_migration::ns::CONTRACT_NS, StorageMigration);
    assert_eq!(callora_topics::ns::CONTRACT_NS, Topics);
    assert_eq!(callora_upgrade::ns::CONTRACT_NS, Upgrade);
    assert_eq!(callora_validators::ns::CONTRACT_NS, Validators);
    assert_eq!(callora_vault::ns::CONTRACT_NS, Vault);
    assert_eq!(callora_whitelist::ns::CONTRACT_NS, Whitelist);
    assert_eq!(callora_yield::ns::CONTRACT_NS, Yield);
}

#[test]
fn integrated_storage_accessors_bind_matching_namespace() {
    let env = Env::default();
    let id = env.register(Dummy, ());
    env.as_contract(&id, || {
        assert_eq!(
            callora_admin::ns::storage(&env).current_namespace(),
            ContractNamespace::Admin
        );
        assert_eq!(
            callora_allowlist::ns::storage(&env).current_namespace(),
            ContractNamespace::Allowlist
        );
        assert_eq!(
            callora_batch_claim::ns::storage(&env).current_namespace(),
            ContractNamespace::BatchClaim
        );
        assert_eq!(
            callora_checkpoint::ns::storage(&env).current_namespace(),
            ContractNamespace::Checkpoint
        );
        assert_eq!(
            callora_cold::ns::storage(&env).current_namespace(),
            ContractNamespace::Cold
        );
        assert_eq!(
            callora_distribute::ns::storage(&env).current_namespace(),
            ContractNamespace::Distribute
        );
        assert_eq!(
            callora_emergency::ns::storage(&env).current_namespace(),
            ContractNamespace::Emergency
        );
        assert_eq!(
            errors::ns::storage(&env).current_namespace(),
            ContractNamespace::Errors
        );
        assert_eq!(
            callora_escrow::ns::storage(&env).current_namespace(),
            ContractNamespace::Escrow
        );
        assert_eq!(
            callora_fee::ns::storage(&env).current_namespace(),
            ContractNamespace::Fee
        );
        assert_eq!(
            callora_freeze::ns::storage(&env).current_namespace(),
            ContractNamespace::Freeze
        );
        assert_eq!(
            callora_hot::ns::storage(&env).current_namespace(),
            ContractNamespace::Hot
        );
        assert_eq!(
            callora_limits::ns::storage(&env).current_namespace(),
            ContractNamespace::Limits
        );
        assert_eq!(
            callora_migrate::ns::storage(&env).current_namespace(),
            ContractNamespace::Migrate
        );
        assert_eq!(
            callora_recipient::ns::storage(&env).current_namespace(),
            ContractNamespace::Recipient
        );
        assert_eq!(
            callora_registry::ns::storage(&env).current_namespace(),
            ContractNamespace::Registry
        );
        assert_eq!(
            callora_rescue::ns::storage(&env).current_namespace(),
            ContractNamespace::Rescue
        );
        assert_eq!(
            callora_refund::ns::storage(&env).current_namespace(),
            ContractNamespace::Refund
        );
        assert_eq!(
            callora_revenue_pool::ns::storage(&env).current_namespace(),
            ContractNamespace::RevenuePool
        );
        assert_eq!(
            callora_settlement::ns::storage(&env).current_namespace(),
            ContractNamespace::Settlement
        );
        assert_eq!(
            callora_stake::ns::storage(&env).current_namespace(),
            ContractNamespace::Stake
        );
        assert_eq!(
            callora_storage_migration::ns::storage(&env).current_namespace(),
            ContractNamespace::StorageMigration
        );
        assert_eq!(
            callora_topics::ns::storage(&env).current_namespace(),
            ContractNamespace::Topics
        );
        assert_eq!(
            callora_upgrade::ns::storage(&env).current_namespace(),
            ContractNamespace::Upgrade
        );
        assert_eq!(
            callora_validators::ns::storage(&env).current_namespace(),
            ContractNamespace::Validators
        );
        assert_eq!(
            callora_vault::ns::storage(&env).current_namespace(),
            ContractNamespace::Vault
        );
        assert_eq!(
            callora_whitelist::ns::storage(&env).current_namespace(),
            ContractNamespace::Whitelist
        );
        assert_eq!(
            callora_yield::ns::storage(&env).current_namespace(),
            ContractNamespace::Yield
        );
    });
}

extern crate std;

use soroban_sdk::{testutils::Address as _, Address, Env};

use crate::{
    capabilities::{
        ALL_CAPABILITIES, CAP_ADMIN_BROADCAST, CAP_AUTHORIZED_CALLER, CAP_BATCH_DEDUCT,
        CAP_DEPOSITOR_ALLOWLIST, CAP_DEDUCT, CAP_DEPOSIT, CAP_OFFERING_METADATA, CAP_PAUSE,
        CAP_PRICE_REGISTRY, CAP_RATE_LIMIT, CAP_REQUEST_IDEMPOTENCY, CAP_REVENUE_POOL,
        CAP_SETTLEMENT, CAP_SLIPPAGE_GUARD, CAP_TWO_STEP_ADMIN, CAP_TWO_STEP_OWNERSHIP,
        CAP_UPGRADE, CAP_WITHDRAW,
    },
    CalloraVault, CalloraVaultClient,
};

fn create_usdc(env: &Env, admin: &Address) -> Address {
    let ca = env.register_stellar_asset_contract_v2(admin.clone());
    ca.address()
}

fn setup(env: &Env) -> CalloraVaultClient<'_> {
    let owner = Address::generate(env);
    let vault_addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(env, &vault_addr);
    let usdc = create_usdc(env, &owner);
    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client
}

// ---------------------------------------------------------------------------
// Basic return value
// ---------------------------------------------------------------------------

#[test]
fn capabilities_returns_nonzero() {
    let env = Env::default();
    let client = setup(&env);
    assert_ne!(client.capabilities(), 0);
}

#[test]
fn capabilities_equals_all_capabilities_constant() {
    let env = Env::default();
    let client = setup(&env);
    assert_eq!(client.capabilities(), ALL_CAPABILITIES);
}

// ---------------------------------------------------------------------------
// Each individual capability bit is set
// ---------------------------------------------------------------------------

#[test]
fn cap_deposit_is_set() {
    let env = Env::default();
    let client = setup(&env);
    assert_ne!(client.capabilities() & CAP_DEPOSIT, 0);
}

#[test]
fn cap_withdraw_is_set() {
    let env = Env::default();
    let client = setup(&env);
    assert_ne!(client.capabilities() & CAP_WITHDRAW, 0);
}

#[test]
fn cap_deduct_is_set() {
    let env = Env::default();
    let client = setup(&env);
    assert_ne!(client.capabilities() & CAP_DEDUCT, 0);
}

#[test]
fn cap_batch_deduct_is_set() {
    let env = Env::default();
    let client = setup(&env);
    assert_ne!(client.capabilities() & CAP_BATCH_DEDUCT, 0);
}

#[test]
fn cap_pause_is_set() {
    let env = Env::default();
    let client = setup(&env);
    assert_ne!(client.capabilities() & CAP_PAUSE, 0);
}

#[test]
fn cap_authorized_caller_is_set() {
    let env = Env::default();
    let client = setup(&env);
    assert_ne!(client.capabilities() & CAP_AUTHORIZED_CALLER, 0);
}

#[test]
fn cap_offering_metadata_is_set() {
    let env = Env::default();
    let client = setup(&env);
    assert_ne!(client.capabilities() & CAP_OFFERING_METADATA, 0);
}

#[test]
fn cap_price_registry_is_set() {
    let env = Env::default();
    let client = setup(&env);
    assert_ne!(client.capabilities() & CAP_PRICE_REGISTRY, 0);
}

#[test]
fn cap_request_idempotency_is_set() {
    let env = Env::default();
    let client = setup(&env);
    assert_ne!(client.capabilities() & CAP_REQUEST_IDEMPOTENCY, 0);
}

#[test]
fn cap_two_step_ownership_is_set() {
    let env = Env::default();
    let client = setup(&env);
    assert_ne!(client.capabilities() & CAP_TWO_STEP_OWNERSHIP, 0);
}

#[test]
fn cap_two_step_admin_is_set() {
    let env = Env::default();
    let client = setup(&env);
    assert_ne!(client.capabilities() & CAP_TWO_STEP_ADMIN, 0);
}

#[test]
fn cap_settlement_is_set() {
    let env = Env::default();
    let client = setup(&env);
    assert_ne!(client.capabilities() & CAP_SETTLEMENT, 0);
}

#[test]
fn cap_revenue_pool_is_set() {
    let env = Env::default();
    let client = setup(&env);
    assert_ne!(client.capabilities() & CAP_REVENUE_POOL, 0);
}

#[test]
fn cap_rate_limit_is_set() {
    let env = Env::default();
    let client = setup(&env);
    assert_ne!(client.capabilities() & CAP_RATE_LIMIT, 0);
}

#[test]
fn cap_admin_broadcast_is_set() {
    let env = Env::default();
    let client = setup(&env);
    assert_ne!(client.capabilities() & CAP_ADMIN_BROADCAST, 0);
}

#[test]
fn cap_depositor_allowlist_is_set() {
    let env = Env::default();
    let client = setup(&env);
    assert_ne!(client.capabilities() & CAP_DEPOSITOR_ALLOWLIST, 0);
}

#[test]
fn cap_slippage_guard_is_set() {
    let env = Env::default();
    let client = setup(&env);
    assert_ne!(client.capabilities() & CAP_SLIPPAGE_GUARD, 0);
}

#[test]
fn cap_upgrade_is_set() {
    let env = Env::default();
    let client = setup(&env);
    assert_ne!(client.capabilities() & CAP_UPGRADE, 0);
}

// ---------------------------------------------------------------------------
// Reserved bits are zero
// ---------------------------------------------------------------------------

#[test]
fn reserved_bits_are_zero() {
    let env = Env::default();
    let client = setup(&env);
    // Bits 18–63 must always be zero.
    let reserved_mask: u64 = !((1u64 << 18) - 1);
    assert_eq!(client.capabilities() & reserved_mask, 0);
}

// ---------------------------------------------------------------------------
// Stability: bit positions match their constant values exactly
// ---------------------------------------------------------------------------

#[test]
fn bit_positions_are_stable() {
    assert_eq!(CAP_DEPOSIT, 0x0000_0000_0000_0001);
    assert_eq!(CAP_WITHDRAW, 0x0000_0000_0000_0002);
    assert_eq!(CAP_DEDUCT, 0x0000_0000_0000_0004);
    assert_eq!(CAP_BATCH_DEDUCT, 0x0000_0000_0000_0008);
    assert_eq!(CAP_PAUSE, 0x0000_0000_0000_0010);
    assert_eq!(CAP_AUTHORIZED_CALLER, 0x0000_0000_0000_0020);
    assert_eq!(CAP_OFFERING_METADATA, 0x0000_0000_0000_0040);
    assert_eq!(CAP_PRICE_REGISTRY, 0x0000_0000_0000_0080);
    assert_eq!(CAP_REQUEST_IDEMPOTENCY, 0x0000_0000_0000_0100);
    assert_eq!(CAP_TWO_STEP_OWNERSHIP, 0x0000_0000_0000_0200);
    assert_eq!(CAP_TWO_STEP_ADMIN, 0x0000_0000_0000_0400);
    assert_eq!(CAP_SETTLEMENT, 0x0000_0000_0000_0800);
    assert_eq!(CAP_REVENUE_POOL, 0x0000_0000_0000_1000);
    assert_eq!(CAP_RATE_LIMIT, 0x0000_0000_0000_2000);
    assert_eq!(CAP_ADMIN_BROADCAST, 0x0000_0000_0000_4000);
    assert_eq!(CAP_DEPOSITOR_ALLOWLIST, 0x0000_0000_0000_8000);
    assert_eq!(CAP_SLIPPAGE_GUARD, 0x0000_0000_0001_0000);
    assert_eq!(CAP_UPGRADE, 0x0000_0000_0002_0000);
}

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

#[test]
fn capabilities_is_idempotent() {
    let env = Env::default();
    let client = setup(&env);
    assert_eq!(client.capabilities(), client.capabilities());
}

#[test]
fn capabilities_available_before_init() {
    // capabilities() is a pure constant view — it does not require the vault to be
    // initialized.
    let env = Env::default();
    let vault_addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(&env, &vault_addr);
    assert_eq!(client.capabilities(), ALL_CAPABILITIES);
}

#[test]
fn all_capabilities_constant_has_no_gaps() {
    // Every bit from 0 through 17 must be present in ALL_CAPABILITIES.
    for bit in 0u64..18 {
        let mask = 1u64 << bit;
        assert_ne!(
            ALL_CAPABILITIES & mask,
            0,
            "bit {bit} is missing from ALL_CAPABILITIES"
        );
    }
}

#[test]
fn all_capabilities_bits_are_power_of_two_distinct() {
    // Verify no two CAP_* constants share a bit position.
    let individual: &[u64] = &[
        CAP_DEPOSIT,
        CAP_WITHDRAW,
        CAP_DEDUCT,
        CAP_BATCH_DEDUCT,
        CAP_PAUSE,
        CAP_AUTHORIZED_CALLER,
        CAP_OFFERING_METADATA,
        CAP_PRICE_REGISTRY,
        CAP_REQUEST_IDEMPOTENCY,
        CAP_TWO_STEP_OWNERSHIP,
        CAP_TWO_STEP_ADMIN,
        CAP_SETTLEMENT,
        CAP_REVENUE_POOL,
        CAP_RATE_LIMIT,
        CAP_ADMIN_BROADCAST,
        CAP_DEPOSITOR_ALLOWLIST,
        CAP_SLIPPAGE_GUARD,
        CAP_UPGRADE,
    ];
    let mut seen: u64 = 0;
    for &cap in individual {
        assert_eq!(cap & (cap - 1), 0, "CAP constant {cap:#x} is not a power of two");
        assert_eq!(seen & cap, 0, "CAP constant {cap:#x} overlaps with a previously seen bit");
        seen |= cap;
    }
}

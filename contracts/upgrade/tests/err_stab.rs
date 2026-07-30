//! Frozen snapshot of `UpgradeError` / `ContractError` discriminant codes.
//!
//! These tests guard against accidental renumbering of error codes, which would
//! silently break off-chain integrators that branch on numeric error codes
//! returned by the contract.

extern crate std;

use callora_upgrade::admin::{UpgradeError, DEFAULT_COOLDOWN_SECONDS};
use std::collections::BTreeSet;

/// Frozen snapshot of the error code mapping.
///
/// Every variant is paired with its **stable numeric discriminant**.  If a
/// discriminant changes (accidentally or intentionally), this test fails
/// with a diff-friendly message identifying the regressed variant.
const FROZEN_ERROR_SNAPSHOT: [(u32, UpgradeError); 2] = [
    (1, UpgradeError::CooldownNotElapsed),
    (2, UpgradeError::Overflow),
];

/// Verify every error code in the snapshot maps to the expected discriminant.
#[test]
fn test_error_codes_frozen_against_snapshot() {
    let mut seen = BTreeSet::new();

    for (expected_code, variant) in FROZEN_ERROR_SNAPSHOT {
        let code_as_u32 = variant as u32;
        assert_eq!(
            code_as_u32, expected_code,
            "Error code for variant {:?} does not match frozen snapshot discriminant",
            variant
        );

        assert!(
            seen.insert(expected_code),
            "Duplicate error code detected in snapshot: {expected_code}"
        );
    }

    assert_eq!(
        seen.len(),
        FROZEN_ERROR_SNAPSHOT.len(),
        "Snapshot size mismatch"
    );
}

/// Verify `CooldownNotElapsed` still has discriminant 1.
#[test]
fn test_cooldown_not_elapsed_is_code_1() {
    assert_eq!(UpgradeError::CooldownNotElapsed as u32, 1);
}

/// Verify `Overflow` still has discriminant 2.
#[test]
fn test_overflow_is_code_2() {
    assert_eq!(UpgradeError::Overflow as u32, 2);
}

/// Verify the default cooldown constant has not changed from 24 hours.
///
/// This is a stability snapshot — if the value changes, all deployments
/// using the compiled default must be re-evaluated.
#[test]
fn test_default_cooldown_is_86400() {
    assert_eq!(
        DEFAULT_COOLDOWN_SECONDS, 86_400,
        "DEFAULT_COOLDOWN_SECONDS must remain 86400 (24 h); update callers if intentionally changed"
    );
}

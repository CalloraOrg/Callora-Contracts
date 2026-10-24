//! Frozen snapshot of `AdminError` discriminant codes.
//!
//! These tests guard against accidental renumbering of error codes, which would
//! silently break off-chain integrators that branch on numeric error codes
//! returned by the contract.

extern crate std;

use callora_admin::errors::{AdminError, AdminLimitError};
use std::collections::BTreeSet;

/// Frozen snapshot of the error code mapping.
///
/// Every variant is paired with its **stable numeric discriminant**. If a
/// discriminant changes (accidentally or intentionally), this test fails
/// with a diff-friendly message identifying the regressed variant.
const FROZEN_ERROR_SNAPSHOT: [(u32, AdminError); 14] = [
    (1, AdminError::NotInitialized),
    (2, AdminError::Unauthorized),
    (3, AdminError::InvalidLimit),
    (4, AdminError::BetsAtCap),
    (5, AdminError::PositionsAtCap),
    (6, AdminError::SubscriptionsAtCap),
    (7, AdminError::CounterUnderflow),
    (8, AdminError::Overflow),
    (9, AdminError::AlreadyInitialized),
    (10, AdminError::SameAdmin),
    (11, AdminError::NoPendingAdmin),
    (12, AdminError::UnauthorizedPending),
    (13, AdminError::NominationExpired),
    (14, AdminError::TimelockNotElapsed),
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

/// Verify each individual error variant maintains its stable numeric value.
#[test]
fn test_admin_error_discriminants() {
    assert_eq!(AdminError::NotInitialized as u32, 1);
    assert_eq!(AdminError::Unauthorized as u32, 2);
    assert_eq!(AdminError::InvalidLimit as u32, 3);
    assert_eq!(AdminError::BetsAtCap as u32, 4);
    assert_eq!(AdminError::PositionsAtCap as u32, 5);
    assert_eq!(AdminError::SubscriptionsAtCap as u32, 6);
    assert_eq!(AdminError::CounterUnderflow as u32, 7);
    assert_eq!(AdminError::Overflow as u32, 8);
    assert_eq!(AdminError::AlreadyInitialized as u32, 9);
    assert_eq!(AdminError::SameAdmin as u32, 10);
    assert_eq!(AdminError::NoPendingAdmin as u32, 11);
    assert_eq!(AdminError::UnauthorizedPending as u32, 12);
    assert_eq!(AdminError::NominationExpired as u32, 13);
    assert_eq!(AdminError::TimelockNotElapsed as u32, 14);
}

/// Verify backwards compatibility with `AdminLimitError` alias discriminants.
#[test]
fn test_admin_limit_error_backwards_compatibility() {
    assert_eq!(AdminLimitError::NotInitialized as u32, 1);
    assert_eq!(AdminLimitError::Unauthorized as u32, 2);
    assert_eq!(AdminLimitError::InvalidLimit as u32, 3);
    assert_eq!(AdminLimitError::BetsAtCap as u32, 4);
    assert_eq!(AdminLimitError::PositionsAtCap as u32, 5);
    assert_eq!(AdminLimitError::SubscriptionsAtCap as u32, 6);
    assert_eq!(AdminLimitError::CounterUnderflow as u32, 7);
    assert_eq!(AdminLimitError::Overflow as u32, 8);
}

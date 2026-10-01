extern crate std;

use crate::RevenuePoolError;
use std::collections::BTreeSet;

#[test]
fn revenue_pool_error_codes_are_stable_and_unique() {
    let mappings = [
        (1_u32, RevenuePoolError::BatchEmpty),
        (2, RevenuePoolError::BatchTooLarge),
        (3, RevenuePoolError::NotInitialized),
        (4, RevenuePoolError::AlreadyInitialized),
        (5, RevenuePoolError::Unauthorized),
        (6, RevenuePoolError::Paused),
        (7, RevenuePoolError::AlreadyPaused),
        (8, RevenuePoolError::NotPaused),
        (9, RevenuePoolError::InvalidUsdcToken),
        (10, RevenuePoolError::NoAdminTransferPending),
        (11, RevenuePoolError::NoPauseGuardian),
        (12, RevenuePoolError::AmountNotPositive),
        (13, RevenuePoolError::AmountExceedsMaxDistribute),
        (14, RevenuePoolError::InvalidRecipient),
        (15, RevenuePoolError::InsufficientBalance),
        (16, RevenuePoolError::DuplicateRecipient),
        (17, RevenuePoolError::Overflow),
        (18, RevenuePoolError::MaxDistributeNotPositive),
        (19, RevenuePoolError::MessageEmpty),
        (20, RevenuePoolError::MessageTooLong),
        (21, RevenuePoolError::NoPendingEmergencyDrain),
        (22, RevenuePoolError::TimelockNotExpired),
        (23, RevenuePoolError::EmergencyPaused),
        (24, RevenuePoolError::AlreadyEmergencyPaused),
        (25, RevenuePoolError::NotEmergencyPaused),
    ];

    let mut seen = BTreeSet::new();
    for (expected_code, variant) in mappings {
        assert_eq!(variant as u32, expected_code);
        assert!(
            seen.insert(expected_code),
            "duplicate revenue-pool error code {expected_code}"
        );
    }

    assert_eq!(seen.len(), 25);
}

#[test]
fn error_code_docs_list_every_revenue_pool_code() {
    let docs = include_str!("../../../docs/ERROR_CODES.md");
    let expected_lines = [
        "| 1 | `BatchEmpty` | Revenue Pool | `batch_distribute` received an empty `payments` vector |",
        "| 2 | `BatchTooLarge` | Revenue Pool | `batch_distribute` exceeded `MAX_BATCH_SIZE` |",
        "| 3 | `NotInitialized` | Revenue Pool | A function was called before `init` |",
        "| 4 | `AlreadyInitialized` | Revenue Pool | `init` was called more than once |",
        "| 5 | `Unauthorized` | Revenue Pool | Caller is not authorized for the operation |",
        "| 6 | `Paused` | Revenue Pool | Distribution is blocked while the pool is paused |",
        "| 7 | `AlreadyPaused` | Revenue Pool | `pause` was called while the pool was already paused |",
        "| 8 | `NotPaused` | Revenue Pool | `unpause` was called while the pool was not paused |",
        "| 9 | `InvalidUsdcToken` | Revenue Pool | USDC address conflicts with the pool or admin address |",
        "| 10 | `NoAdminTransferPending` | Revenue Pool | No admin transfer is pending |",
        "| 11 | `NoPauseGuardian` | Revenue Pool | No pause guardian is configured |",
        "| 12 | `AmountNotPositive` | Revenue Pool | Amount must be greater than zero |",
        "| 13 | `AmountExceedsMaxDistribute` | Revenue Pool | Amount exceeds the configured per-leg cap |",
        "| 14 | `InvalidRecipient` | Revenue Pool | Recipient is the revenue pool contract |",
        "| 15 | `InsufficientBalance` | Revenue Pool | Pool USDC balance is below the requested amount |",
        "| 16 | `DuplicateRecipient` | Revenue Pool | A batch contains the same recipient more than once |",
        "| 17 | `Overflow` | Revenue Pool | Checked arithmetic detected an overflow |",
        "| 18 | `MaxDistributeNotPositive` | Revenue Pool | Distribution cap must be greater than zero |",
        "| 19 | `MessageEmpty` | Revenue Pool | Admin broadcast message is empty |",
        "| 20 | `MessageTooLong` | Revenue Pool | Admin broadcast message exceeds the length limit |",
        "| 21 | `NoPendingEmergencyDrain` | Revenue Pool | No emergency drain proposal is pending |",
        "| 22 | `TimelockNotExpired` | Revenue Pool | Emergency drain timelock has not elapsed |",
        "| 23 | `EmergencyPaused` | Revenue Pool | Recovery-only emergency mode is active |",
        "| 24 | `AlreadyEmergencyPaused` | Revenue Pool | Emergency pause was already active |",
        "| 25 | `NotEmergencyPaused` | Revenue Pool | Emergency recovery was requested while inactive |",
    ];

    for line in expected_lines {
        assert!(
            docs.contains(line),
            "missing revenue-pool docs line: {line}"
        );
    }
}

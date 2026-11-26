extern crate std;

use crate::{PriceRegistryError, SettlementError};
use std::collections::BTreeSet;

#[test]
fn settlement_error_codes_are_stable_and_unique() {
    let mappings: [(u32, u32); 51] = [
        (1, SettlementError::NotInitialized as u32),
        (2, SettlementError::AlreadyInitialized as u32),
        (3, SettlementError::Unauthorized as u32),
        (4, SettlementError::AmountNotPositive as u32),
        (5, SettlementError::DeveloperRequired as u32),
        (6, SettlementError::DeveloperMustBeNone as u32),
        (7, SettlementError::PoolOverflow as u32),
        (8, SettlementError::DeveloperOverflow as u32),
        (9, SettlementError::UsdcTokenNotConfigured as u32),
        (10, SettlementError::InsufficientDeveloperBalance as u32),
        (11, SettlementError::DeveloperBalanceUnderflow as u32),
        (12, SettlementError::InsufficientContractBalance as u32),
        (13, SettlementError::DailyWithdrawCapExceeded as u32),
        // code 14 retired (was GasExhaustionRisk)
        // code 15 retired (was ReasonTooLong)
        (16, SettlementError::MigrationSameAddress as u32),
        (17, SettlementError::InvalidMigrationTarget as u32),
        (18, SettlementError::NoDeveloperBalance as u32),
        (19, SettlementError::TimelockOverflow as u32),
        (20, SettlementError::MigrationNotFound as u32),
        (21, SettlementError::TimelockNotExpired as u32),
        (22, SettlementError::MigrationBalanceChanged as u32),
        // code 23 retired (was OverDraft)
        (24, SettlementError::InvalidClaimWindow as u32),
        (25, SettlementError::ClaimWindowClosed as u32),
        (26, SettlementError::MinBalanceViolation as u32),
        (27, SettlementError::ReplayDetected as u32),
        (28, SettlementError::BatchEmpty as u32),
        (29, SettlementError::BatchTooLarge as u32),
        (30, SettlementError::DeveloperFrozen as u32),
        (31, SettlementError::DeveloperNotFrozen as u32),
        (32, SettlementError::FreezeUnauthorized as u32),
        (33, SettlementError::WriteRateLimitExceeded as u32),
        (34, SettlementError::InvalidConfigDistinct as u32),
        (35, SettlementError::InvalidConfigAdminContract as u32),
        (36, SettlementError::InvalidConfigVaultContract as u32),
        (37, SettlementError::InvalidUsdcToken as u32),
        (38, SettlementError::InvalidRecipient as u32),
        (39, SettlementError::NoAdminTransferPending as u32),
        (40, SettlementError::InvalidVault as u32),
        (41, SettlementError::NoVaultRotationPending as u32),
        (42, SettlementError::BroadcastMessageTooLong as u32),
        (43, SettlementError::CrossTenantBatch as u32),
        (44, SettlementError::NoUpgradePending as u32),
        (45, SettlementError::ZeroWasmHash as u32),
        (46, SettlementError::UpgradeTimelockNotExpired as u32),
        (47, SettlementError::UnsupportedToken as u32),
        (48, SettlementError::DuplicateRequestId as u32),
        (49, SettlementError::LengthMismatch as u32),
        (50, SettlementError::InvalidCursor as u32),
        (51, SettlementError::TooManyResults as u32),
        (52, SettlementError::RecipientRegistryNotSet as u32),
        (53, SettlementError::PayoutNameNotFound as u32),
        (54, PriceRegistryError::SystemFrozen as u32),
    ];

    let mut seen = BTreeSet::new();
    for (expected_code, actual_code) in mappings {
        assert_eq!(actual_code, expected_code);
        assert!(
            seen.insert(expected_code),
            "duplicate settlement error code {expected_code}"
        );
    }

    assert_eq!(seen.len(), 50);
}

#[test]
fn error_code_docs_list_every_settlement_code() {
    let docs = include_str!("../../../docs/ERROR_CODES.md");
    let expected_lines = [
        "| 1 | `NotInitialized` | Settlement | A function was called before `init` |",
        "| 2 | `AlreadyInitialized` | Settlement | `init` was called more than once |",
        "| 3 | `Unauthorized` | Settlement | Caller is not the vault or current admin |",
        "| 4 | `AmountNotPositive` | Settlement | Amount must be greater than zero |",
        "| 5 | `DeveloperRequired` | Settlement | `to_pool=false` requires a developer address |",
        "| 6 | `DeveloperMustBeNone` | Settlement | `to_pool=true` forbids a developer address |",
        "| 7 | `PoolOverflow` | Settlement | Global pool credit would overflow `i128` |",
        "| 8 | `DeveloperOverflow` | Settlement | Developer balance credit would overflow `i128` |",
        "| 9 | `UsdcTokenNotConfigured` | Settlement | USDC token address is not configured |",
        "| 10 | `InsufficientDeveloperBalance` | Settlement | Developer balance is lower than the withdrawal |",
        "| 11 | `DeveloperBalanceUnderflow` | Settlement | Developer balance debit would underflow |",
        "| 12 | `InsufficientContractBalance` | Settlement | Contract USDC balance is lower than requested amount |",
        "| 13 | `DailyWithdrawCapExceeded` | Settlement | Daily developer withdrawal cap would be exceeded |",
        "| 14 | _reserved_ (was `GasExhaustionRisk`) | Settlement | Retired: never returned |",
        "| 15 | _reserved_ (was `ReasonTooLong`) | Settlement | Retired: never returned |",
        "| 16 | `MigrationSameAddress` | Settlement | Migration source and target are identical |",
        "| 17 | `InvalidMigrationTarget` | Settlement | Migration target is the settlement contract |",
        "| 18 | `NoDeveloperBalance` | Settlement | Migration source has no positive balance |",
        "| 19 | `TimelockOverflow` | Settlement | Timelock timestamp addition overflowed |",
        "| 20 | `MigrationNotFound` | Settlement | No migration is pending for the source |",
        "| 21 | `TimelockNotExpired` | Settlement | Migration delay has not elapsed |",
        "| 22 | `MigrationBalanceChanged` | Settlement | Approved amount is no longer available |",
        "| 23 | _reserved_ (was `OverDraft`) | Settlement | Retired: never returned |",
        "| 24 | `InvalidClaimWindow` | Settlement | Claim window parameters are invalid |",
        "| 25 | `ClaimWindowClosed` | Settlement | Developer claim window is not currently open |",
        "| 26 | `MinBalanceViolation` | Settlement | Withdrawal would leave balance below the minimum |",
        "| 27 | `ReplayDetected` | Settlement | Settlement request reused or regressed the replay-guard ledger sequence |",
        "| 28 | `BatchEmpty` | Settlement | Batch operation received an empty vector |",
        "| 29 | `BatchTooLarge` | Settlement | Batch operation exceeded the maximum allowed size |",
        "| 30 | `DeveloperFrozen` | Settlement | Developer is frozen and cannot withdraw |",
        "| 31 | `DeveloperNotFrozen` | Settlement | Developer is not frozen; cannot unfreeze |",
        "| 32 | `FreezeUnauthorized` | Settlement | Caller is not authorized to freeze/unfreeze |",
        "| 33 | `WriteRateLimitExceeded` | Settlement | Admin wrote prices too frequently |",
        "| 34 | `InvalidConfigDistinct` | Settlement | Init config requires distinct admin and vault |",
        "| 35 | `InvalidConfigAdminContract` | Settlement | Init config forbids the admin being the contract |",
        "| 36 | `InvalidConfigVaultContract` | Settlement | Init config forbids the vault being the contract |",
        "| 37 | `InvalidUsdcToken` | Settlement | USDC token address is invalid |",
        "| 38 | `InvalidRecipient` | Settlement | Withdrawal recipient or payout name is invalid |",
        "| 39 | `NoAdminTransferPending` | Settlement | No admin transfer is pending |",
        "| 40 | `InvalidVault` | Settlement | Vault address is invalid |",
        "| 41 | `NoVaultRotationPending` | Settlement | No vault rotation is pending |",
        "| 42 | `BroadcastMessageTooLong` | Settlement | Admin broadcast message exceeds the maximum length |",
        "| 43 | `CrossTenantBatch` | Settlement | Batch settlement mixed multiple developers |",
        "| 47 | `UnsupportedToken` | Settlement | Token is not enabled for settlement payments |",
        "| 48 | `DuplicateRequestId` | Settlement | Deduction request ID has already been recorded |",
        "| 49 | `LengthMismatch` | Settlement | Paired batch vectors have different lengths |",
        "| 50 | `InvalidCursor` | Settlement | Batch cursor is invalid or listing cursor is past the end |",
        "| 51 | `TooManyResults` | Settlement | Unpaginated developer-balance view exceeds its configured cap |",
        "| 52 | `RecipientRegistryNotSet` | Settlement | A payout name is set but no recipient registry is configured |",
        "| 53 | `PayoutNameNotFound` | Settlement | The configured payout name no longer resolves in the registry |",
        "| 54 | `SystemFrozen` | Settlement | System is globally frozen via the configured freeze contract |",
    ];

    for line in expected_lines {
        assert!(docs.contains(line), "missing settlement docs line: {line}");
    }
}

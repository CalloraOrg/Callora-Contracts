extern crate std;

use crate::VaultError;
use std::collections::BTreeSet;

#[test]
fn vault_error_codes_are_stable_and_unique() {
    let mappings = [
        (1_u32, VaultError::NotInitialized),
        (2, VaultError::AlreadyInitialized),
        (3, VaultError::Unauthorized),
        (4, VaultError::Paused),
        (5, VaultError::InsufficientBalance),
        (6, VaultError::AmountNotPositive),
        (7, VaultError::ExceedsMaxDeduct),
        (8, VaultError::BelowMinDeposit),
        (9, VaultError::Overflow),
        (10, VaultError::InitialBalanceNegative),
        (11, VaultError::MinDepositNotPositive),
        (12, VaultError::MaxDeductNotPositive),
        (13, VaultError::MinDepositExceedsMaxDeduct),
        (14, VaultError::UsdcTokenCannotBeVault),
        (15, VaultError::RevenuePoolCannotBeVault),
        (16, VaultError::AuthorizedCallerCannotBeVault),
        (17, VaultError::InitialBalanceExceedsOnLedger),
        (18, VaultError::AlreadyPaused),
        (19, VaultError::NotPaused),
        (20, VaultError::SettlementNotSet),
        (21, VaultError::BatchEmpty),
        (22, VaultError::BatchTooLarge),
        (23, VaultError::NewOwnerSameAsCurrent),
        (24, VaultError::NoOwnershipTransferPending),
        (25, VaultError::NoAdminTransferPending),
        (26, VaultError::OfferingIdTooLong),
        (27, VaultError::MetadataTooLong),
        (28, VaultError::PriceParseError),
        (29, VaultError::DuplicateRequestId),
        (30, VaultError::OfferingIdInvalid),
        (31, VaultError::MetadataInvalid),
        (32, VaultError::StaleNonce),
        (33, VaultError::NewRevenuePoolSameAsCurrent),
        (34, VaultError::NoRevenuePoolTransferPending),
    ];

    let mut seen = BTreeSet::new();
    for (expected_code, variant) in mappings {
        assert_eq!(variant as u32, expected_code);
        assert!(seen.insert(expected_code), "duplicate vault error code {expected_code}");
    }

    assert_eq!(seen.len(), 34);
}

#[test]
fn error_code_docs_list_every_vault_code() {
    let docs = include_str!("../../../docs/ERROR_CODES.md");
    let expected_lines = [
        "| 1 | `NotInitialized` | Vault | Vault has not been initialized |",
        "| 2 | `AlreadyInitialized` | Vault | `init` was called more than once |",
        "| 3 | `Unauthorized` | Vault | Caller is not authorized for the operation |",
        "| 4 | `Paused` | Vault | State-changing action is blocked while paused |",
        "| 5 | `InsufficientBalance` | Vault | Vault balance is too low for the requested operation |",
        "| 6 | `AmountNotPositive` | Vault | Amount must be greater than zero |",
        "| 7 | `ExceedsMaxDeduct` | Vault | Deduct amount exceeds the configured cap |",
        "| 8 | `BelowMinDeposit` | Vault | Deposit amount is below the configured minimum |",
        "| 9 | `Overflow` | Vault | Arithmetic overflow was detected |",
        "| 10 | `InitialBalanceNegative` | Vault | Initial balance must be non-negative |",
        "| 11 | `MinDepositNotPositive` | Vault | Minimum deposit must be greater than zero |",
        "| 12 | `MaxDeductNotPositive` | Vault | Maximum deduct must be greater than zero |",
        "| 13 | `MinDepositExceedsMaxDeduct` | Vault | Minimum deposit cannot exceed maximum deduct |",
        "| 14 | `UsdcTokenCannotBeVault` | Vault | USDC token address cannot be the vault contract |",
        "| 15 | `RevenuePoolCannotBeVault` | Vault | Revenue pool address cannot be the vault contract |",
        "| 16 | `AuthorizedCallerCannotBeVault` | Vault | Authorized caller cannot be the vault contract |",
        "| 17 | `InitialBalanceExceedsOnLedger` | Vault | Initial tracked balance exceeds on-ledger USDC |",
        "| 18 | `AlreadyPaused` | Vault | Contract is already paused |",
        "| 19 | `NotPaused` | Vault | Contract is not paused |",
        "| 20 | `SettlementNotSet` | Vault | Settlement address has not been configured |",
        "| 21 | `BatchEmpty` | Vault | Batch deduct received no items |",
        "| 22 | `BatchTooLarge` | Vault | Batch deduct exceeds the maximum allowed size |",
        "| 23 | `NewOwnerSameAsCurrent` | Vault | Proposed owner matches the current owner |",
        "| 24 | `NoOwnershipTransferPending` | Vault | No ownership transfer is pending |",
        "| 25 | `NoAdminTransferPending` | Vault | No admin transfer is pending |",
        "| 26 | `OfferingIdTooLong` | Vault | Offering ID exceeds the maximum length |",
        "| 27 | `MetadataTooLong` | Vault | Metadata exceeds the maximum length |",
        "| 28 | `PriceParseError` | Vault | Price is invalid or non-positive |",
        "| 29 | `DuplicateRequestId` | Vault | Request ID has already been processed |",
        "| 30 | `OfferingIdInvalid` | Vault | Offering ID is empty or contains invalid characters |",
        "| 31 | `MetadataInvalid` | Vault | Metadata is empty or contains invalid characters |",
        "| 32 | `StaleNonce` | Vault | Rotation nonce does not match the stored current nonce |",
        "| 33 | `NewRevenuePoolSameAsCurrent` | Vault | Proposed revenue pool matches the current revenue pool |",
        "| 34 | `NoRevenuePoolTransferPending` | Vault | No revenue-pool transfer is pending |",
    ];

    for line in expected_lines {
        assert!(docs.contains(line), "missing vault docs line: {line}");
    }
}

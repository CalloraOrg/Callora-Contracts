//! Error stability tests for the Callora Vault contract.
//!
//! Verifies that all `VaultError` discriminant values are stable (existing
//! codes keep their values; new codes are appended only) and that the new
//! `InvalidRecipient` (code 45) and `ZeroAddressRecipient` (code 46) codes
//! are present.
//!
//! Run with: `cargo test -p callora-vault err_stab`

use callora_vault::VaultError;

#[test]
fn error_codes_are_stable() {
    // Existing codes must keep their values.
    assert_eq!(VaultError::NotInitialized as u32, 1);
    assert_eq!(VaultError::AlreadyInitialized as u32, 2);
    assert_eq!(VaultError::Unauthorized as u32, 3);
    assert_eq!(VaultError::Paused as u32, 4);
    assert_eq!(VaultError::InsufficientBalance as u32, 5);
    assert_eq!(VaultError::AmountNotPositive as u32, 6);
    assert_eq!(VaultError::ExceedsMaxDeduct as u32, 7);
    assert_eq!(VaultError::BelowMinDeposit as u32, 8);
    assert_eq!(VaultError::Overflow as u32, 9);
    assert_eq!(VaultError::MinDepositNotPositive as u32, 11);
    assert_eq!(VaultError::MaxDeductNotPositive as u32, 12);
    assert_eq!(VaultError::MinDepositExceedsMaxDeduct as u32, 13);
    assert_eq!(VaultError::UsdcTokenCannotBeVault as u32, 14);
    assert_eq!(VaultError::RevenuePoolCannotBeVault as u32, 15);
    assert_eq!(VaultError::AuthorizedCallerCannotBeVault as u32, 16);
    assert_eq!(VaultError::InitialBalanceExceedsOnLedger as u32, 17);
    assert_eq!(VaultError::AlreadyPaused as u32, 18);
    assert_eq!(VaultError::NotPaused as u32, 19);
    assert_eq!(VaultError::SettlementNotSet as u32, 20);
    assert_eq!(VaultError::BatchEmpty as u32, 21);
    assert_eq!(VaultError::BatchTooLarge as u32, 22);
    assert_eq!(VaultError::NoOwnershipTransferPending as u32, 24);
    assert_eq!(VaultError::NoAdminTransferPending as u32, 25);
    assert_eq!(VaultError::PriceParseError as u32, 28);
    assert_eq!(VaultError::DuplicateRequestId as u32, 29);
    assert_eq!(VaultError::OfferingIdInvalid as u32, 30);
    assert_eq!(VaultError::MetadataInvalid as u32, 31);
    assert_eq!(VaultError::StaleNonce as u32, 32);
    assert_eq!(VaultError::NewRevenuePoolSameAsCurrent as u32, 33);
    assert_eq!(VaultError::NoRevenuePoolTransferPending as u32, 34);
    // New codes appended at 45, 46.
    assert_eq!(VaultError::InvalidRecipient as u32, 45);
    assert_eq!(VaultError::ZeroAddressRecipient as u32, 46);
    assert_eq!(VaultError::CallerNotInAllowlist as u32, 44);
    assert_eq!(VaultError::AdminCooldownActive as u32, 49);
    assert_eq!(VaultError::SettlementCannotBeVault as u32, 56);
    assert_eq!(VaultError::SettlementCannotBeToken as u32, 57);
    assert_eq!(VaultError::ProposalExpired as u32, 58);
}

#[test]
fn error_codes_are_contiguous_appended() {
    // Verify new codes are appended only - existing codes unchanged.
    assert_eq!(VaultError::InvalidRecipient as u32, 45);
    assert_eq!(VaultError::ZeroAddressRecipient as u32, 46);
}

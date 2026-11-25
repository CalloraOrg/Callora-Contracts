#![cfg(test)]

use callora_checkpoint::errors::CheckpointError;

#[test]
fn test_checkpoint_error_stability() {
    // Freeze client-facing error code numbers for checkpoint.
    assert_eq!(CheckpointError::NotInitialized as u32, 1);
    assert_eq!(CheckpointError::AlreadyInitialized as u32, 2);
    assert_eq!(CheckpointError::Unauthorized as u32, 3);
    assert_eq!(CheckpointError::BatchEmpty as u32, 4);
    assert_eq!(CheckpointError::BatchTooLarge as u32, 5);
    assert_eq!(CheckpointError::CheckpointNotFound as u32, 6);
    assert_eq!(CheckpointError::AmountNegative as u32, 7);
    assert_eq!(CheckpointError::InvalidPageSize as u32, 8);
    assert_eq!(CheckpointError::Overflow as u32, 9);
    assert_eq!(CheckpointError::NoAdminTransferPending as u32, 10);
    assert_eq!(CheckpointError::MigrationPending as u32, 11);
    assert_eq!(CheckpointError::VersionSkip as u32, 12);
    assert_eq!(CheckpointError::SchemaMismatch as u32, 13);
    assert_eq!(CheckpointError::SilentLayoutChange as u32, 14);
    assert_eq!(CheckpointError::WasmHashZero as u32, 15);
    assert_eq!(CheckpointError::RollbackNotAllowed as u32, 16);
    assert_eq!(CheckpointError::BackupMissing as u32, 17);
    assert_eq!(CheckpointError::UnauthorizedUpgradeState as u32, 18);
    assert_eq!(CheckpointError::StorageUnreadable as u32, 19);
    assert_eq!(CheckpointError::CheckpointPruned as u32, 20);
    assert_eq!(CheckpointError::PruneOutOfRange as u32, 21);
    assert_eq!(CheckpointError::InvalidMaxRemove as u32, 22);
}

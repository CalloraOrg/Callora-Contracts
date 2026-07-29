#![cfg(test)]

use callora_freeze::errors::FreezeError;

#[test]
fn test_freeze_error_stability() {
    // Freeze client-facing error code numbers for freeze.
    assert_eq!(FreezeError::NotInitialized as u32, 1);
    assert_eq!(FreezeError::AlreadyInitialized as u32, 2);
    assert_eq!(FreezeError::Unauthorized as u32, 3);
    assert_eq!(FreezeError::AlreadyFrozen as u32, 4);
    assert_eq!(FreezeError::NotFrozen as u32, 5);
    assert_eq!(FreezeError::Overflow as u32, 6);
}

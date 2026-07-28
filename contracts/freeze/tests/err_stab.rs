#![cfg(test)]

use callora_freeze::errors::ContractError;

#[test]
fn test_freeze_error_stability() {
    // Freeze client-facing error code numbers for freeze.
    assert_eq!(ContractError::NotInitialized as u32, 1);
    assert_eq!(ContractError::AlreadyInitialized as u32, 2);
    assert_eq!(ContractError::Unauthorized as u32, 3);
    assert_eq!(ContractError::AlreadyFrozen as u32, 4);
    assert_eq!(ContractError::NotFrozen as u32, 5);
    assert_eq!(ContractError::InvalidState as u32, 6);
    assert_eq!(ContractError::Overflow as u32, 7);
}

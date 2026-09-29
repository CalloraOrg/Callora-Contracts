#![cfg(test)]

use callora_fee::errors::ContractError;

#[test]
fn test_fee_error_stability() {
    assert_eq!(ContractError::NotInitialized as u32, 1);
    assert_eq!(ContractError::AlreadyInitialized as u32, 2);
    assert_eq!(ContractError::Unauthorized as u32, 3);
    assert_eq!(ContractError::InvalidAmount as u32, 4);
    assert_eq!(ContractError::Overflow as u32, 5);
    assert_eq!(ContractError::FeeTooHigh as u32, 6);
    assert_eq!(ContractError::InsufficientBalance as u32, 7);
    assert_eq!(ContractError::InvalidRecipient as u32, 8);
}

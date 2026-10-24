#![cfg(test)]

use callora_fee::errors::ContractError;

/// Asserts that every error discriminant is pinned to its declared u32 value.
/// These values are part of the public ABI — never renumber them.
/// New variants are appended only, so existing codes remain stable.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
enum ExpectedError {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    Unauthorized = 3,
    InvalidAmount = 4,
    Overflow = 5,
    FeeTooHigh = 6,
    InsufficientBalance = 7,
    InvalidRecipient = 8,
}

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

#[test]
fn test_fee_error_codes_are_unique() {
    let codes = [
        ContractError::NotInitialized as u32,
        ContractError::AlreadyInitialized as u32,
        ContractError::Unauthorized as u32,
        ContractError::InvalidAmount as u32,
        ContractError::Overflow as u32,
        ContractError::FeeTooHigh as u32,
        ContractError::InsufficientBalance as u32,
        ContractError::InvalidRecipient as u32,
    ];
    for i in 0..codes.len() {
        for j in (i + 1)..codes.len() {
            assert_ne!(codes[i], codes[j], "duplicate error code");
        }
    }
}

#[test]
fn test_error_discriminants_match_expected() {
    assert_eq!(
        ContractError::NotInitialized as u32,
        ExpectedError::NotInitialized as u32
    );
    assert_eq!(
        ContractError::AlreadyInitialized as u32,
        ExpectedError::AlreadyInitialized as u32
    );
    assert_eq!(
        ContractError::Unauthorized as u32,
        ExpectedError::Unauthorized as u32
    );
    assert_eq!(
        ContractError::InvalidAmount as u32,
        ExpectedError::InvalidAmount as u32
    );
    assert_eq!(
        ContractError::Overflow as u32,
        ExpectedError::Overflow as u32
    );
    assert_eq!(
        ContractError::FeeTooHigh as u32,
        ExpectedError::FeeTooHigh as u32
    );
    assert_eq!(
        ContractError::InsufficientBalance as u32,
        ExpectedError::InsufficientBalance as u32
    );
    assert_eq!(
        ContractError::InvalidRecipient as u32,
        ExpectedError::InvalidRecipient as u32
    );
}

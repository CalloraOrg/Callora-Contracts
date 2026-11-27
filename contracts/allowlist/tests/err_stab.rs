//! Stable error-code snapshot for the Callora Allowlist contract.
//!
//! Numeric discriminants are part of the client-facing interface. New variants
//! must be appended with fresh codes; existing codes must never be renumbered.

#![cfg(test)]
extern crate std;

use callora_allowlist::AllowlistError;

#[test]
fn snapshot_error_codes() {
    assert_eq!(AllowlistError::NotInitialized as u32, 1);
    assert_eq!(AllowlistError::AlreadyInitialized as u32, 2);
    assert_eq!(AllowlistError::Unauthorized as u32, 3);
    assert_eq!(AllowlistError::AlreadyPresent as u32, 4);
    assert_eq!(AllowlistError::NotPresent as u32, 5);
    assert_eq!(AllowlistError::InvalidPage as u32, 6);
    assert_eq!(AllowlistError::Overflow as u32, 7);
    assert_eq!(AllowlistError::NoPendingAdmin as u32, 8);
}

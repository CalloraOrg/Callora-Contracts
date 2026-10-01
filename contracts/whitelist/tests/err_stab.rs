#![cfg(test)]

extern crate std;

use callora_whitelist::WhitelistError;

/// Stable error-code snapshot for the Callora Whitelist contract.
///
/// The numeric discriminants in [`WhitelistError`] are **frozen** as part of
/// the client-facing contract interface. Downstream clients and off-chain
/// indexers rely on these `u32` codes remaining stable across contract
/// upgrades.
///
/// If a new error variant is added, it MUST be appended after the existing
/// codes without re-numbering or removing any existing variant.
///
/// | Code | Variant                     | Meaning                                              |
/// |------|-----------------------------|------------------------------------------------------|
/// | 1    | NotInitialized              | Contract has not been initialized                    |
/// | 2    | AlreadyInitialized          | `init` was called more than once                     |
/// | 3    | Unauthorized                | Caller is not authorized for the operation           |
/// | 4    | AddressAlreadyInWhitelist   | Address is already in the whitelist                  |
/// | 5    | AddressNotInWhitelist       | Address is not in the whitelist                      |
/// | 49   | AdminCooldownActive         | Admin cool-off window is still active                |
/// | 50   | InvalidAdminCooldown        | Admin cool-off window is outside accepted bounds     |
/// | 51   | NoAdminTransferPending      | No admin transfer is pending                         |
/// | 52   | NewAdminSameAsCurrent       | Proposed admin matches the current admin             |
#[test]
fn snapshot_error_codes() {
    assert_eq!(WhitelistError::NotInitialized as u32, 1);
    assert_eq!(WhitelistError::AlreadyInitialized as u32, 2);
    assert_eq!(WhitelistError::Unauthorized as u32, 3);
    assert_eq!(WhitelistError::AddressAlreadyInWhitelist as u32, 4);
    assert_eq!(WhitelistError::AddressNotInWhitelist as u32, 5);

    assert_eq!(WhitelistError::AdminCooldownActive as u32, 49);
    assert_eq!(WhitelistError::InvalidAdminCooldown as u32, 50);
    assert_eq!(WhitelistError::NoAdminTransferPending as u32, 51);
    assert_eq!(WhitelistError::NewAdminSameAsCurrent as u32, 52);
}

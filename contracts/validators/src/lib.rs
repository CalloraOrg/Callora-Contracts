#![no_std]

//! Reusable, semantic input validators for the Callora contracts.
//!
//! This crate exists to replace generic panics and opaque `Result<_, ()>`
//! error values in validation code with a stable, machine-readable
//! [`ValidatorError`] enum. Callers can branch on the numeric error codes
//! instead of parsing panic strings, and every rejection carries a specific
//! reason (empty input, out-of-range amount, arithmetic overflow, and so on).
//!
//! # Core invariant
//! For every input string `s`:
//! - `is_visible_ascii_metadata(s) == normalize_visible_ascii(s).is_ok()`
//! - Acceptance iff `s` is non-empty, length ≤ [`MAX_VALIDATED_STRING_LEN`],
//!   every byte is in `0x20..=0x7E`, and neither the first nor last byte is
//!   ASCII space (`0x20`).

use soroban_sdk::String;

/// Error type for validation failures in [`normalize_visible_ascii`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationError {
    /// String is empty.
    Empty,
    /// String exceeds maximum length.
    TooLong,
    /// String contains non-visible ASCII characters.
    InvalidCharacter,
    /// String has leading or trailing spaces.
    InvalidSpacing,
}

/// Maximum byte length accepted by [`normalize_visible_ascii`].
pub const MAX_VALIDATED_STRING_LEN: u32 = 256;

/// Normalize a bounded metadata string to its canonical on-chain form.
///
/// Accepted strings are non-empty visible ASCII with no leading or trailing
/// spaces. This rejects C0/DEL controls, zero-width and bidi controls, and
/// Unicode confusables by construction. Because ASCII has no decomposed forms,
/// the returned value is NFC-normalized and byte-stable.
///
/// # Errors
/// Returns `Err(ValidationError)` when the string is empty, exceeds
/// [`MAX_VALIDATED_STRING_LEN`], contains a non-visible-ASCII byte, or has
/// leading/trailing ASCII space.
pub fn normalize_visible_ascii(
    s: &String,
) -> Result<[u8; MAX_VALIDATED_STRING_LEN as usize], ValidationError> {
    let len = s.len();
    if len == 0 {
        return Err(ValidationError::Empty);
    }
    if len > MAX_VALIDATED_STRING_LEN {
        return Err(ValidationError::TooLong);
    }

    let mut buf = [0u8; MAX_VALIDATED_STRING_LEN as usize];
    s.copy_into_slice(&mut buf[..len as usize]);
    let bytes = &buf[..len as usize];

    if bytes[0] == b' ' || bytes[len as usize - 1] == b' ' {
        return Err(ValidationError::InvalidSpacing);
    }

    for &b in bytes {
        if !(0x20..=0x7e).contains(&b) {
            return Err(ValidationError::InvalidCharacter);
        }
    }

    Ok(buf)
}

/// Return whether a bounded metadata string is accepted by the on-chain policy.
pub fn is_visible_ascii_metadata(s: &String) -> bool {
    normalize_visible_ascii(s).is_ok()
}

/// Pure reference predicate mirroring [`normalize_visible_ascii`] over raw bytes.
///
/// Used by property tests to check the on-chain validator against an independent
/// oracle that does not touch the Soroban host.
pub fn bytes_are_visible_ascii(bytes: &[u8]) -> bool {
    let len = bytes.len();
    if len == 0 || len > MAX_VALIDATED_STRING_LEN as usize {
        return false;
    }
    if bytes[0] == b' ' || bytes[len - 1] == b' ' {
        return false;
    }
    bytes.iter().all(|b| (0x20..=0x7e).contains(b))
}
//! All validators are pure and stateless: they neither read nor write contract
//! storage and expose no state-changing entrypoints. There is therefore nothing
//! to authorize (`require_auth` is not applicable here), and all arithmetic uses
//! overflow-safe checked operations.

pub mod errors;
pub mod validators;

pub use errors::ValidatorError;
pub use validators::{
    checked_add_amount, is_visible_ascii_metadata, normalize_visible_ascii, require_in_range,
    require_non_negative_amount, require_positive_amount, MAX_VALIDATED_STRING_LEN,
};

#[cfg(test)]
mod test_errors;
#[cfg(test)]
mod test_validators;

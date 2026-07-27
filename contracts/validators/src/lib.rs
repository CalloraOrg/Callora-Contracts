#![no_std]

//! Reusable, semantic input validators for the Callora contracts.
//!
//! This crate exists to replace generic panics and opaque `Result<_, ()>`
//! error values in validation code with a stable, machine-readable
//! [`ValidatorError`] enum. Callers can branch on the numeric error codes
//! instead of parsing panic strings, and every rejection carries a specific
//! reason (empty input, out-of-range amount, arithmetic overflow, and so on).
//!
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

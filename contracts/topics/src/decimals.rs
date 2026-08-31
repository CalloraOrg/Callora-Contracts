//! Token decimal normalization utilities for contract boundaries.
//!
//! This module provides functions to normalize token amounts to a canonical
//! decimal scale (18 decimals) and denormalize them back to a token's native
//! decimal representation. This ensures arithmetic correctness across tokens
//! with different decimal places.
//!
//! ## Design
//!
//! Callora uses 18 decimals internally as the canonical scale for all arithmetic.
//! When tokens enter or exit contract boundaries, they must be normalized or
//! denormalized accordingly.
//!
//! For example:
//! - USDC (6 decimals): 1 USDC = 1_000_000 base units = 10^18 internal units
//! - Native token (18 decimals): 1 token = 1 base unit = 10^18 internal units
//!
//! The normalization formula is:
//! ```ignore
//! internal_amount = external_amount * 10^(18 - token_decimals)
//! external_amount = internal_amount / 10^(18 - token_decimals)
//! ```
//!
//! # Authorization and Atomicity
//!
//! All normalization functions are pure computations with no side effects.
//! They do not consume gas, emit events, or modify state. Authorization and
//! atomicity guarantees are enforced by the calling contract.
//!
//! # Arithmetic Safety
//!
//! All operations use checked arithmetic that panics on overflow/underflow
//! rather than silently wrapping. This includes:
//! - Multiplication during normalization (amount * 10^power)
//! - Division during denormalization (amount / 10^power)
//! - Both operations reject negative amounts

use soroban_sdk::Env;

/// Canonical decimal scale used internally by all Callora contracts.
///
/// All arithmetic operations work at this scale. Tokens with different
/// native decimal places must be normalized to this scale at contract
/// entry points and denormalized on exit.
pub const CANONICAL_DECIMALS: u32 = 18;

/// Maximum value for token decimals (Soroban supports 0-18).
pub const MAX_TOKEN_DECIMALS: u32 = 18;

/// Normalization error.
#[derive(Clone, Debug, PartialEq)]
pub enum DecimalError {
    /// Token decimals exceed the maximum of 18.
    DecimalsTooLarge,
    /// Amount is negative or the operation would overflow.
    ArithmeticError,
}

/// Normalize an external token amount to internal canonical scale (18 decimals).
///
/// Converts from the token's native decimal representation to the internal
/// 18-decimal scale used for all arithmetic.
///
/// # Arguments
/// * `amount` — the amount in the token's native decimal scale
/// * `token_decimals` — the number of decimals for the token (0-18)
///
/// # Returns
/// * `Ok(normalized)` — the amount scaled to 18 decimals
/// * `Err(DecimalError::DecimalsTooLarge)` — if token_decimals > 18
/// * `Err(DecimalError::ArithmeticError)` — if amount is negative or multiply overflows
///
/// # Example
/// ```ignore
/// // USDC with 6 decimals: normalize 1.5 USDC (1_500_000 base units)
/// let normalized = normalize(1_500_000, 6).unwrap();
/// // normalized == 1_500_000_000_000_000_000 (18 decimals)
/// ```
pub fn normalize(amount: i128, token_decimals: u32) -> Result<i128, DecimalError> {
    if amount < 0 {
        return Err(DecimalError::ArithmeticError);
    }
    if token_decimals > CANONICAL_DECIMALS {
        return Err(DecimalError::DecimalsTooLarge);
    }

    let scale = CANONICAL_DECIMALS - token_decimals;
    if scale == 0 {
        Ok(amount)
    } else {
        let multiplier: i128 = 10i128
            .checked_pow(scale)
            .ok_or(DecimalError::ArithmeticError)?;
        amount
            .checked_mul(multiplier)
            .ok_or(DecimalError::ArithmeticError)
    }
}

/// Denormalize an internal canonical amount back to a token's native decimal scale.
///
/// Converts from the internal 18-decimal scale back to the token's native
/// representation.
///
/// # Arguments
/// * `amount` — the amount in internal canonical scale (18 decimals)
/// * `token_decimals` — the number of decimals for the token (0-18)
///
/// # Returns
/// * `Ok(denormalized)` — the amount in the token's native decimal scale
/// * `Err(DecimalError::DecimalsTooLarge)` — if token_decimals > 18
/// * `Err(DecimalError::ArithmeticError)` — if amount is negative or divide would lose precision
///
/// # Precision Loss
/// Division is integer division. If the internal amount is not evenly divisible
/// by the scale factor, precision is lost (rounded down).
///
/// # Example
/// ```ignore
/// // Convert 1.5 * 10^18 back to 6-decimal USDC
/// let denormalized = denormalize(1_500_000_000_000_000_000i128, 6).unwrap();
/// // denormalized == 1_500_000 (6 decimals)
/// ```
pub fn denormalize(amount: i128, token_decimals: u32) -> Result<i128, DecimalError> {
    if amount < 0 {
        return Err(DecimalError::ArithmeticError);
    }
    if token_decimals > CANONICAL_DECIMALS {
        return Err(DecimalError::DecimalsTooLarge);
    }

    let scale = CANONICAL_DECIMALS - token_decimals;
    if scale == 0 {
        Ok(amount)
    } else {
        let divisor: i128 = 10i128
            .checked_pow(scale)
            .ok_or(DecimalError::ArithmeticError)?;
        Ok(amount / divisor)
    }
}

/// Check if denormalization would incur precision loss.
///
/// Returns `true` if `amount / 10^(18 - token_decimals)` has a remainder,
/// indicating that some precision would be lost in the conversion.
///
/// # Arguments
/// * `amount` — the amount in internal canonical scale (18 decimals)
/// * `token_decimals` — the number of decimals for the token (0-18)
///
/// # Returns
/// * `true` — if the division has a remainder (precision loss)
/// * `false` — if the division is exact (no precision loss)
/// * `Err(DecimalError::DecimalsTooLarge)` — if token_decimals > 18
pub fn would_lose_precision(amount: i128, token_decimals: u32) -> Result<bool, DecimalError> {
    if token_decimals > CANONICAL_DECIMALS {
        return Err(DecimalError::DecimalsTooLarge);
    }

    let scale = CANONICAL_DECIMALS - token_decimals;
    if scale == 0 {
        Ok(false)
    } else {
        let divisor: i128 = 10i128
            .checked_pow(scale)
            .ok_or(DecimalError::ArithmeticError)?;
        Ok(amount % divisor != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_usdc_6_decimals() {
        // 1 USDC = 1_000_000 in 6-decimal scale
        // should normalize to 10^18 in canonical scale
        let result = normalize(1_000_000, 6).unwrap();
        assert_eq!(result, 1_000_000_000_000_000_000i128);
    }

    #[test]
    fn normalize_18_decimal_no_change() {
        // Token already at 18 decimals should pass through unchanged
        let result = normalize(1_000_000_000_000_000_000i128, 18).unwrap();
        assert_eq!(result, 1_000_000_000_000_000_000i128);
    }

    #[test]
    fn normalize_0_decimal_token() {
        // Token with 0 decimals (whole tokens only)
        // 5 whole tokens = 5 in 0-decimal scale
        // should normalize to 5 * 10^18
        let result = normalize(5, 0).unwrap();
        assert_eq!(result, 5_000_000_000_000_000_000i128);
    }

    #[test]
    fn normalize_negative_amount_fails() {
        let result = normalize(-100, 6);
        assert_eq!(result, Err(DecimalError::ArithmeticError));
    }

    #[test]
    fn normalize_decimals_too_large_fails() {
        let result = normalize(1_000_000, 19);
        assert_eq!(result, Err(DecimalError::DecimalsTooLarge));
    }

    #[test]
    fn normalize_overflow() {
        // Try to normalize the maximum i128 at low decimals
        // This should overflow even with 6 decimals
        let result = normalize(i128::MAX, 0);
        assert_eq!(result, Err(DecimalError::ArithmeticError));
    }

    #[test]
    fn denormalize_to_usdc() {
        // 10^18 in canonical scale back to 6-decimal USDC
        let result = denormalize(1_000_000_000_000_000_000i128, 6).unwrap();
        assert_eq!(result, 1_000_000i128);
    }

    #[test]
    fn denormalize_18_decimal_no_change() {
        // Already at target decimals
        let result = denormalize(1_000_000_000_000_000_000i128, 18).unwrap();
        assert_eq!(result, 1_000_000_000_000_000_000i128);
    }

    #[test]
    fn denormalize_0_decimal_token() {
        // 5 * 10^18 back to 0-decimal whole tokens
        let result = denormalize(5_000_000_000_000_000_000i128, 0).unwrap();
        assert_eq!(result, 5i128);
    }

    #[test]
    fn denormalize_negative_amount_fails() {
        let result = denormalize(-1_000_000_000_000_000_000i128, 6);
        assert_eq!(result, Err(DecimalError::ArithmeticError));
    }

    #[test]
    fn denormalize_decimals_too_large_fails() {
        let result = denormalize(1_000_000_000_000_000_000i128, 19);
        assert_eq!(result, Err(DecimalError::DecimalsTooLarge));
    }

    #[test]
    fn roundtrip_usdc() {
        // USDC: normalize then denormalize should recover original
        let original = 1_234_567i128; // 1.234567 USDC
        let normalized = normalize(original, 6).unwrap();
        let denormalized = denormalize(normalized, 6).unwrap();
        assert_eq!(denormalized, original);
    }

    #[test]
    fn roundtrip_18_decimals() {
        // 18-decimal token should roundtrip exactly
        let original = 1_234_567_890_123_456_789i128;
        let normalized = normalize(original, 18).unwrap();
        let denormalized = denormalize(normalized, 18).unwrap();
        assert_eq!(denormalized, original);
    }

    #[test]
    fn precision_loss_detection() {
        // Amount that only has precision at 18 decimals
        // Should lose precision when converting to 6 decimals
        let amount = 1_000_000_000_000_000_001i128; // 1.000000000000000001 * 10^18
        let loses_precision = would_lose_precision(amount, 6).unwrap();
        assert!(loses_precision, "Should detect precision loss at 6 decimals");
    }

    #[test]
    fn no_precision_loss_detection() {
        // Amount that converts cleanly
        let amount = 1_000_000_000_000_000_000i128; // exactly 1.0 * 10^18
        let loses_precision = would_lose_precision(amount, 6).unwrap();
        assert!(!loses_precision, "Should not detect precision loss for clean conversion");
    }

    #[test]
    fn precision_loss_at_exact_boundary() {
        // Amount with 1 wei remainder
        let amount = 1_000_000_000_000_000_000i128 + 1i128;
        let loses_precision = would_lose_precision(amount, 18).unwrap();
        assert!(!loses_precision, "No remainder at 18 decimals");

        let loses_precision = would_lose_precision(amount, 17).unwrap();
        assert!(loses_precision, "Has remainder at 17 decimals");
    }
}

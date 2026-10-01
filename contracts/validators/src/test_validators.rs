extern crate std;

use crate::validators::{
    checked_add_amount, is_visible_ascii_metadata, normalize_visible_ascii, require_in_range,
    require_non_negative_amount, require_positive_amount, MAX_VALIDATED_STRING_LEN,
};
use crate::{
    capabilities, CAP_AMOUNT_VALIDATION, CAP_CHECKED_ARITHMETIC, CAP_RANGE_VALIDATION,
    CAP_STRING_VALIDATION, ValidatorError,
};
use soroban_sdk::{Env, String};

// --- normalize_visible_ascii -------------------------------------------------

#[test]
fn accepts_visible_ascii() {
    let env = Env::default();
    let s = String::from_str(&env, "offering-42");
    let buf = normalize_visible_ascii(&s).unwrap();
    assert_eq!(&buf[..11], b"offering-42");
    assert!(is_visible_ascii_metadata(&s));
}

#[test]
fn rejects_empty() {
    let env = Env::default();
    let s = String::from_str(&env, "");
    assert_eq!(normalize_visible_ascii(&s), Err(ValidatorError::Empty));
    assert!(!is_visible_ascii_metadata(&s));
}

#[test]
fn accepts_max_length_boundary() {
    let env = Env::default();
    let text: std::string::String = "a".repeat(MAX_VALIDATED_STRING_LEN as usize);
    let s = String::from_str(&env, text.as_str());
    assert!(normalize_visible_ascii(&s).is_ok());
}

#[test]
fn rejects_too_long() {
    let env = Env::default();
    let text: std::string::String = "a".repeat(MAX_VALIDATED_STRING_LEN as usize + 1);
    let s = String::from_str(&env, text.as_str());
    assert_eq!(normalize_visible_ascii(&s), Err(ValidatorError::TooLong));
}

#[test]
fn rejects_leading_whitespace() {
    let env = Env::default();
    let s = String::from_str(&env, " leading");
    assert_eq!(
        normalize_visible_ascii(&s),
        Err(ValidatorError::LeadingWhitespace)
    );
}

#[test]
fn rejects_trailing_whitespace() {
    let env = Env::default();
    let s = String::from_str(&env, "trailing ");
    assert_eq!(
        normalize_visible_ascii(&s),
        Err(ValidatorError::TrailingWhitespace)
    );
}

#[test]
fn rejects_control_character() {
    let env = Env::default();
    // Embedded tab (0x09) is a C0 control below the visible range.
    let s = String::from_str(&env, "bad\tvalue");
    assert_eq!(
        normalize_visible_ascii(&s),
        Err(ValidatorError::NonVisibleAscii)
    );
}

#[test]
fn rejects_non_ascii() {
    let env = Env::default();
    // "café" contains a multi-byte non-ASCII character.
    let s = String::from_str(&env, "café");
    assert_eq!(
        normalize_visible_ascii(&s),
        Err(ValidatorError::NonVisibleAscii)
    );
    assert!(!is_visible_ascii_metadata(&s));
}

#[test]
fn single_visible_char_is_accepted() {
    let env = Env::default();
    let s = String::from_str(&env, "x");
    assert!(normalize_visible_ascii(&s).is_ok());
}

// --- require_positive_amount -------------------------------------------------

#[test]
fn positive_amount_ok() {
    assert_eq!(require_positive_amount(1), Ok(1));
    assert_eq!(require_positive_amount(i128::MAX), Ok(i128::MAX));
}

#[test]
fn positive_amount_rejects_zero_and_negative() {
    assert_eq!(
        require_positive_amount(0),
        Err(ValidatorError::AmountNotPositive)
    );
    assert_eq!(
        require_positive_amount(-1),
        Err(ValidatorError::AmountNotPositive)
    );
}

// --- require_non_negative_amount ---------------------------------------------

#[test]
fn non_negative_amount_ok() {
    assert_eq!(require_non_negative_amount(0), Ok(0));
    assert_eq!(require_non_negative_amount(5), Ok(5));
}

#[test]
fn non_negative_amount_rejects_negative() {
    assert_eq!(
        require_non_negative_amount(-1),
        Err(ValidatorError::AmountNegative)
    );
}

// --- checked_add_amount ------------------------------------------------------

#[test]
fn checked_add_ok() {
    assert_eq!(checked_add_amount(2, 3), Ok(5));
    assert_eq!(checked_add_amount(-4, 4), Ok(0));
}

#[test]
fn checked_add_detects_overflow() {
    assert_eq!(
        checked_add_amount(i128::MAX, 1),
        Err(ValidatorError::Overflow)
    );
    assert_eq!(
        checked_add_amount(i128::MIN, -1),
        Err(ValidatorError::Overflow)
    );
}

// --- require_in_range --------------------------------------------------------

#[test]
fn in_range_ok() {
    assert_eq!(require_in_range(5, 1, 10), Ok(5));
    assert_eq!(require_in_range(1, 1, 10), Ok(1)); // lower boundary
    assert_eq!(require_in_range(10, 1, 10), Ok(10)); // upper boundary
}

#[test]
fn out_of_range_rejected() {
    assert_eq!(require_in_range(0, 1, 10), Err(ValidatorError::OutOfRange));
    assert_eq!(require_in_range(11, 1, 10), Err(ValidatorError::OutOfRange));
}

#[test]
fn empty_range_rejects_everything() {
    // min > max is an empty range.
    assert_eq!(require_in_range(5, 10, 1), Err(ValidatorError::OutOfRange));
}

// --- capability bitmap -------------------------------------------------------

#[test]
fn capabilities_exposes_supported_validator_features() {
    let env = Env::default();
    let caps = capabilities(&env);

    assert_ne!(caps, 0);
    assert_eq!(
        caps,
        CAP_STRING_VALIDATION
            | CAP_AMOUNT_VALIDATION
            | CAP_CHECKED_ARITHMETIC
            | CAP_RANGE_VALIDATION
    );
    assert_eq!(caps & !((1u64 << 4) - 1), 0);
}

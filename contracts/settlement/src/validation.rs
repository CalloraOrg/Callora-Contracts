//! String validation helpers for the settlement contract.
//!
//! Wraps `callora_validators` to provide settlement-specific validation
//! of metadata, offering IDs, prices, and broadcast messages.

use soroban_sdk::String;

use crate::errors::SettlementError;

/// Maximum byte length for offering IDs in the price registry.
pub const MAX_PRICE_OFFERING_ID_LEN: u32 = 64;

/// Maximum byte length for price values.
pub const MAX_PRICE_LEN: u32 = 32;

/// Maximum byte length for broadcast messages.
pub const MAX_MESSAGE_LEN: u32 = 256;

/// Validate an offering ID for the price registry.
///
/// The offering ID must be non-empty, within byte length bounds, and consist
/// only of visible ASCII characters with no leading or trailing whitespace.
///
/// # Errors
/// Returns [`SettlementError::InvalidEncoding`] if the offering ID violates
/// any of these constraints.
pub fn require_valid_offering_id(offering_id: &String) -> Result<(), SettlementError> {
    if offering_id.is_empty() || offering_id.len() > MAX_PRICE_OFFERING_ID_LEN {
        return Err(SettlementError::InvalidEncoding);
    }
    if !callora_validators::is_visible_ascii_metadata(offering_id) {
        return Err(SettlementError::InvalidEncoding);
    }
    Ok(())
}

/// Validate a price string for the price registry.
///
/// The price must be non-empty, within byte length bounds, and consist only
/// of visible ASCII characters with no leading or trailing whitespace.
///
/// # Errors
/// Returns [`SettlementError::InvalidEncoding`] if the price violates
/// any of these constraints.
pub fn require_valid_price(price: &String) -> Result<(), SettlementError> {
    if price.is_empty() || price.len() > MAX_PRICE_LEN {
        return Err(SettlementError::InvalidEncoding);
    }
    if !callora_validators::is_visible_ascii_metadata(price) {
        return Err(SettlementError::InvalidEncoding);
    }
    Ok(())
}

/// Validate a broadcast message string.
///
/// The message must be non-empty, within byte length bounds, and consist only
/// of visible ASCII characters with no leading or trailing whitespace.
///
/// # Errors
/// Returns [`SettlementError::InvalidEncoding`] if the message violates
/// any of these constraints.
pub fn require_valid_message(message: &String) -> Result<(), SettlementError> {
    if message.is_empty() || message.len() > MAX_MESSAGE_LEN {
        return Err(SettlementError::InvalidEncoding);
    }
    if !callora_validators::is_visible_ascii_metadata(message) {
        return Err(SettlementError::InvalidEncoding);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use soroban_sdk::Env;

    #[test]
    fn valid_offering_id() {
        let env = Env::default();
        let id = String::from_str(&env, "offer_123");
        assert!(require_valid_offering_id(&id).is_ok());
    }

    #[test]
    fn empty_offering_id_rejected() {
        let env = Env::default();
        let id = String::from_str(&env, "");
        assert_eq!(
            require_valid_offering_id(&id),
            Err(SettlementError::InvalidEncoding)
        );
    }

    #[test]
    fn too_long_offering_id_rejected() {
        let env = Env::default();
        let long_id: std::string::String = "a".repeat(MAX_PRICE_OFFERING_ID_LEN as usize + 1);
        let id = String::from_str(&env, &long_id);
        assert_eq!(
            require_valid_offering_id(&id),
            Err(SettlementError::InvalidEncoding)
        );
    }

    #[test]
    fn non_ascii_offering_id_rejected() {
        let env = Env::default();
        let id = String::from_str(&env, "offer\x01");
        assert_eq!(
            require_valid_offering_id(&id),
            Err(SettlementError::InvalidEncoding)
        );
    }

    #[test]
    fn leading_space_offering_id_rejected() {
        let env = Env::default();
        let id = String::from_str(&env, " offer");
        assert_eq!(
            require_valid_offering_id(&id),
            Err(SettlementError::InvalidEncoding)
        );
    }

    #[test]
    fn trailing_space_offering_id_rejected() {
        let env = Env::default();
        let id = String::from_str(&env, "offer ");
        assert_eq!(
            require_valid_offering_id(&id),
            Err(SettlementError::InvalidEncoding)
        );
    }

    #[test]
    fn valid_price() {
        let env = Env::default();
        let price = String::from_str(&env, "100.50");
        assert!(require_valid_price(&price).is_ok());
    }

    #[test]
    fn empty_price_rejected() {
        let env = Env::default();
        let price = String::from_str(&env, "");
        assert_eq!(
            require_valid_price(&price),
            Err(SettlementError::InvalidEncoding)
        );
    }

    #[test]
    fn too_long_price_rejected() {
        let env = Env::default();
        let long_price: std::string::String = "9".repeat(MAX_PRICE_LEN as usize + 1);
        let price = String::from_str(&env, &long_price);
        assert_eq!(
            require_valid_price(&price),
            Err(SettlementError::InvalidEncoding)
        );
    }

    #[test]
    fn valid_message() {
        let env = Env::default();
        let msg = String::from_str(&env, "System maintenance scheduled");
        assert!(require_valid_message(&msg).is_ok());
    }

    #[test]
    fn empty_message_rejected() {
        let env = Env::default();
        let msg = String::from_str(&env, "");
        assert_eq!(
            require_valid_message(&msg),
            Err(SettlementError::InvalidEncoding)
        );
    }

    #[test]
    fn too_long_message_rejected() {
        let env = Env::default();
        let long_msg: std::string::String = "x".repeat(MAX_MESSAGE_LEN as usize + 1);
        let msg = String::from_str(&env, &long_msg);
        assert_eq!(
            require_valid_message(&msg),
            Err(SettlementError::InvalidEncoding)
        );
    }

    #[test]
    fn control_char_message_rejected() {
        let env = Env::default();
        let msg = String::from_str(&env, "hello\x00world");
        assert_eq!(
            require_valid_message(&msg),
            Err(SettlementError::InvalidEncoding)
        );
    }

    #[test]
    fn boundary_length_price_accepted() {
        let env = Env::default();
        let price_str: std::string::String = "9".repeat(MAX_PRICE_LEN as usize);
        let price = String::from_str(&env, &price_str);
        assert!(require_valid_price(&price).is_ok());
    }

    #[test]
    fn boundary_length_message_accepted() {
        let env = Env::default();
        let msg_str: std::string::String = "a".repeat(MAX_MESSAGE_LEN as usize);
        let msg = String::from_str(&env, &msg_str);
        assert!(require_valid_message(&msg).is_ok());
    }
}

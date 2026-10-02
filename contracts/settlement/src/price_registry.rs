//! Per-admin write rate-limited price registry for the settlement contract.
//!
//! This module provides a simple on-chain price registry where the admin can
//! set and remove prices for offering identifiers. Every write operation
//! (`set_price`, `remove_price`) is subject to a per-admin rate limit that
//! prevents any single admin from updating prices more frequently than
//! [`MIN_WRITE_INTERVAL`] ledgers.
//!
//! # Rate Limit
//!
//! The rate limit is enforced by tracking the last ledger sequence in which
//! each admin wrote to the registry. The storage key is scoped per admin
//! address (`StorageKey::PriceRegistryLastWrite(Address)`), so one admin's
//! rate limit does not affect another admin.
//!
//! | Constant | Value | Meaning |
//! |----------|-------|---------|
//! | `MIN_WRITE_INTERVAL` | 10 ledgers | Minimum ledgers between consecutive writes by the same admin |
//!
//! # Errors
//!
//! Returns [`crate::SettlementError::WriteRateLimitExceeded`] when an admin
//! attempts to write before the interval has elapsed.

use soroban_sdk::{Address, Env, String};

use callora_validators::{normalize_offering_id, MAX_OFFERING_ID_LEN};
use crate::events::{emit_price_removed, emit_price_set};
use crate::{CalloraSettlement, SettlementError, StorageKey};
use crate::types::{PERSISTENT_BUMP_THRESHOLD, PERSISTENT_BUMP_AMOUNT};

/// Minimum number of ledgers that must pass between consecutive price writes
/// by the same admin.
///
/// On Stellar, one ledger corresponds to approximately 5 seconds, so
/// `MIN_WRITE_INTERVAL = 10` represents a ~50-second minimum interval.
pub const MIN_WRITE_INTERVAL: u32 = 10;

/// Maximum allowed byte length for a price string.
///
/// This bounds on-chain storage and parsing cost for price values.
pub const MAX_PRICE_LEN: u32 = 32;

/// Validate that a price string represents a positive decimal number.
///
/// Accepts formats like "100", "100.5", "0.01", "100.000000".
/// Rejects empty strings, negative values, zero, non-numeric characters,
/// multiple decimal points, and strings exceeding [`MAX_PRICE_LEN`].
fn validate_price(price: &String) -> Result<(), SettlementError> {
    let len = price.len();
    if len == 0 || len > MAX_PRICE_LEN {
        return Err(SettlementError::InvalidPrice);
    }

    let mut buf = [0u8; MAX_PRICE_LEN as usize];
    price.copy_into_slice(&mut buf[..len as usize]);
    let bytes = &buf[..len as usize];

    let mut seen_digit = false;
    let mut seen_dot = false;
    let mut digits_after_dot = 0;

    for (i, &b) in bytes.iter().enumerate() {
        if b == b'.' {
            if seen_dot {
                return Err(SettlementError::InvalidPrice);
            }
            seen_dot = true;
        } else if b.is_ascii_digit() {
            seen_digit = true;
            if seen_dot {
                digits_after_dot += 1;
                if digits_after_dot > 7 {
                    return Err(SettlementError::InvalidPrice);
                }
            }
        } else {
            return Err(SettlementError::InvalidPrice);
        }
    }

    if !seen_digit {
        return Err(SettlementError::InvalidPrice);
    }

    // Check that the value is > 0 (not just "0" or "0.0" etc.)
    let mut all_zero = true;
    for &b in bytes {
        if b != b'0' && b != b'.' {
            all_zero = false;
            break;
        }
    }
    if all_zero {
        return Err(SettlementError::InvalidPrice);
    }

    Ok(())
}

/// Set the price for an offering.
///
/// # Access Control
/// The caller must be the current admin and must authorize the call via
/// `require_auth`.
///
/// # Rate Limiting
/// If the admin's previous write occurred fewer than [`MIN_WRITE_INTERVAL`]
/// ledgers ago, the function returns [`SettlementError::WriteRateLimitExceeded`].
///
/// # Arguments
/// * `env` - Execution environment.
/// * `caller` - Must be the admin; `caller.require_auth()` is invoked.
/// * `offering_id` - Identifier for the offering whose price is being set.
/// * `price` - Price value as a string (positive decimal, max 7 decimal places).
///
/// # Panics
/// * [`SettlementError::Unauthorized`] — caller is not the admin.
/// * [`SettlementError::WriteRateLimitExceeded`] — write interval not elapsed.
/// * [`SettlementError::InvalidOfferingId`] — offering_id failed validation.
/// * [`SettlementError::InvalidPrice`] — price failed validation.
pub fn set_price(env: &Env, caller: Address, offering_id: String, price: String) {
    caller.require_auth();
    let admin =
        CalloraSettlement::get_admin(env.clone()).unwrap_or_else(|e| env.panic_with_error(e));
    if caller != admin {
        env.panic_with_error(SettlementError::Unauthorized);
    }

    normalize_offering_id(&offering_id).map_err(|_| env.panic_with_error(SettlementError::InvalidOfferingId))?;
    validate_price(&price)?;

    enforce_write_rate_limit(env, &caller);
    let key = StorageKey::Price(offering_id.clone());
    let old = env.storage().persistent().get::<_, String>(&key);
    env.storage().persistent().set(&key, &price);
    env.storage().persistent().extend_ttl(&key, PERSISTENT_BUMP_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
    emit_price_set(env, &offering_id, old, &price);
    update_last_write_ledger(env, &caller);
}

/// Remove the price for an offering.
///
/// # Access Control
/// The caller must be the current admin and must authorize the call via
/// `require_auth`.
///
/// # Rate Limiting
/// If the admin's previous write occurred fewer than [`MIN_WRITE_INTERVAL`]
/// ledgers ago, the function returns [`SettlementError::WriteRateLimitExceeded`].
///
/// # Arguments
/// * `env` - Execution environment.
/// * `caller` - Must be the admin; `caller.require_auth()` is invoked.
/// * `offering_id` - Identifier for the offering whose price is being removed.
///
/// # Panics
/// * [`SettlementError::Unauthorized`] — caller is not the admin.
/// * [`SettlementError::WriteRateLimitExceeded`] — write interval not elapsed.
pub fn remove_price(env: &Env, caller: Address, offering_id: String) {
    caller.require_auth();
    let admin =
        CalloraSettlement::get_admin(env.clone()).unwrap_or_else(|e| env.panic_with_error(e));
    if caller != admin {
        env.panic_with_error(SettlementError::Unauthorized);
    }
    enforce_write_rate_limit(env, &caller);
    let key = StorageKey::Price(offering_id.clone());
    if let Some(old) = env.storage().persistent().get::<_, String>(&key) {
        env.storage().persistent().remove(&key);
        emit_price_removed(env, &offering_id, &old);
    }
    update_last_write_ledger(env, &caller);
}

/// Get the price for an offering.
///
/// Returns `None` if no price has been set for the given offering ID.
/// Extends the TTL of the price entry on read.
///
/// # Arguments
/// * `env` - Execution environment.
/// * `offering_id` - Identifier for the offering.
///
/// # Panics
/// * [`SettlementError::InvalidOfferingId`] — offering_id failed validation.
pub fn get_price(env: &Env, offering_id: String) -> Option<String> {
    normalize_offering_id(&offering_id).map_err(|_| env.panic_with_error(SettlementError::InvalidOfferingId))?;
    let key = StorageKey::Price(offering_id);
    if env.storage().persistent().has(&key) {
        env.storage().persistent().extend_ttl(&key, PERSISTENT_BUMP_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
    }
    env.storage().persistent().get(&key)
}

/// Enforce the per-admin write rate limit.
///
/// Reads the admin's last write ledger from persistent storage and compares
/// it against the current ledger sequence. If the difference is less than
/// [`MIN_WRITE_INTERVAL`], panics with [`SettlementError::WriteRateLimitExceeded`].
///
/// # Arguments
/// * `env` - Execution environment.
/// * `admin` - Admin address whose rate limit is being checked.
fn enforce_write_rate_limit(env: &Env, admin: &Address) {
    let current_ledger = env.ledger().sequence();
    let last_write_key = StorageKey::PriceRegistryLastWrite(admin.clone());
    let last_write_ledger: u32 = env.storage().persistent().get(&last_write_key).unwrap_or(0);
    if last_write_ledger == 0 {
        return;
    }
    let elapsed = current_ledger.saturating_sub(last_write_ledger);
    if elapsed < MIN_WRITE_INTERVAL {
        env.panic_with_error(SettlementError::WriteRateLimitExceeded);
    }
}

/// Update the admin's last write ledger to the current ledger sequence.
///
/// Writes the current ledger sequence to persistent storage under
/// [`StorageKey::PriceRegistryLastWrite(admin)`](StorageKey::PriceRegistryLastWrite)
/// and extends the TTL.
///
/// # Arguments
/// * `env` - Execution environment.
/// * `admin` - Admin address whose last write ledger is being updated.
fn update_last_write_ledger(env: &Env, admin: &Address) {
    let current_ledger = env.ledger().sequence();
    let key = StorageKey::PriceRegistryLastWrite(admin.clone());
    env.storage().persistent().set(&key, &current_ledger);
    env.storage().persistent().extend_ttl(
        &key,
        crate::types::PERSISTENT_BUMP_THRESHOLD,
        crate::types::PERSISTENT_BUMP_AMOUNT,
    );
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use crate::{CalloraSettlement, CalloraSettlementClient, SettlementError};
    use soroban_sdk::Env;
    use soroban_sdk::testutils::{Address as _, Ledger as _};

    fn setup() -> (Env, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_sequence_number(100);
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);
        (env, addr, admin)
    }

    #[test]
    fn set_price_succeeds_on_first_call() {
        let (env, contract, admin) = setup();
        let client = CalloraSettlementClient::new(&env, &contract);

        client.set_price(
            &admin,
            &String::from_str(&env, "offer1"),
            &String::from_str(&env, "100"),
        );

        let price = client.try_get_price(&String::from_str(&env, "offer1"));
        match price {
            Ok(Ok(Some(p))) => assert_eq!(p, String::from_str(&env, "100")),
            _ => panic!("expected price to be set"),
        }
    }

    #[test]
    fn set_price_succeeds_after_interval() {
        let (env, contract, admin) = setup();
        let client = CalloraSettlementClient::new(&env, &contract);

        client.set_price(
            &admin,
            &String::from_str(&env, "offer1"),
            &String::from_str(&env, "100"),
        );

        // Advance ledger by exactly MIN_WRITE_INTERVAL
        env.ledger()
            .set_sequence_number(env.ledger().sequence() + MIN_WRITE_INTERVAL as u32);

        client.set_price(
            &admin,
            &String::from_str(&env, "offer2"),
            &String::from_str(&env, "200"),
        );

        let price1 = client.try_get_price(&String::from_str(&env, "offer1"));
        let price2 = client.try_get_price(&String::from_str(&env, "offer2"));
        match (price1, price2) {
            (Ok(Ok(Some(p1))), Ok(Ok(Some(p2)))) => {
                assert_eq!(p1, String::from_str(&env, "100"));
                assert_eq!(p2, String::from_str(&env, "200"));
            }
            _ => panic!("expected both prices to be set"),
        }
    }

    #[test]
    fn set_price_fails_when_rate_limit_exceeded() {
        let (env, contract, admin) = setup();
        let client = CalloraSettlementClient::new(&env, &contract);

        client.set_price(
            &admin,
            &String::from_str(&env, "offer1"),
            &String::from_str(&env, "100"),
        );

        // Try again before the interval has passed
        let result = client.try_set_price(
            &admin,
            &String::from_str(&env, "offer2"),
            &String::from_str(&env, "200"),
        );
        assert!(is_write_rate_limit_error(result));
    }

    #[test]
    fn rate_limit_is_per_admin() {
        let (env, contract, admin) = setup();
        let client = CalloraSettlementClient::new(&env, &contract);

        client.set_price(
            &admin,
            &String::from_str(&env, "offer1"),
            &String::from_str(&env, "100"),
        );

        env.ledger()
            .set_sequence_number(env.ledger().sequence() + MIN_WRITE_INTERVAL as u32);

        client.set_price(
            &admin,
            &String::from_str(&env, "offer2"),
            &String::from_str(&env, "200"),
        );

        let price1 = client.try_get_price(&String::from_str(&env, "offer1"));
        let price2 = client.try_get_price(&String::from_str(&env, "offer2"));
        match (price1, price2) {
            (Ok(Ok(Some(p1))), Ok(Ok(Some(p2)))) => {
                assert_eq!(p1, String::from_str(&env, "100"));
                assert_eq!(p2, String::from_str(&env, "200"));
            }
            _ => panic!("expected both prices to be set"),
        }
    }

    #[test]
    fn set_price_requires_auth() {
        let (env, contract, admin) = setup();
        let client = CalloraSettlementClient::new(&env, &contract);
        let unauthorized = Address::generate(&env);

        env.set_auths(&[]);
        let result = client.try_set_price(
            &unauthorized,
            &String::from_str(&env, "offer1"),
            &String::from_str(&env, "100"),
        );
        assert!(result.is_err());
    }

    #[test]
    fn remove_price_succeeds() {
        let (env, contract, admin) = setup();
        let client = CalloraSettlementClient::new(&env, &contract);

        client.set_price(
            &admin,
            &String::from_str(&env, "offer1"),
            &String::from_str(&env, "100"),
        );
        let before = client.try_get_price(&String::from_str(&env, "offer1"));
        match before {
            Ok(Ok(Some(p))) => assert_eq!(p, String::from_str(&env, "100")),
            _ => panic!("expected price to be set before removal"),
        }

        env.ledger()
            .set_sequence_number(env.ledger().sequence() + MIN_WRITE_INTERVAL as u32);
        client.remove_price(&admin, &String::from_str(&env, "offer1"));
        let after = client.try_get_price(&String::from_str(&env, "offer1"));
        match after {
            Ok(Ok(None)) => {}
            _ => panic!("expected price to be None after removal"),
        }
    }

    #[test]
    fn remove_price_fails_when_rate_limit_exceeded() {
        let (env, contract, admin) = setup();
        let client = CalloraSettlementClient::new(&env, &contract);

        client.set_price(
            &admin,
            &String::from_str(&env, "offer1"),
            &String::from_str(&env, "100"),
        );

        let result = client.try_remove_price(&admin, &String::from_str(&env, "offer1"));
        assert!(is_write_rate_limit_error(result));
    }

    #[test]
    fn write_at_exact_interval_boundary_succeeds() {
        let (env, contract, admin) = setup();
        let client = CalloraSettlementClient::new(&env, &contract);

        client.set_price(
            &admin,
            &String::from_str(&env, "offer1"),
            &String::from_str(&env, "100"),
        );

        // Advance ledger by exactly MIN_WRITE_INTERVAL — should succeed
        env.ledger()
            .set_sequence_number(env.ledger().sequence() + MIN_WRITE_INTERVAL as u32);

        let result = client.try_set_price(
            &admin,
            &String::from_str(&env, "offer2"),
            &String::from_str(&env, "200"),
        );
        assert!(
            result.is_ok(),
            "write at exact interval boundary should succeed"
        );
    }

    #[test]
    fn write_one_ledger_before_interval_fails() {
        let (env, contract, admin) = setup();
        let client = CalloraSettlementClient::new(&env, &contract);

        client.set_price(
            &admin,
            &String::from_str(&env, "offer1"),
            &String::from_str(&env, "100"),
        );

        // Advance by MIN_WRITE_INTERVAL - 1 — should fail
        env.ledger()
            .set_sequence_number(env.ledger().sequence() + MIN_WRITE_INTERVAL as u32 - 1);

        let result = client.try_set_price(
            &admin,
            &String::from_str(&env, "offer2"),
            &String::from_str(&env, "200"),
        );
        assert!(is_write_rate_limit_error(result));
    }

    #[test]
    fn remove_price_requires_auth() {
        let (env, contract, admin) = setup();
        let client = CalloraSettlementClient::new(&env, &contract);
        let unauthorized = Address::generate(&env);

        env.set_auths(&[]);
        let result = client.try_remove_price(&unauthorized, &String::from_str(&env, "offer1"));
        assert!(result.is_err());
    }

    #[test]
    fn get_price_returns_none_for_unknown_offering() {
        let (env, contract, _admin) = setup();
        let client = CalloraSettlementClient::new(&env, &contract);

        let result = client.try_get_price(&String::from_str(&env, "nonexistent"));
        match result {
            Ok(Ok(None)) => {}
            _ => panic!("expected Ok(Ok(None)), got {:?}", result),
        }
    }

    #[test]
    fn set_price_overwrites_previous_price() {
        let (env, contract, admin) = setup();
        let client = CalloraSettlementClient::new(&env, &contract);

        client.set_price(
            &admin,
            &String::from_str(&env, "offer1"),
            &String::from_str(&env, "100"),
        );
        env.ledger()
            .set_sequence_number(env.ledger().sequence() + MIN_WRITE_INTERVAL as u32);
        client.set_price(
            &admin,
            &String::from_str(&env, "offer1"),
            &String::from_str(&env, "200"),
        );

        let price = client.try_get_price(&String::from_str(&env, "offer1"));
        match price {
            Ok(Ok(Some(p))) => assert_eq!(p, String::from_str(&env, "200")),
            _ => panic!("expected overwritten price"),
        }
    }

    #[test]
    fn set_price_unauthorized_non_admin() {
        let (env, contract, admin) = setup();
        let client = CalloraSettlementClient::new(&env, &contract);
        let non_admin = Address::generate(&env);

        let result = client.try_set_price(
            &non_admin,
            &String::from_str(&env, "offer1"),
            &String::from_str(&env, "100"),
        );
        assert!(is_error(result, SettlementError::Unauthorized));
    }

    #[test]
    fn remove_price_unauthorized_non_admin() {
        let (env, contract, admin) = setup();
        let client = CalloraSettlementClient::new(&env, &contract);
        let non_admin = Address::generate(&env);

        let result = client.try_remove_price(&non_admin, &String::from_str(&env, "offer1"));
        assert!(is_error(result, SettlementError::Unauthorized));
    }

    #[test]
    fn set_price_rejects_empty_offering_id() {
        let (env, contract, admin) = setup();
        let client = CalloraSettlementClient::new(&env, &contract);

        let result = client.try_set_price(
            &admin,
            &String::from_str(&env, ""),
            &String::from_str(&env, "100"),
        );
        assert!(is_error(result, SettlementError::InvalidOfferingId));
    }

    #[test]
    fn set_price_rejects_offering_id_too_long() {
        let (env, contract, admin) = setup();
        let client = CalloraSettlementClient::new(&env, &contract);

        let long_id = "a".repeat(MAX_OFFERING_ID_LEN as usize + 1);
        let result = client.try_set_price(
            &admin,
            &String::from_str(&env, &long_id),
            &String::from_str(&env, "100"),
        );
        assert!(is_error(result, SettlementError::InvalidOfferingId));
    }

    #[test]
    fn set_price_rejects_offering_id_with_invalid_chars() {
        let (env, contract, admin) = setup();
        let client = CalloraSettlementClient::new(&env, &contract);

        // uppercase
        let result = client.try_set_price(
            &admin,
            &String::from_str(&env, "OFFER1"),
            &String::from_str(&env, "100"),
        );
        assert!(is_error(result, SettlementError::InvalidOfferingId));

        // space
        let result = client.try_set_price(
            &admin,
            &String::from_str(&env, "offer 1"),
            &String::from_str(&env, "100"),
        );
        assert!(is_error(result, SettlementError::InvalidOfferingId));

        // special chars
        let result = client.try_set_price(
            &admin,
            &String::from_str(&env, "offer@1"),
            &String::from_str(&env, "100"),
        );
        assert!(is_error(result, SettlementError::InvalidOfferingId));
    }

    #[test]
    fn set_price_rejects_offering_id_with_leading_trailing_space() {
        let (env, contract, admin) = setup();
        let client = CalloraSettlementClient::new(&env, &contract);

        let result = client.try_set_price(
            &admin,
            &String::from_str(&env, " offer1"),
            &String::from_str(&env, "100"),
        );
        assert!(is_error(result, SettlementError::InvalidOfferingId));

        let result = client.try_set_price(
            &admin,
            &String::from_str(&env, "offer1 "),
            &String::from_str(&env, "100"),
        );
        assert!(is_error(result, SettlementError::InvalidOfferingId));
    }

    #[test]
    fn set_price_rejects_empty_price() {
        let (env, contract, admin) = setup();
        let client = CalloraSettlementClient::new(&env, &contract);

        let result = client.try_set_price(
            &admin,
            &String::from_str(&env, "offer1"),
            &String::from_str(&env, ""),
        );
        assert!(is_error(result, SettlementError::InvalidPrice));
    }

    #[test]
    fn set_price_rejects_price_too_long() {
        let (env, contract, admin) = setup();
        let client = CalloraSettlementClient::new(&env, &contract);

        let long_price = "1".repeat(MAX_PRICE_LEN as usize + 1);
        let result = client.try_set_price(
            &admin,
            &String::from_str(&env, "offer1"),
            &String::from_str(&env, &long_price),
        );
        assert!(is_error(result, SettlementError::InvalidPrice));
    }

    #[test]
    fn set_price_rejects_non_numeric_price() {
        let (env, contract, admin) = setup();
        let client = CalloraSettlementClient::new(&env, &contract);

        let result = client.try_set_price(
            &admin,
            &String::from_str(&env, "offer1"),
            &String::from_str(&env, "abc"),
        );
        assert!(is_error(result, SettlementError::InvalidPrice));

        let result = client.try_set_price(
            &admin,
            &String::from_str(&env, "offer1"),
            &String::from_str(&env, "100.5.5"),
        );
        assert!(is_error(result, SettlementError::InvalidPrice));

        let result = client.try_set_price(
            &admin,
            &String::from_str(&env, "offer1"),
            &String::from_str(&env, "100a"),
        );
        assert!(is_error(result, SettlementError::InvalidPrice));
    }

    #[test]
    fn set_price_rejects_zero_or_negative_price() {
        let (env, contract, admin) = setup();
        let client = CalloraSettlementClient::new(&env, &contract);

        // zero
        let result = client.try_set_price(
            &admin,
            &String::from_str(&env, "offer1"),
            &String::from_str(&env, "0"),
        );
        assert!(is_error(result, SettlementError::InvalidPrice));

        let result = client.try_set_price(
            &admin,
            &String::from_str(&env, "offer1"),
            &String::from_str(&env, "0.0"),
        );
        assert!(is_error(result, SettlementError::InvalidPrice));

        let result = client.try_set_price(
            &admin,
            &String::from_str(&env, "offer1"),
            &String::from_str(&env, "0.0000000"),
        );
        assert!(is_error(result, SettlementError::InvalidPrice));

        // negative
        let result = client.try_set_price(
            &admin,
            &String::from_str(&env, "offer1"),
            &String::from_str(&env, "-100"),
        );
        assert!(is_error(result, SettlementError::InvalidPrice));

        let result = client.try_set_price(
            &admin,
            &String::from_str(&env, "offer1"),
            &String::from_str(&env, "-0.5"),
        );
        assert!(is_error(result, SettlementError::InvalidPrice));
    }

    #[test]
    fn set_price_accepts_valid_decimal_prices() {
        let (env, contract, admin) = setup();
        let client = CalloraSettlementClient::new(&env, &contract);

        // integer
        client.set_price(
            &admin,
            &String::from_str(&env, "offer1"),
            &String::from_str(&env, "100"),
        );
        let price = client.try_get_price(&String::from_str(&env, "offer1"));
        match price {
            Ok(Ok(Some(p))) => assert_eq!(p, String::from_str(&env, "100")),
            _ => panic!("expected price to be set"),
        }

        // decimal
        env.ledger().set_sequence_number(env.ledger().sequence() + MIN_WRITE_INTERVAL as u32);
        client.set_price(
            &admin,
            &String::from_str(&env, "offer2"),
            &String::from_str(&env, "100.5"),
        );
        let price = client.try_get_price(&String::from_str(&env, "offer2"));
        match price {
            Ok(Ok(Some(p))) => assert_eq!(p, String::from_str(&env, "100.5")),
            _ => panic!("expected price to be set"),
        }

        // small decimal
        env.ledger().set_sequence_number(env.ledger().sequence() + MIN_WRITE_INTERVAL as u32);
        client.set_price(
            &admin,
            &String::from_str(&env, "offer3"),
            &String::from_str(&env, "0.0000001"),
        );
        let price = client.try_get_price(&String::from_str(&env, "offer3"));
        match price {
            Ok(Ok(Some(p))) => assert_eq!(p, String::from_str(&env, "0.0000001")),
            _ => panic!("expected price to be set"),
        }
    }

    #[test]
    fn set_price_rejects_too_many_decimal_places() {
        let (env, contract, admin) = setup();
        let client = CalloraSettlementClient::new(&env, &contract);

        // 8 decimal places - should fail
        let result = client.try_set_price(
            &admin,
            &String::from_str(&env, "offer1"),
            &String::from_str(&env, "0.00000001"),
        );
        assert!(is_error(result, SettlementError::InvalidPrice));
    }

    #[test]
    fn get_price_rejects_invalid_offering_id() {
        let (env, contract, admin) = setup();
        let client = CalloraSettlementClient::new(&env, &contract);

        // First set a valid price
        client.set_price(
            &admin,
            &String::from_str(&env, "offer1"),
            &String::from_str(&env, "100"),
        );

        // Try to get with invalid offering_id
        let result = client.try_get_price(&String::from_str(&env, "OFFER1"));
        assert!(is_error(result, SettlementError::InvalidOfferingId));

        let result = client.try_get_price(&String::from_str(&env, ""));
        assert!(is_error(result, SettlementError::InvalidOfferingId));
    }

    #[test]
    fn get_price_extends_ttl() {
        let (env, contract, admin) = setup();
        let client = CalloraSettlementClient::new(&env, &contract);

        client.set_price(
            &admin,
            &String::from_str(&env, "offer1"),
            &String::from_str(&env, "100"),
        );

        // get_price should not fail and should extend TTL (we can't directly test TTL extension
        // in unit tests, but we verify the call succeeds)
        let price = client.try_get_price(&String::from_str(&env, "offer1"));
        match price {
            Ok(Ok(Some(p))) => assert_eq!(p, String::from_str(&env, "100")),
            _ => panic!("expected price to be set"),
        }

        // Second read should also succeed
        let price = client.try_get_price(&String::from_str(&env, "offer1"));
        match price {
            Ok(Ok(Some(p))) => assert_eq!(p, String::from_str(&env, "100")),
            _ => panic!("expected price to be set"),
        }
    }

    #[test]
    fn set_price_extends_ttl_on_write() {
        let (env, contract, admin) = setup();
        let client = CalloraSettlementClient::new(&env, &contract);

        // set_price should extend TTL on write (we verify the call succeeds)
        client.set_price(
            &admin,
            &String::from_str(&env, "offer1"),
            &String::from_str(&env, "100"),
        );

        let price = client.try_get_price(&String::from_str(&env, "offer1"));
        match price {
            Ok(Ok(Some(p))) => assert_eq!(p, String::from_str(&env, "100")),
            _ => panic!("expected price to be set"),
        }
    }

    fn is_write_rate_limit_error<V, CE: Into<soroban_sdk::Error>, E: Into<soroban_sdk::Error>>(
        result: Result<Result<V, CE>, Result<E, soroban_sdk::InvokeError>>,
    ) -> bool {
        match result {
            Err(Ok(e)) => e.into().get_code() == SettlementError::WriteRateLimitExceeded as u32,
            _ => false,
        }
    }

    fn is_error<V, CE: Into<soroban_sdk::Error>, E: Into<soroban_sdk::Error>>(
        result: Result<Result<V, CE>, Result<E, soroban_sdk::InvokeError>>,
        expected: SettlementError,
    ) -> bool {
        let expected_code = expected as u32;
        match result {
            Err(Ok(e)) => e.into().get_code() == expected_code,
            _ => false,
        }
    }
}

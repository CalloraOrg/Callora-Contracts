#![no_std]
use soroban_sdk::{Address, Env, String, contract, contracterror, contractimpl, contracttype};

mod events;

/// Maximum length, in bytes, of an error description accepted by
/// [`ErrorsContract::register_error`] and [`ErrorsContract::update_error`].
///
/// Error descriptions become part of the contract's public interface (see
/// `docs/ERROR_CODES.md`), so they must be bounded to keep storage entries
/// and event payloads predictable. Matches the 256-byte convention used by
/// `MAX_METADATA_LEN` (registry) and `MAX_MESSAGE_LEN` (revenue_pool).
pub const MAX_DESC_LEN: u32 = 256;

/// Persistent-entry TTL threshold: entries below this remaining TTL are
/// extended on the next write (`register_error` / `update_error`).
pub const PERSISTENT_BUMP_THRESHOLD: u32 = 50_000;

/// Persistent-entry TTL bump target: a successful write leaves the
/// `ErrorReg(code)` entry with exactly this many ledgers of remaining TTL.
pub const PERSISTENT_BUMP_AMOUNT: u32 = 50_000;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    Unauthorized = 3,
    Overflow = 4,
    /// Supplied description exceeds [`MAX_DESC_LEN`] bytes.
    DescriptionTooLong = 5,
    /// The code is already registered; use `update_error` to change it.
    AlreadyRegistered = 6,
    /// `update_error` was called for a code that was never registered.
    NotRegistered = 7,
}

#[contracttype]
pub enum DataKey {
    Admin,
    ErrorReg(u32),
    RecentErr(Address),
}

#[contract]
pub struct ErrorsContract;

#[contractimpl]
impl ErrorsContract {
    pub fn init(env: Env, admin: Address) -> Result<(), Error> {
        admin.require_auth();

        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        Ok(())
    }

    /// Register a new error-code description.
    ///
    /// The admin's authorization is required and the caller must be the
    /// stored admin.
    ///
    /// Behaviour:
    /// * `desc` longer than [`MAX_DESC_LEN`] bytes is rejected with
    ///   [`Error::DescriptionTooLong`] — nothing is written.
    /// * An already-registered `code` is rejected with
    ///   [`Error::AlreadyRegistered`]; descriptions are immutable once
    ///   registered and may only be changed via [`Self::update_error`].
    /// * On success the `ErrorReg(code)` persistent entry is written, its
    ///   TTL is extended to [`PERSISTENT_BUMP_AMOUNT`] ledgers, and an
    ///   `error_registered` event (topic `(error_registered, admin)`,
    ///   payload `(code, desc)`) is emitted.
    pub fn register_error(env: Env, admin: Address, code: u32, desc: String) -> Result<(), Error> {
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }

        if desc.len() > MAX_DESC_LEN {
            return Err(Error::DescriptionTooLong);
        }

        let key = DataKey::ErrorReg(code);
        if env.storage().persistent().has(&key) {
            return Err(Error::AlreadyRegistered);
        }

        env.storage().persistent().set(&key, &desc);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_BUMP_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_error_registered(&env, &admin, code, &desc);
        Ok(())
    }

    /// Replace the description of an already-registered error code.
    ///
    /// This is the explicit update path: `register_error` never overwrites.
    /// The admin's authorization is required and the caller must be the
    /// stored admin.
    ///
    /// Behaviour:
    /// * `desc` longer than [`MAX_DESC_LEN`] bytes is rejected with
    ///   [`Error::DescriptionTooLong`] — nothing is written.
    /// * A code that was never registered is rejected with
    ///   [`Error::NotRegistered`].
    /// * On success the `ErrorReg(code)` persistent entry is rewritten, its
    ///   TTL is extended to [`PERSISTENT_BUMP_AMOUNT`] ledgers, and an
    ///   `error_updated` event (topic `(error_updated, admin)`, payload
    ///   `(code, desc)`) is emitted.
    pub fn update_error(env: Env, admin: Address, code: u32, desc: String) -> Result<(), Error> {
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }

        if desc.len() > MAX_DESC_LEN {
            return Err(Error::DescriptionTooLong);
        }

        let key = DataKey::ErrorReg(code);
        if !env.storage().persistent().has(&key) {
            return Err(Error::NotRegistered);
        }

        env.storage().persistent().set(&key, &desc);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_BUMP_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_error_updated(&env, &admin, code, &desc);
        Ok(())
    }

    pub fn log_error(env: Env, user: Address, code: u32) -> Result<(), Error> {
        user.require_auth();

        // Overflow-safe: checked_add prevents silent wrap at u32::MAX.
        // This is the only arithmetic path in the contract; all other
        // operations are storage reads/writes and comparisons.
        let _safe_calc = code.checked_add(1).ok_or(Error::Overflow)?;

        env.storage()
            .temporary()
            .set(&DataKey::RecentErr(user.clone()), &code);

        env.storage()
            .temporary()
            .extend_ttl(&DataKey::RecentErr(user), 100, 100);

        Ok(())
    }
}

#[cfg(test)]
mod test;

#![no_std]
use soroban_sdk::{
    Address, Env, String, Symbol, Vec, contract, contracterror, contractimpl, contracttype,
};

mod events;

/// Maximum length, in bytes, of an error description accepted by
/// [`ErrorsContract::register_error`] and [`ErrorsContract::update_error`].
///
/// Error descriptions become part of the contract's public interface (see
/// `docs/ERROR_CODES.md`), so they must be bounded to keep storage entries
/// and event payloads predictable. Matches the 256-byte convention used by
/// `MAX_METADATA_LEN` (registry) and `MAX_MESSAGE_LEN` (revenue_pool).
pub const MAX_DESC_LEN: u32 = 256;

/// Maximum number of error codes retained per user by [`ErrorsContract::log_error`].
///
/// The history is a bounded ring buffer: once it holds
/// [`MAX_ERROR_HISTORY`] entries, logging a new code evicts the oldest one.
/// Keeping the bound small makes per-user temporary storage predictable.
pub const MAX_ERROR_HISTORY: u32 = 8;

/// Maximum number of error codes returned in a single page by
/// [`ErrorsContract::list_error_codes`].
pub const MAX_PAGE_LIMIT: u32 = 100;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    Unauthorized = 3,
    Overflow = 4,
    /// `log_error` was called with a code that `register_error` never defined.
    UnknownErrorCode = 5,
    /// Supplied description exceeds [`MAX_DESC_LEN`] bytes.
    DescriptionTooLong = 6,
    /// The code is already registered; use `update_error` to change it.
    AlreadyRegistered = 7,
    /// `update_error` was called for a code that was never registered.
    NotRegistered = 8,
    /// `deprecate_error` was called for a code that has already been deprecated.
    AlreadyDeprecated = 9,
    /// `log_error` was called with a code that has been deprecated.
    DeprecatedErrorCode = 10,
}

#[contracttype]
pub enum DataKey {
    Admin,
    ErrorReg(u32),
    RecentErr(Address),
    /// Bounded oldest-to-newest history of codes logged by a user.
    /// Appended last so existing XDR discriminants stay stable.
    ErrHistory(Address),
    /// Total count of unique registered error codes.
    CodeCount,
    /// Registration order index (0-based) -> error code.
    CodeAt(u32),
    /// Deprecation flag for a registered error code.
    Deprecated(u32),
}

/// Ledger cadence used across the workspace for TTL policy.
const LEDGERS_PER_DAY: u32 = 17_280;

/// TTL policy for the contract instance holding the admin address.
///
/// The admin lives in instance storage, which must be refreshed on every
/// entrypoint — including read-only views — so the contract cannot archive out
/// from under integrators that only ever register or resolve codes. Extends to
/// ~60 days whenever the remaining TTL falls below ~30 days, mirroring the
/// workspace convention (`contracts/fee`, `contracts/vault`).
const INSTANCE_BUMP_THRESHOLD: u32 = LEDGERS_PER_DAY * 30;
const INSTANCE_BUMP_AMOUNT: u32 = LEDGERS_PER_DAY * 60;

/// Lifetime of the temporary `RecentErr(Address)` and `ErrHistory(Address)`
/// entries — the most recent code and the bounded history logged by a user.
///
/// This is deliberately short: the values exist only for short-term debugging
/// and rate-limiting feedback, so they are written with a ~8-minute lifetime
/// (`RECENT_ERR_TTL_LEDGERS` ledgers at 5 s/ledger) and left to expire
/// naturally. They are intentionally *not* refreshed on reads and must not be
/// relied upon as durable state — use the persistent `ErrorReg(code)` registry
/// for anything that needs to survive. Both entries share the same TTL so the
/// history and the most-recent value expire together.
const RECENT_ERR_TTL_LEDGERS: u32 = 100;

/// TTL policy for the persistent error registry.
///
/// A registry entry is refreshed when its remaining TTL drops below ~30 days,
/// extending it back out to ~60 days. This mirrors the repo-wide convention
/// (`contracts/fee`: `INSTANCE_BUMP_THRESHOLD` = 30 days,
/// `INSTANCE_BUMP_AMOUNT` = 60 days) so a long-lived registry cannot archive
/// out from under integrators that resolve descriptions on the read path.
const REGISTRY_TTL_THRESHOLD: u32 = LEDGERS_PER_DAY * 30;
const REGISTRY_TTL_BUMP: u32 = LEDGERS_PER_DAY * 60;

#[contract]
pub struct ErrorsContract;

#[contractimpl]
impl ErrorsContract {
    /// Extend the instance storage TTL holding the admin address.
    ///
    /// Called on every entrypoint, including read-only views, before any
    /// instance state is accessed, so the contract instance never archives due
    /// to inactivity.
    fn bump_instance(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    pub fn init(env: Env, admin: Address) -> Result<(), Error> {
        admin.require_auth();
        Self::bump_instance(&env);

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
    ///   TTL is extended to `REGISTRY_TTL_BUMP` ledgers, and an
    ///   `error_registered` event (topic `(error_registered, admin)`,
    ///   payload `(code, desc)`) is emitted.
    pub fn register_error(env: Env, admin: Address, code: u32, desc: String) -> Result<(), Error> {
        admin.require_auth();
        Self::bump_instance(&env);

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
        env.storage()
            .persistent()
            .extend_ttl(&key, REGISTRY_TTL_THRESHOLD, REGISTRY_TTL_BUMP);

        // Append to registered code index for list_error_codes
        let count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::CodeCount)
            .unwrap_or(0);
        let at_key = DataKey::CodeAt(count);
        env.storage().persistent().set(&at_key, &code);
        env.storage()
            .persistent()
            .extend_ttl(&at_key, REGISTRY_TTL_THRESHOLD, REGISTRY_TTL_BUMP);
        let new_count = count.checked_add(1).ok_or(Error::Overflow)?;
        env.storage()
            .instance()
            .set(&DataKey::CodeCount, &new_count);

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
    ///   TTL is extended to `REGISTRY_TTL_BUMP` ledgers, and an
    ///   `error_updated` event (topic `(error_updated, admin)`, payload
    ///   `(code, desc)`) is emitted.
    pub fn update_error(env: Env, admin: Address, code: u32, desc: String) -> Result<(), Error> {
        admin.require_auth();
        Self::bump_instance(&env);

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
        env.storage()
            .persistent()
            .extend_ttl(&key, REGISTRY_TTL_THRESHOLD, REGISTRY_TTL_BUMP);

        events::emit_error_updated(&env, &admin, code, &desc);
        Ok(())
    }

    /// Mark an existing error code as deprecated (admin only).
    ///
    /// Deprecating a code retains its persistent description entry so past references
    /// and [`Self::get_error_description`] lookups continue to resolve. However,
    /// subsequent [`Self::log_error`] calls for the code will be rejected with
    /// [`Error::DeprecatedErrorCode`].
    ///
    /// # Errors
    /// * [`Error::NotInitialized`] when the contract has not been initialized.
    /// * [`Error::Unauthorized`] when `admin` is not the stored admin address.
    /// * [`Error::NotRegistered`] when `code` has not been registered.
    /// * [`Error::AlreadyDeprecated`] when `code` is already deprecated.
    pub fn deprecate_error(env: Env, admin: Address, code: u32) -> Result<(), Error> {
        admin.require_auth();
        Self::bump_instance(&env);

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }

        let reg_key = DataKey::ErrorReg(code);
        if !env.storage().persistent().has(&reg_key) {
            return Err(Error::NotRegistered);
        }

        let dep_key = DataKey::Deprecated(code);
        if env.storage().persistent().has(&dep_key) {
            return Err(Error::AlreadyDeprecated);
        }

        env.storage().persistent().set(&dep_key, &true);
        env.storage()
            .persistent()
            .extend_ttl(&dep_key, REGISTRY_TTL_THRESHOLD, REGISTRY_TTL_BUMP);

        events::emit_error_deprecated(&env, &admin, code);
        Ok(())
    }

    /// Return the description registered for `code`, or `None` when the code is
    /// unknown.
    ///
    /// A successful lookup refreshes the registry entry's TTL, so codes that
    /// integrators actively resolve stay available. Deprecated codes still resolve.
    pub fn get_error_description(env: Env, code: u32) -> Option<String> {
        Self::bump_instance(&env);
        let key = DataKey::ErrorReg(code);
        let desc: Option<String> = env.storage().persistent().get(&key);
        if desc.is_some() {
            env.storage()
                .persistent()
                .extend_ttl(&key, REGISTRY_TTL_THRESHOLD, REGISTRY_TTL_BUMP);
        }
        desc
    }

    /// Log an error for `user`.
    ///
    /// Only codes previously defined by [`Self::register_error`] and not retired
    /// by [`Self::deprecate_error`] are accepted. Anything unregistered returns
    /// [`Error::UnknownErrorCode`], while deprecated codes return
    /// [`Error::DeprecatedErrorCode`].
    /// On success the code is stored as the user's most recent error, appended
    /// to their bounded history (see [`Self::get_error_history`]), and an
    /// `error_logged` event is published.
    ///
    /// Both temporary entries expire after roughly eight minutes
    /// (`RECENT_ERR_TTL_LEDGERS` ledgers) unless another log refreshes them.
    pub fn log_error(env: Env, user: Address, code: u32) -> Result<(), Error> {
        user.require_auth();
        Self::bump_instance(&env);

        let reg_key = DataKey::ErrorReg(code);

        // Only registered codes may be logged. Reject before touching either
        // temporary entry so a failed call leaves the history untouched.
        if !env.storage().persistent().has(&reg_key) {
            return Err(Error::UnknownErrorCode);
        }

        // Deprecated codes are rejected with a typed error.
        let dep_key = DataKey::Deprecated(code);
        if env.storage().persistent().has(&dep_key) {
            return Err(Error::DeprecatedErrorCode);
        }

        // A code that is actively logged is actively used — keep it warm.
        env.storage()
            .persistent()
            .extend_ttl(&reg_key, REGISTRY_TTL_THRESHOLD, REGISTRY_TTL_BUMP);

        let recent_key = DataKey::RecentErr(user.clone());
        env.storage().temporary().set(&recent_key, &code);
        env.storage().temporary().extend_ttl(
            &recent_key,
            RECENT_ERR_TTL_LEDGERS,
            RECENT_ERR_TTL_LEDGERS,
        );

        let history_key = DataKey::ErrHistory(user.clone());
        let mut history: Vec<u32> = env
            .storage()
            .temporary()
            .get(&history_key)
            .unwrap_or_else(|| Vec::new(&env));
        if history.len() >= MAX_ERROR_HISTORY {
            history.remove(0);
        }
        history.push_back(code);
        env.storage().temporary().set(&history_key, &history);
        env.storage().temporary().extend_ttl(
            &history_key,
            RECENT_ERR_TTL_LEDGERS,
            RECENT_ERR_TTL_LEDGERS,
        );

        env.events()
            .publish((Symbol::new(&env, "error_logged"), user), code);

        Ok(())
    }

    /// Return the most recent code logged by `user`, or `None` if they never
    /// logged an error.
    ///
    /// This is the single-value companion to [`Self::get_error_history`]; it
    /// is unchanged by the history ring buffer. The value lives in temporary
    /// storage and expires after ~8 minutes of inactivity.
    pub fn get_recent_error(env: Env, user: Address) -> Option<u32> {
        Self::bump_instance(&env);
        env.storage().temporary().get(&DataKey::RecentErr(user))
    }

    /// Return the last [`MAX_ERROR_HISTORY`] codes logged by `user`, oldest
    /// first.
    ///
    /// Returns an empty vector when nothing is stored. The history is kept in
    /// temporary storage with the same ~8-minute lifetime as
    /// [`Self::get_recent_error`]; reads do not refresh that lifetime, so the
    /// value is diagnostic and must not be relied on as durable state. This
    /// view requires no authentication.
    pub fn get_error_history(env: Env, user: Address) -> Vec<u32> {
        Self::bump_instance(&env);
        env.storage()
            .temporary()
            .get(&DataKey::ErrHistory(user))
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Return a paginated list of registered error codes and their deprecation
    /// flags in registration order.
    ///
    /// Up to [`MAX_PAGE_LIMIT`] entries are returned starting from `cursor`.
    /// Reading refreshes the TTL of each indexed entry, its deprecation flag,
    /// and its underlying error description. A `cursor` at or past the total
    /// number of registered codes returns an empty vector.
    pub fn list_error_codes(env: Env, cursor: u32, limit: u32) -> Vec<(u32, bool)> {
        Self::bump_instance(&env);
        let mut list = Vec::new(&env);
        let count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::CodeCount)
            .unwrap_or(0);
        if cursor >= count || limit == 0 {
            return list;
        }

        let page_limit = limit.min(MAX_PAGE_LIMIT);
        let end = cursor.saturating_add(page_limit).min(count);
        for i in cursor..end {
            let at_key = DataKey::CodeAt(i);
            if let Some(code) = env.storage().persistent().get::<_, u32>(&at_key) {
                env.storage().persistent().extend_ttl(
                    &at_key,
                    REGISTRY_TTL_THRESHOLD,
                    REGISTRY_TTL_BUMP,
                );
                let dep_key = DataKey::Deprecated(code);
                let deprecated = env.storage().persistent().has(&dep_key);
                if deprecated {
                    env.storage().persistent().extend_ttl(
                        &dep_key,
                        REGISTRY_TTL_THRESHOLD,
                        REGISTRY_TTL_BUMP,
                    );
                }
                let reg_key = DataKey::ErrorReg(code);
                if env.storage().persistent().has(&reg_key) {
                    env.storage().persistent().extend_ttl(
                        &reg_key,
                        REGISTRY_TTL_THRESHOLD,
                        REGISTRY_TTL_BUMP,
                    );
                }
                list.push_back((code, deprecated));
            }
        }
        list
    }
}

#[cfg(test)]
mod test;

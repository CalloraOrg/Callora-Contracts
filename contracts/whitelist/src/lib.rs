#![no_std]
#![allow(clippy::enum_variant_names)]
//!
//! # Callora Whitelist Contract
//!
//! Manages a whitelist of authorized addresses. Admin actions that modify the
//! whitelist — `add_address`, `remove_address`, `clear_all` — are protected by
//! a configurable **cool-off window** that prevents rapid successive operations.
//!
//! ## Cool-Off Mechanism
//!
//! Each successful state-changing admin action arms a global timer. Subsequent
//! actions are rejected until the window elapses. This gives monitors and
//! off-chain governance time to react before another whitelist mutation can occur.
//!
//! - Default window: **1 hour** (`DEFAULT_COOLDOWN_SECONDS`)
//! - Configurable range: **1 second** – **30 days**
//! - Configured via `set_admin_cooldown` (admin only)
//!
//! ## Admin Roles
//!
//! - **Owner**: set at `init`, can transfer ownership (two-step)
//! - **Admin**: defaults to owner, can be transferred (two-step)
//!
//! Both the owner and admin can manage the whitelist, but the cool-off window
//! applies regardless of which role performs the action.
//!
//! ## Temporary Grants
//!
//! `add_address` grants a permanent entry. `add_address_until` grants a
//! temporary entry that lapses on its own: the address is whitelisted strictly
//! before `expires_at` and is no longer whitelisted at or after it. Expired
//! entries are filtered from `get_whitelist` and may be re-added (which
//! replaces the stale entry). `remove_address` and `clear_all` delete any
//! associated expiry data.

mod errors;
pub use errors::WhitelistError;

pub mod admin;
pub mod events;

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol, Vec};

/// Instance / persistent storage keys for the whitelist contract.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum StorageKey {
    /// Contract owner address.
    WhitelistOwner,
    /// Current admin address (defaults to owner at init).
    WhitelistAdmin,
    /// Pending admin address awaiting acceptance (two-step transfer).
    WhitelistPendingAdmin,
    /// Vector of whitelisted addresses.
    WhitelistList,
    /// Global cool-off window between critical whitelist admin executions.
    WhitelistAdminCooldown,
    /// Audit record for the most recently executed critical whitelist admin action.
    WhitelistLastCriticalAction,
    /// Optional expiry timestamp for an address added with `add_address_until`.
    ///
    /// Appended at the end so every pre-existing discriminant stays stable.
    WhitelistExpiry(Address),
}

// ---------------------------------------------------------------------------
// TTL Constants
// ---------------------------------------------------------------------------

/// Ledgers per day at a 5-second close cadence.
const LEDGERS_PER_DAY: u32 = 17_280;

/// TTL extension trigger for instance storage keys (~30 days of ledgers at 5 s/ledger).
pub const INSTANCE_BUMP_THRESHOLD: u32 = LEDGERS_PER_DAY * 30;

/// TTL extension target for instance storage keys (~60 days of ledgers at 5 s/ledger).
pub const INSTANCE_BUMP_AMOUNT: u32 = LEDGERS_PER_DAY * 60;

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct CalloraWhitelist;

#[contractimpl]
impl CalloraWhitelist {
    // -----------------------------------------------------------------------
    // Initialization
    // -----------------------------------------------------------------------

    /// Initialize the whitelist contract (one-time setup).
    ///
    /// Sets the contract owner and admin (both default to `admin`). The contract
    /// may only be initialized once.
    ///
    /// # Parameters
    /// - `admin` — initial owner and admin address.
    ///
    /// # Errors
    /// - [`WhitelistError::AlreadyInitialized`] if `init` has already been called.
    pub fn init(env: Env, admin: Address) -> Result<(), WhitelistError> {
        admin.require_auth();

        if env.storage().instance().has(&StorageKey::WhitelistOwner) {
            return Err(WhitelistError::AlreadyInitialized);
        }

        env.storage()
            .instance()
            .set(&StorageKey::WhitelistOwner, &admin);
        env.storage()
            .instance()
            .set(&StorageKey::WhitelistAdmin, &admin);

        Self::bump_instance_ttl(&env);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Admin management
    // -----------------------------------------------------------------------

    /// Return the current admin address.
    ///
    /// Defaults to the owner when no admin has been explicitly set.
    ///
    /// # Errors
    /// - [`WhitelistError::NotInitialized`] if the contract has not been initialized.
    pub fn get_admin(env: Env) -> Result<Address, WhitelistError> {
        Self::bump_instance_ttl(&env);
        env.storage()
            .instance()
            .get::<_, Address>(&StorageKey::WhitelistAdmin)
            .ok_or(WhitelistError::NotInitialized)
    }

    /// Initiate a two-step admin transfer (current admin only).
    ///
    /// The nominated admin must call [`accept_admin`] to complete the transfer.
    ///
    /// # Parameters
    /// - `caller` — Must be the current admin.
    /// - `new_admin` — Address to nominate as the next admin.
    ///
    /// # Errors
    /// - [`WhitelistError::Unauthorized`] if `caller` is not the admin.
    /// - [`WhitelistError::NewAdminSameAsCurrent`] if `new_admin` is the same as the current admin.
    pub fn set_admin(env: Env, caller: Address, new_admin: Address) -> Result<(), WhitelistError> {
        Self::require_admin(&env, &caller)?;

        let current_admin = env
            .storage()
            .instance()
            .get::<_, Address>(&StorageKey::WhitelistAdmin)
            .ok_or(WhitelistError::NotInitialized)?;

        if new_admin == current_admin {
            return Err(WhitelistError::NewAdminSameAsCurrent);
        }

        env.storage()
            .instance()
            .set(&StorageKey::WhitelistPendingAdmin, &new_admin);
        Self::bump_instance_ttl(&env);
        Ok(())
    }

    /// Accept a pending admin transfer (pending admin only).
    ///
    /// # Errors
    /// - [`WhitelistError::NoAdminTransferPending`] if no admin transfer has been initiated.
    pub fn accept_admin(env: Env) -> Result<(), WhitelistError> {
        let new_admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::WhitelistPendingAdmin)
            .ok_or(WhitelistError::NoAdminTransferPending)?;
        new_admin.require_auth();

        env.storage()
            .instance()
            .set(&StorageKey::WhitelistAdmin, &new_admin);
        env.storage()
            .instance()
            .remove(&StorageKey::WhitelistPendingAdmin);
        Self::bump_instance_ttl(&env);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Whitelist management
    // -----------------------------------------------------------------------

    /// Add an address to the whitelist (admin only, cooldown-gated).
    ///
    /// If the address is already present, returns
    /// [`WhitelistError::AddressAlreadyInWhitelist`].
    ///
    /// # Parameters
    /// - `caller` — Must be the current admin.
    /// - `address` — Address to add to the whitelist.
    ///
    /// # Errors
    /// - [`WhitelistError::Unauthorized`] if `caller` is not the admin.
    /// - [`WhitelistError::NotInitialized`] if the contract has not been initialized.
    /// - [`WhitelistError::AdminCooldownActive`] if another action's cool-off is still active.
    /// - [`WhitelistError::AddressAlreadyInWhitelist`] if the address is already whitelisted.
    pub fn add_address(env: Env, caller: Address, address: Address) -> Result<(), WhitelistError> {
        // `require_admin` performs `caller.require_auth()`; authorizing twice
        // in the same frame fails with `Auth, ExistingValue` on Soroban.
        Self::require_admin(&env, &caller)?;

        admin::guard(&env, Symbol::new(&env, "add_address"))?;

        let mut list = env
            .storage()
            .instance()
            .get::<_, Vec<Address>>(&StorageKey::WhitelistList)
            .unwrap_or_else(|| Vec::new(&env));

        if let Some(index) = list.iter().position(|a| a == address) {
            // An expired temporary grant is treated as absent: re-adding it
            // permanently replaces the stale entry and drops its expiry.
            if !Self::is_expired(&env, &address) {
                return Err(WhitelistError::AddressAlreadyInWhitelist);
            }
            list.remove(index as u32);
            env.storage()
                .instance()
                .remove(&StorageKey::WhitelistExpiry(address.clone()));
        }

        list.push_back(address);
        env.storage()
            .instance()
            .set(&StorageKey::WhitelistList, &list);

        Self::bump_instance_ttl(&env);
        Ok(())
    }

    /// Add an address to the whitelist until `expires_at` (admin only,
    /// cooldown-gated).
    ///
    /// The address is whitelisted strictly before `expires_at` and is no
    /// longer whitelisted at or after it. `expires_at` must be strictly in the
    /// future. Re-adding an expired address is allowed and replaces the stale
    /// entry; re-adding a live entry returns
    /// [`WhitelistError::AddressAlreadyInWhitelist`].
    ///
    /// Expired entries are omitted from [`Self::get_whitelist`] but remain in
    /// storage until re-added, removed with [`Self::remove_address`], or
    /// cleared with [`Self::clear_all`].
    ///
    /// # Parameters
    /// - `caller` — Must be the current admin.
    /// - `address` — Address to add.
    /// - `expires_at` — Ledger timestamp at which the grant lapses.
    ///
    /// # Errors
    /// - [`WhitelistError::Unauthorized`] if `caller` is not the admin.
    /// - [`WhitelistError::NotInitialized`] if the contract has not been initialized.
    /// - [`WhitelistError::InvalidExpiry`] if `expires_at <= now`.
    /// - [`WhitelistError::AdminCooldownActive`] if another action's cool-off is still active.
    /// - [`WhitelistError::AddressAlreadyInWhitelist`] if a live entry exists.
    pub fn add_address_until(
        env: Env,
        caller: Address,
        address: Address,
        expires_at: u64,
    ) -> Result<(), WhitelistError> {
        Self::require_admin(&env, &caller)?;

        if expires_at <= env.ledger().timestamp() {
            return Err(WhitelistError::InvalidExpiry);
        }

        admin::guard(&env, Symbol::new(&env, "add_address_until"))?;

        let mut list = env
            .storage()
            .instance()
            .get::<_, Vec<Address>>(&StorageKey::WhitelistList)
            .unwrap_or_else(|| Vec::new(&env));

        if let Some(index) = list.iter().position(|a| a == address) {
            if !Self::is_expired(&env, &address) {
                return Err(WhitelistError::AddressAlreadyInWhitelist);
            }
            list.remove(index as u32);
        }

        list.push_back(address.clone());
        env.storage()
            .instance()
            .set(&StorageKey::WhitelistList, &list);
        env.storage()
            .instance()
            .set(&StorageKey::WhitelistExpiry(address.clone()), &expires_at);

        Self::bump_instance_ttl(&env);
        env.events().publish(
            (
                events::event_address_added_until(&env),
                caller,
                address.clone(),
            ),
            expires_at,
        );
        Ok(())
    }

    /// Remove an address from the whitelist (admin only, cooldown-gated).
    ///
    /// # Parameters
    /// - `caller` — Must be the current admin.
    /// - `address` — Address to remove from the whitelist.
    ///
    /// # Errors
    /// - [`WhitelistError::Unauthorized`] if `caller` is not the admin.
    /// - [`WhitelistError::NotInitialized`] if the contract has not been initialized.
    /// - [`WhitelistError::AdminCooldownActive`] if another action's cool-off is still active.
    /// - [`WhitelistError::AddressNotInWhitelist`] if the address is not whitelisted.
    pub fn remove_address(
        env: Env,
        caller: Address,
        address: Address,
    ) -> Result<(), WhitelistError> {
        Self::require_admin(&env, &caller)?;

        admin::guard(&env, Symbol::new(&env, "remove_address"))?;

        let mut list = env
            .storage()
            .instance()
            .get::<_, Vec<Address>>(&StorageKey::WhitelistList)
            .ok_or(WhitelistError::AddressNotInWhitelist)?;

        let pos = list.iter().position(|a| a == address);
        match pos {
            Some(idx) => {
                list.remove(idx as u32);
            }
            None => return Err(WhitelistError::AddressNotInWhitelist),
        }

        env.storage()
            .instance()
            .remove(&StorageKey::WhitelistExpiry(address.clone()));

        if list.is_empty() {
            env.storage().instance().remove(&StorageKey::WhitelistList);
        } else {
            env.storage()
                .instance()
                .set(&StorageKey::WhitelistList, &list);
        }

        Self::bump_instance_ttl(&env);
        Ok(())
    }

    /// Remove all addresses from the whitelist (admin only, cooldown-gated).
    ///
    /// This operation is idempotent — calling it on an empty whitelist succeeds
    /// but still arms the cool-off window.
    ///
    /// # Parameters
    /// - `caller` — Must be the current admin.
    ///
    /// # Errors
    /// - [`WhitelistError::Unauthorized`] if `caller` is not the admin.
    /// - [`WhitelistError::NotInitialized`] if the contract has not been initialized.
    /// - [`WhitelistError::AdminCooldownActive`] if another action's cool-off is still active.
    pub fn clear_all(env: Env, caller: Address) -> Result<(), WhitelistError> {
        Self::require_admin(&env, &caller)?;

        admin::guard(&env, Symbol::new(&env, "clear_all"))?;

        // Remove each address's optional expiry before dropping the list so no
        // orphaned expiry entries survive a clear.
        if let Some(list) = env
            .storage()
            .instance()
            .get::<_, Vec<Address>>(&StorageKey::WhitelistList)
        {
            for address in list.iter() {
                env.storage()
                    .instance()
                    .remove(&StorageKey::WhitelistExpiry(address));
            }
        }
        env.storage().instance().remove(&StorageKey::WhitelistList);

        Self::bump_instance_ttl(&env);
        Ok(())
    }

    /// Check whether an address is in the whitelist and not expired.
    ///
    /// No authentication required — this is a public read-only view function.
    /// Entries added with `add_address` never expire. Entries added with
    /// `add_address_until(T)` are whitelisted strictly before `T` and are not
    /// whitelisted at or after `T`.
    ///
    /// # Returns
    /// `true` if the address is present and unexpired, `false` otherwise.
    pub fn is_whitelisted(env: Env, address: Address) -> bool {
        match env
            .storage()
            .instance()
            .get::<_, Vec<Address>>(&StorageKey::WhitelistList)
        {
            Some(list) => list.contains(&address) && !Self::is_expired(&env, &address),
            None => false,
        }
    }

    /// Return the current whitelist, excluding expired entries.
    ///
    /// No authentication required — this is a public read-only view function.
    /// Addresses are returned in insertion order. Expired temporary grants are
    /// filtered out (and may be re-added or removed by the admin).
    ///
    /// # Returns
    /// `Vec<Address>` containing all unexpired addresses in the whitelist.
    /// Returns an empty vector if no whitelist has been configured.
    pub fn get_whitelist(env: Env) -> Vec<Address> {
        Self::bump_instance_ttl(&env);
        let mut active = Vec::new(&env);
        if let Some(list) = env
            .storage()
            .instance()
            .get::<_, Vec<Address>>(&StorageKey::WhitelistList)
        {
            for address in list.iter() {
                if !Self::is_expired(&env, &address) {
                    active.push_back(address);
                }
            }
        }
        active
    }

    // -----------------------------------------------------------------------
    // Cooldown management
    // -----------------------------------------------------------------------

    /// Return the configured admin cool-off window in seconds.
    ///
    /// Defaults to [`admin::DEFAULT_COOLDOWN_SECONDS`] (1 hour) when no window
    /// has been explicitly set. This read-only view requires no authorization.
    pub fn get_admin_cooldown(env: Env) -> u64 {
        admin::get_cooldown(&env)
    }

    /// Configure the admin cool-off window (admin only).
    ///
    /// # Bounds
    /// - Minimum: [`admin::MIN_COOLDOWN_SECONDS`] (1 s).
    /// - Maximum: [`admin::MAX_COOLDOWN_SECONDS`] (30 d).
    /// - Default: [`admin::DEFAULT_COOLDOWN_SECONDS`] (1 h).
    ///
    /// # Authorization
    /// `caller` must be the current admin and must authorize this invocation.
    ///
    /// # Errors
    /// - [`WhitelistError::Unauthorized`] when `caller` is not the current admin.
    /// - [`WhitelistError::NotInitialized`] when the contract has no configured admin.
    /// - [`WhitelistError::InvalidAdminCooldown`] when `seconds` is outside bounds.
    pub fn set_admin_cooldown(
        env: Env,
        caller: Address,
        seconds: u64,
    ) -> Result<(), WhitelistError> {
        Self::require_admin(&env, &caller)?;
        admin::set_cooldown(&env, seconds)?;
        Self::bump_instance_ttl(&env);
        Ok(())
    }

    /// Return seconds remaining before another critical whitelist action may run.
    ///
    /// Returns `0` when no cooldown is active.
    pub fn admin_cooldown_remaining(env: Env) -> u64 {
        admin::remaining(&env)
    }

    /// Return whether a critical whitelist action may execute now.
    pub fn is_admin_action_ready(env: Env) -> bool {
        admin::is_ready(&env)
    }

    /// Return the most recently executed critical whitelist admin action, if any.
    pub fn get_last_critical_admin_action(env: Env) -> Option<admin::CriticalAdminAction> {
        admin::last_action(&env)
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Require the caller to be the current admin.
    fn require_admin(env: &Env, caller: &Address) -> Result<(), WhitelistError> {
        caller.require_auth();
        let admin = env
            .storage()
            .instance()
            .get::<_, Address>(&StorageKey::WhitelistAdmin)
            .ok_or(WhitelistError::NotInitialized)?;
        if caller != &admin {
            return Err(WhitelistError::Unauthorized);
        }
        Ok(())
    }

    /// Return `true` when `address` has a recorded expiry that has been reached.
    ///
    /// An address with no `WhitelistExpiry` entry never expires.
    fn is_expired(env: &Env, address: &Address) -> bool {
        match env
            .storage()
            .instance()
            .get::<_, u64>(&StorageKey::WhitelistExpiry(address.clone()))
        {
            Some(expires_at) => env.ledger().timestamp() >= expires_at,
            None => false,
        }
    }

    /// Extend instance storage TTL to `INSTANCE_BUMP_AMOUNT` when the remaining
    /// TTL falls below `INSTANCE_BUMP_THRESHOLD`.
    #[inline]
    pub(crate) fn bump_instance_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::testutils::Ledger as _;
    use soroban_sdk::Env;

    /// Deploy a fresh whitelist contract, init with `admin`, and return
    /// `(env, admin, client)`.
    fn deploy_whitelist<'a>(env: &'a Env, admin: &Address) -> CalloraWhitelistClient<'a> {
        let contract_id = env.register(CalloraWhitelist, ());
        let client = CalloraWhitelistClient::new(env, &contract_id);
        client.init(admin);
        client
    }

    // -----------------------------------------------------------------------
    // Init tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_init_sets_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        assert_eq!(client.get_admin(), admin);
    }

    #[test]
    fn test_init_rejects_double_init() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        let result = client.try_init(&admin);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Admin management tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_and_accept_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let new_admin = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin(&admin, &new_admin);
        client.accept_admin();
        assert_eq!(client.get_admin(), new_admin);
    }

    #[test]
    fn test_non_admin_cannot_set_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let intruder = Address::generate(&env);
        let new_admin = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        let result = client.try_set_admin(&intruder, &new_admin);
        assert!(result.is_err());
    }

    #[test]
    fn test_set_admin_same_as_current_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        let result = client.try_set_admin(&admin, &admin);
        assert_eq!(
            result.unwrap_err(),
            Ok(WhitelistError::NewAdminSameAsCurrent)
        );
    }

    #[test]
    fn test_accept_admin_without_pending_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        let result = client.try_accept_admin();
        assert_eq!(
            result.unwrap_err(),
            Ok(WhitelistError::NoAdminTransferPending)
        );
    }

    // -----------------------------------------------------------------------
    // Whitelist management tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_add_and_check_address() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);
        let addr = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &1);

        client.add_address(&admin, &addr);
        assert!(client.is_whitelisted(&addr));
    }

    #[test]
    fn test_add_duplicate_address_fails() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);
        let addr = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        // Use a short cooldown so we can test duplicates without long waits.
        client.set_admin_cooldown(&admin, &1);

        // First add — succeeds
        client.add_address(&admin, &addr);

        // Advance past the 1-second cooldown so we can call add_address again.
        env.ledger().set_timestamp(1_000_001);

        // Second add (same address) — should fail with AddressAlreadyInWhitelist
        let result = client.try_add_address(&admin, &addr);
        assert_eq!(
            result.unwrap_err(),
            Ok(WhitelistError::AddressAlreadyInWhitelist)
        );
    }

    #[test]
    fn test_remove_address() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);
        let addr = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &1);

        client.add_address(&admin, &addr);
        assert!(client.is_whitelisted(&addr));

        env.ledger().set_timestamp(1_000_001);
        client.remove_address(&admin, &addr);
        assert!(!client.is_whitelisted(&addr));
    }

    #[test]
    fn test_remove_nonexistent_address_fails() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);
        let addr = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &1);

        let result = client.try_remove_address(&admin, &addr);
        assert_eq!(
            result.unwrap_err(),
            Ok(WhitelistError::AddressNotInWhitelist)
        );
    }

    #[test]
    fn test_clear_all_removes_all_addresses() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);
        let addr1 = Address::generate(&env);
        let addr2 = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &1);

        client.add_address(&admin, &addr1);
        env.ledger().set_timestamp(1_000_001);
        client.add_address(&admin, &addr2);

        assert_eq!(client.get_whitelist().len(), 2);

        env.ledger().set_timestamp(1_000_002);
        client.clear_all(&admin);
        assert!(client.get_whitelist().is_empty());
    }

    /// Return whether the contract has a stored expiry for `address`.
    fn has_expiry(env: &Env, client: &CalloraWhitelistClient<'_>, address: &Address) -> bool {
        let contract_id = client.address.clone();
        env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .has(&StorageKey::WhitelistExpiry(address.clone()))
        })
    }

    #[test]
    fn test_add_address_until_boundaries() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);
        let addr = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &1);
        client.add_address_until(&admin, &addr, &1_000_100);

        // Strictly before T: whitelisted.
        env.ledger().set_timestamp(1_000_099);
        assert!(client.is_whitelisted(&addr));
        assert_eq!(client.get_whitelist().len(), 1);

        // Exactly at T: expired.
        env.ledger().set_timestamp(1_000_100);
        assert!(!client.is_whitelisted(&addr));
        assert!(client.get_whitelist().is_empty());
    }

    #[test]
    fn test_add_address_until_rejects_non_future_expiry() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);
        let addr = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &1);

        for expires_at in [999_999u64, 1_000_000] {
            assert_eq!(
                client.try_add_address_until(&admin, &addr, &expires_at),
                Err(Ok(WhitelistError::InvalidExpiry))
            );
        }
        assert!(!client.is_whitelisted(&addr));
        // Invalid input must not arm the cool-off window.
        assert!(client.is_admin_action_ready());
    }

    #[test]
    fn test_readd_expired_address_replaces_stale_entry() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);
        let addr = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &1);
        client.add_address_until(&admin, &addr, &1_000_010);

        env.ledger().set_timestamp(1_000_011);
        assert!(!client.is_whitelisted(&addr));

        // Re-adding an expired temporary grant is allowed and refreshes it.
        client.add_address_until(&admin, &addr, &1_000_100);
        assert!(client.is_whitelisted(&addr));
        assert_eq!(client.get_whitelist().len(), 1);

        let contract_id = client.address.clone();
        env.as_contract(&contract_id, || {
            assert_eq!(
                env.storage()
                    .instance()
                    .get::<_, u64>(&StorageKey::WhitelistExpiry(addr.clone())),
                Some(1_000_100)
            );
        });
    }

    #[test]
    fn test_add_address_permanently_after_expiry_clears_expiry() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);
        let addr = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &1);
        client.add_address_until(&admin, &addr, &1_000_010);

        env.ledger().set_timestamp(1_000_011);
        client.add_address(&admin, &addr);

        assert!(client.is_whitelisted(&addr));
        assert!(!has_expiry(&env, &client, &addr));

        // Permanent entries never expire.
        env.ledger().set_timestamp(9_000_000);
        assert!(client.is_whitelisted(&addr));
        assert_eq!(client.get_whitelist().len(), 1);
    }

    #[test]
    fn test_duplicate_live_entry_rejected_for_both_adders() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);
        let addr = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &1);
        client.add_address_until(&admin, &addr, &1_000_100);
        env.ledger().set_timestamp(1_000_001);

        assert_eq!(
            client.try_add_address(&admin, &addr),
            Err(Ok(WhitelistError::AddressAlreadyInWhitelist))
        );
        env.ledger().set_timestamp(1_000_002);
        assert_eq!(
            client.try_add_address_until(&admin, &addr, &1_000_200),
            Err(Ok(WhitelistError::AddressAlreadyInWhitelist))
        );
    }

    #[test]
    fn test_add_address_until_is_cooldown_gated() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);
        let addr1 = Address::generate(&env);
        let addr2 = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &10);
        client.add_address_until(&admin, &addr1, &1_000_100);

        assert_eq!(
            client.try_add_address_until(&admin, &addr2, &1_000_100),
            Err(Ok(WhitelistError::AdminCooldownActive))
        );

        env.ledger().set_timestamp(1_000_010);
        client.add_address_until(&admin, &addr2, &1_000_100);
        assert!(client.is_whitelisted(&addr2));
    }

    #[test]
    fn test_remove_address_deletes_expiry() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);
        let addr = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &1);
        client.add_address_until(&admin, &addr, &1_000_100);
        assert!(has_expiry(&env, &client, &addr));

        env.ledger().set_timestamp(1_000_001);
        client.remove_address(&admin, &addr);
        assert!(!client.is_whitelisted(&addr));
        assert!(!has_expiry(&env, &client, &addr));
    }

    #[test]
    fn test_clear_all_deletes_all_expiries() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);
        let addr1 = Address::generate(&env);
        let addr2 = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &1);
        client.add_address_until(&admin, &addr1, &1_000_100);
        env.ledger().set_timestamp(1_000_001);
        client.add_address_until(&admin, &addr2, &1_000_200);

        env.ledger().set_timestamp(1_000_002);
        client.clear_all(&admin);

        assert!(client.get_whitelist().is_empty());
        assert!(!has_expiry(&env, &client, &addr1));
        assert!(!has_expiry(&env, &client, &addr2));
    }

    #[test]
    fn test_get_whitelist_filters_expired_entries() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);
        let permanent = Address::generate(&env);
        let temporary = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &1);
        client.add_address(&admin, &permanent);
        env.ledger().set_timestamp(1_000_001);
        client.add_address_until(&admin, &temporary, &1_000_100);

        env.ledger().set_timestamp(1_000_100);
        let list = client.get_whitelist();
        assert_eq!(list.len(), 1);
        assert_eq!(list.get(0).unwrap(), permanent);
        assert!(!client.is_whitelisted(&temporary));
    }

    #[test]
    fn test_is_whitelisted_returns_false_for_uninit() {
        let env = Env::default();
        let addr = Address::generate(&env);

        let contract_id = env.register(CalloraWhitelist, ());
        let client = CalloraWhitelistClient::new(&env, &contract_id);

        // Before init, whitelist should be empty
        assert!(client.get_whitelist().is_empty());
        assert!(!client.is_whitelisted(&addr));
    }

    #[test]
    fn test_non_admin_cannot_add_address() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let intruder = Address::generate(&env);
        let addr = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        let result = client.try_add_address(&intruder, &addr);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Cooldown integration tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_cooldown_blocks_consecutive_actions() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);

        let admin = Address::generate(&env);
        let addr1 = Address::generate(&env);
        let addr2 = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &300);

        // First action succeeds
        client.add_address(&admin, &addr1);
        assert!(client.is_whitelisted(&addr1));

        // Second action blocked by cooldown
        let result = client.try_add_address(&admin, &addr2);
        assert_eq!(result.unwrap_err(), Ok(WhitelistError::AdminCooldownActive));

        // Advance past cooldown window
        env.ledger().set_timestamp(1_000_300);
        assert!(client.is_admin_action_ready());

        // Second action now succeeds
        client.add_address(&admin, &addr2);
        assert!(client.is_whitelisted(&addr2));
    }

    #[test]
    fn test_remove_address_is_cooldown_gated() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);

        let admin = Address::generate(&env);
        let addr = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &300);

        // Add an address first
        client.add_address(&admin, &addr);

        // Advance past cooldown before removing
        env.ledger().set_timestamp(1_000_300);
        client.remove_address(&admin, &addr);
        assert!(!client.is_whitelisted(&addr));

        // remove_address just armed cooldown — another remove on non-existent
        // address should be blocked by cooldown, not by "not found"
        let result = client.try_remove_address(&admin, &addr);
        assert_eq!(result.unwrap_err(), Ok(WhitelistError::AdminCooldownActive));
    }

    #[test]
    fn test_clear_all_is_cooldown_gated() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);

        let admin = Address::generate(&env);
        let addr1 = Address::generate(&env);
        let addr2 = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &300);

        client.add_address(&admin, &addr1);
        env.ledger().set_timestamp(1_000_300);
        client.add_address(&admin, &addr2);

        // Clear blocked by cooldown (just after second add)
        let result = client.try_clear_all(&admin);
        assert_eq!(result.unwrap_err(), Ok(WhitelistError::AdminCooldownActive));

        env.ledger().set_timestamp(1_000_600);
        assert!(client.is_admin_action_ready());

        client.clear_all(&admin);
        assert!(client.get_whitelist().is_empty());
    }

    #[test]
    fn test_cooldown_configuration() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);

        // Default cooldown
        assert_eq!(client.get_admin_cooldown(), admin::DEFAULT_COOLDOWN_SECONDS);

        // Set to minimum
        client.set_admin_cooldown(&admin, &admin::MIN_COOLDOWN_SECONDS);
        assert_eq!(client.get_admin_cooldown(), admin::MIN_COOLDOWN_SECONDS);

        // Set to maximum
        client.set_admin_cooldown(&admin, &admin::MAX_COOLDOWN_SECONDS);
        assert_eq!(client.get_admin_cooldown(), admin::MAX_COOLDOWN_SECONDS);

        // Out of bounds
        let result = client.try_set_admin_cooldown(&admin, &0);
        assert_eq!(
            result.unwrap_err(),
            Ok(WhitelistError::InvalidAdminCooldown)
        );

        let result = client.try_set_admin_cooldown(&admin, &(admin::MAX_COOLDOWN_SECONDS + 1));
        assert_eq!(
            result.unwrap_err(),
            Ok(WhitelistError::InvalidAdminCooldown)
        );
    }

    #[test]
    fn test_cooldown_remaining() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);

        let admin = Address::generate(&env);
        let addr = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &300);

        // No action yet, cooldown should be 0
        assert_eq!(client.admin_cooldown_remaining(), 0);

        // Execute action
        client.add_address(&admin, &addr);
        assert_eq!(client.admin_cooldown_remaining(), 300);

        // Advance partway
        env.ledger().set_timestamp(1_000_100);
        assert_eq!(client.admin_cooldown_remaining(), 200);

        // Advance past window
        env.ledger().set_timestamp(1_000_300);
        assert_eq!(client.admin_cooldown_remaining(), 0);
    }

    #[test]
    fn test_get_last_critical_admin_action() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);

        let admin = Address::generate(&env);
        let addr = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &300);

        // No action yet
        assert!(client.get_last_critical_admin_action().is_none());

        client.add_address(&admin, &addr);
        let record = client.get_last_critical_admin_action().unwrap();
        assert_eq!(record.action, Symbol::new(&env, "add_address"));
        assert_eq!(record.executed_at, 1_000_000);

        env.ledger().set_timestamp(1_000_300);
        client.remove_address(&admin, &addr);
        let record = client.get_last_critical_admin_action().unwrap();
        assert_eq!(record.action, Symbol::new(&env, "remove_address"));
    }

    #[test]
    fn test_is_admin_action_ready() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);

        let admin = Address::generate(&env);
        let addr = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &300);

        // No action yet — ready
        assert!(client.is_admin_action_ready());

        client.add_address(&admin, &addr);
        assert!(!client.is_admin_action_ready());

        env.ledger().set_timestamp(1_000_300);
        assert!(client.is_admin_action_ready());
    }

    #[test]
    fn test_error_code_stability() {
        assert_eq!(WhitelistError::NotInitialized as u32, 1);
        assert_eq!(WhitelistError::AlreadyInitialized as u32, 2);
        assert_eq!(WhitelistError::Unauthorized as u32, 3);
        assert_eq!(WhitelistError::AddressAlreadyInWhitelist as u32, 4);
        assert_eq!(WhitelistError::AddressNotInWhitelist as u32, 5);

        assert_eq!(WhitelistError::AdminCooldownActive as u32, 49);
        assert_eq!(WhitelistError::InvalidAdminCooldown as u32, 50);
        assert_eq!(WhitelistError::NoAdminTransferPending as u32, 51);
        assert_eq!(WhitelistError::NewAdminSameAsCurrent as u32, 52);
        assert_eq!(WhitelistError::InvalidExpiry as u32, 53);
    }
}

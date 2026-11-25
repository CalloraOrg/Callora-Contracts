#![no_std]
//! Callora emergency pause coordinator.
//!
//! The coordinator owns a bounded, admin-managed registry of pausable Callora
//! contracts plus an optional guardian role. `pause_all` calls `pause` on every
//! registered target and reports per-contract failures without blocking the
//! remaining targets; `unpause_all` mirrors it and is admin-only.
//!
//! The coordinator's own contract address must be configured as the admin (or
//! pause guardian, where the target supports one) on every registered target.
//! Otherwise that target rejects the call and appears in the failure list.
//!
//! The crate also keeps the read-only capability bitmap so clients can detect
//! which emergency features a deployment supports.

pub mod errors;
pub mod events;
pub mod migrate;
mod views;

pub use errors::EmergencyError;
pub use views::{
    capabilities, ALL_CAPABILITIES, CAP_CONTRACT_REGISTRY, CAP_EMERGENCY_DRAIN_CANCEL,
    CAP_EMERGENCY_DRAIN_EXECUTE, CAP_EMERGENCY_DRAIN_PROPOSE, CAP_EMERGENCY_PAUSE,
    CAP_EMERGENCY_UNPAUSE, CAP_GUARDIAN_ROLE, CAP_PAUSE_COORDINATOR, CAP_PENDING_DRAIN_VIEW,
};

use soroban_sdk::{contract, contractclient, contractimpl, contracttype, Address, Env, Vec};

/// Maximum number of contracts the coordinator will pause in one call.
///
/// Keeping the registry bounded makes `pause_all` resource usage predictable.
pub const MAX_REGISTRY_SIZE: u32 = 20;

const LEDGERS_PER_DAY: u32 = 17_280;
const INSTANCE_BUMP_THRESHOLD: u32 = LEDGERS_PER_DAY * 30;
const INSTANCE_BUMP_AMOUNT: u32 = LEDGERS_PER_DAY * 60;

/// Storage keys for the coordinator.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum DataKey {
    Admin,
    PendingAdmin,
    Guardian,
    /// Instance: ordered list of registered pausable contracts.
    Registry,
    /// Instance: report from the most recent `pause_all` / `unpause_all`.
    LastReport,
}

/// Result of a `pause_all` or `unpause_all` run.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PauseReport {
    /// Address that triggered the run (admin or guardian).
    pub caller: Address,
    /// Number of targets that accepted the call.
    pub succeeded: u32,
    /// Targets that rejected the call or could not be invoked.
    pub failed: Vec<Address>,
    /// `true` for `pause_all`, `false` for `unpause_all`.
    pub paused: bool,
    /// Ledger sequence of the run.
    pub ledger: u32,
    /// Ledger timestamp of the run.
    pub timestamp: u64,
}

/// Minimal interface implemented by pausable Callora contracts.
#[contractclient(name = "PausableClient")]
pub trait Pausable {
    fn pause(env: Env, caller: Address);
    fn unpause(env: Env, caller: Address);
}

#[contract]
pub struct CalloraEmergency;

#[contractimpl]
impl CalloraEmergency {
    fn bump_instance(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    fn admin(env: &Env) -> Result<Address, EmergencyError> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(EmergencyError::NotInitialized)
    }

    fn guardian(env: &Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::Guardian)
    }

    fn require_admin(env: &Env, caller: &Address) -> Result<Address, EmergencyError> {
        let admin = Self::admin(env)?;
        if caller != &admin {
            return Err(EmergencyError::Unauthorized);
        }
        Ok(admin)
    }

    fn require_admin_or_guardian(env: &Env, caller: &Address) -> Result<(), EmergencyError> {
        let admin = Self::admin(env)?;
        if caller == &admin {
            return Ok(());
        }
        if let Some(guardian) = Self::guardian(env) {
            if caller == &guardian {
                return Ok(());
            }
        }
        Err(EmergencyError::Unauthorized)
    }

    fn registry(env: &Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::Registry)
            .unwrap_or_else(|| Vec::new(env))
    }

    fn set_registry(env: &Env, registry: &Vec<Address>) {
        env.storage().instance().set(&DataKey::Registry, registry);
    }

    fn registry_position(registry: &Vec<Address>, target: &Address) -> Option<u32> {
        for i in 0..registry.len() {
            if &registry.get_unchecked(i) == target {
                return Some(i);
            }
        }
        None
    }

    fn store_report(
        env: &Env,
        caller: &Address,
        succeeded: u32,
        failed: Vec<Address>,
        paused: bool,
    ) -> PauseReport {
        let report = PauseReport {
            caller: caller.clone(),
            succeeded,
            failed,
            paused,
            ledger: env.ledger().sequence(),
            timestamp: env.ledger().timestamp(),
        };
        env.storage().instance().set(&DataKey::LastReport, &report);
        report
    }

    /// Initialise the coordinator with `admin`.
    pub fn init(env: Env, admin: Address) -> Result<(), EmergencyError> {
        admin.require_auth();
        Self::bump_instance(&env);
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(EmergencyError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::Registry, &Vec::<Address>::new(&env));
        events::emit_initialized(&env, &admin);
        Ok(())
    }

    /// Nominate `new_admin` as the next admin (two-step transfer).
    pub fn set_admin(
        env: Env,
        caller: Address,
        new_admin: Address,
    ) -> Result<(), EmergencyError> {
        caller.require_auth();
        Self::bump_instance(&env);
        let admin = Self::require_admin(&env, &caller)?;
        if new_admin == admin {
            return Err(EmergencyError::InvalidAdmin);
        }
        env.storage()
            .instance()
            .set(&DataKey::PendingAdmin, &new_admin);
        events::emit_admin_nominated(&env, &admin, &new_admin);
        Ok(())
    }

    /// Accept a pending admin nomination.
    pub fn accept_admin(env: Env, new_admin: Address) -> Result<(), EmergencyError> {
        new_admin.require_auth();
        Self::bump_instance(&env);
        let pending: Address = env
            .storage()
            .instance()
            .get(&DataKey::PendingAdmin)
            .ok_or(EmergencyError::NoPendingAdmin)?;
        if new_admin != pending {
            return Err(EmergencyError::Unauthorized);
        }
        let old_admin = Self::admin(&env)?;
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        env.storage().instance().remove(&DataKey::PendingAdmin);
        events::emit_admin_accepted(&env, &old_admin, &new_admin);
        Ok(())
    }

    /// Cancel a pending admin nomination.
    pub fn cancel_admin_transfer(env: Env, caller: Address) -> Result<(), EmergencyError> {
        caller.require_auth();
        Self::bump_instance(&env);
        let admin = Self::require_admin(&env, &caller)?;
        if !env.storage().instance().has(&DataKey::PendingAdmin) {
            return Err(EmergencyError::NoPendingAdmin);
        }
        env.storage().instance().remove(&DataKey::PendingAdmin);
        events::emit_admin_cancelled(&env, &admin);
        Ok(())
    }

    /// Set the guardian allowed to trigger `pause_all` (admin only).
    pub fn set_guardian(
        env: Env,
        caller: Address,
        guardian: Address,
    ) -> Result<(), EmergencyError> {
        caller.require_auth();
        Self::bump_instance(&env);
        let admin = Self::require_admin(&env, &caller)?;
        if guardian == env.current_contract_address() {
            return Err(EmergencyError::InvalidTarget);
        }
        env.storage().instance().set(&DataKey::Guardian, &guardian);
        events::emit_guardian_set(&env, &admin, &guardian);
        Ok(())
    }

    /// Remove the guardian role (admin only).
    pub fn clear_guardian(env: Env, caller: Address) -> Result<(), EmergencyError> {
        caller.require_auth();
        Self::bump_instance(&env);
        let admin = Self::require_admin(&env, &caller)?;
        if env.storage().instance().has(&DataKey::Guardian) {
            env.storage().instance().remove(&DataKey::Guardian);
            events::emit_guardian_cleared(&env, &admin);
        }
        Ok(())
    }

    /// Register a pausable contract (admin only).
    pub fn register_contract(
        env: Env,
        caller: Address,
        target: Address,
    ) -> Result<(), EmergencyError> {
        caller.require_auth();
        Self::bump_instance(&env);
        let admin = Self::require_admin(&env, &caller)?;
        if target == env.current_contract_address() {
            return Err(EmergencyError::InvalidTarget);
        }
        let mut registry = Self::registry(&env);
        if Self::registry_position(&registry, &target).is_some() {
            return Err(EmergencyError::AlreadyRegistered);
        }
        if registry.len() >= MAX_REGISTRY_SIZE {
            return Err(EmergencyError::RegistryFull);
        }
        registry.push_back(target.clone());
        Self::set_registry(&env, &registry);
        events::emit_contract_registered(&env, &admin, &target);
        Ok(())
    }

    /// Remove a pausable contract from the registry (admin only).
    pub fn deregister_contract(
        env: Env,
        caller: Address,
        target: Address,
    ) -> Result<(), EmergencyError> {
        caller.require_auth();
        Self::bump_instance(&env);
        let admin = Self::require_admin(&env, &caller)?;
        let mut registry = Self::registry(&env);
        let position =
            Self::registry_position(&registry, &target).ok_or(EmergencyError::NotRegistered)?;
        registry.remove(position);
        Self::set_registry(&env, &registry);
        events::emit_contract_deregistered(&env, &admin, &target);
        Ok(())
    }

    /// Return a page of registered contracts.
    pub fn get_registered(env: Env, cursor: u32, limit: u32) -> Vec<Address> {
        Self::bump_instance(&env);
        let registry = Self::registry(&env);
        let effective_limit = if limit == 0 || limit > MAX_REGISTRY_SIZE {
            MAX_REGISTRY_SIZE
        } else {
            limit
        };
        let mut page = Vec::new(&env);
        if cursor >= registry.len() {
            return page;
        }
        let end = cursor
            .saturating_add(effective_limit)
            .min(registry.len());
        for i in cursor..end {
            page.push_back(registry.get_unchecked(i));
        }
        page
    }

    /// Return whether `target` is registered.
    pub fn is_registered(env: Env, target: Address) -> bool {
        Self::bump_instance(&env);
        Self::registry_position(&Self::registry(&env), &target).is_some()
    }

    /// Return the number of registered contracts.
    pub fn get_registry_size(env: Env) -> u32 {
        Self::bump_instance(&env);
        Self::registry(&env).len()
    }

    /// Return the admin address.
    pub fn get_admin(env: Env) -> Result<Address, EmergencyError> {
        Self::bump_instance(&env);
        Self::admin(&env)
    }

    /// Return the pending admin nomination, if any.
    pub fn get_pending_admin(env: Env) -> Option<Address> {
        Self::bump_instance(&env);
        env.storage().instance().get(&DataKey::PendingAdmin)
    }

    /// Return the guardian, if configured.
    pub fn get_guardian(env: Env) -> Option<Address> {
        Self::bump_instance(&env);
        Self::guardian(&env)
    }

    /// Pause every registered contract.
    ///
    /// Callable by the admin or guardian. A target that rejects the call (for
    /// example, because it is already paused or does not recognise the
    /// coordinator as admin) is recorded in the report and does not stop the
    /// remaining targets. Returns the number of contracts paused.
    pub fn pause_all(env: Env, caller: Address) -> Result<u32, EmergencyError> {
        caller.require_auth();
        Self::bump_instance(&env);
        Self::require_admin_or_guardian(&env, &caller)?;

        let coordinator = env.current_contract_address();
        let registry = Self::registry(&env);
        let mut paused = 0u32;
        let mut failed = Vec::new(&env);
        for target in registry.iter() {
            let result = PausableClient::new(&env, &target).try_pause(&coordinator);
            if result.is_ok() {
                paused += 1;
            } else {
                failed.push_back(target);
            }
        }

        let report = Self::store_report(&env, &caller, paused, failed, true);
        events::emit_pause_all_executed(&env, &caller, &report);
        Ok(paused)
    }

    /// Unpause every registered contract (admin only).
    ///
    /// Mirrors [`Self::pause_all`]: failures are reported and do not stop the
    /// remaining targets. Returns the number of contracts unpaused.
    pub fn unpause_all(env: Env, caller: Address) -> Result<u32, EmergencyError> {
        caller.require_auth();
        Self::bump_instance(&env);
        Self::require_admin(&env, &caller)?;

        let coordinator = env.current_contract_address();
        let registry = Self::registry(&env);
        let mut unpaused = 0u32;
        let mut failed = Vec::new(&env);
        for target in registry.iter() {
            let result = PausableClient::new(&env, &target).try_unpause(&coordinator);
            if result.is_ok() {
                unpaused += 1;
            } else {
                failed.push_back(target);
            }
        }

        let report = Self::store_report(&env, &caller, unpaused, failed, false);
        events::emit_unpause_all_executed(&env, &caller, &report);
        Ok(unpaused)
    }

    /// Return the report from the most recent `pause_all` / `unpause_all`.
    pub fn get_last_pause_report(env: Env) -> Option<PauseReport> {
        Self::bump_instance(&env);
        env.storage().instance().get(&DataKey::LastReport)
    }

    /// Return the emergency-feature capability bitmap for this deployment.
    ///
    /// Each set bit signals a supported emergency feature. Bits are stable
    /// across upgrades — once assigned a bit position is never reused.
    pub fn capabilities(env: Env) -> u64 {
        views::capabilities(&env)
    }
}

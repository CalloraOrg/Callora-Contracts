#![no_std]

pub mod errors;

use crate::errors::UpgradeError;
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, BytesN, Env, Symbol,
};

/// Storage keys for the upgrade contract.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Contract admin address.
    Admin,
    /// Pending upgrade proposal information.
    PendingUpgrade,
    /// Current WASM version hash.
    WasmHash,
    /// Contract version number.
    Version,
}

/// Represents a pending upgrade proposal.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingUpgradeProposal {
    /// New target WASM hash.
    pub new_wasm_hash: BytesN<32>,
    /// Timestamp when the upgrade can be executed.
    pub unlock_timestamp: u64,
}

#[contract]
pub struct CalloraUpgrade;

#[contractimpl]
impl CalloraUpgrade {
    /// Initializes the upgrade contract with an admin address.
    ///
    /// # Arguments
    /// * `env` - Soroban execution environment.
    /// * `admin` - Address of the contract administrator.
    ///
    /// # Errors
    /// * `AlreadyInitialized` - If the contract has already been initialized.
    pub fn init(env: Env, admin: Address) -> Result<(), UpgradeError> {
        admin.require_auth();

        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(UpgradeError::AlreadyInitialized);
        }

        env.storage().instance().set(&StorageKey::Admin, &admin);
        env.storage().instance().set(&StorageKey::Version, &1_u32);

        env.events().publish(
            (symbol_short!("init"), admin),
            1_u32,
        );

        Ok(())
    }

    /// Proposes a WASM contract upgrade with a timelock delay.
    ///
    /// # Arguments
    /// * `env` - Soroban execution environment.
    /// * `caller` - Admin address initiating the proposal.
    /// * `new_wasm_hash` - The 32-byte hash of the new WASM binary.
    /// * `delay_seconds` - Minimum delay in seconds before the upgrade can be executed.
    ///
    /// # Errors
    /// * `NotInitialized` - If contract is not initialized.
    /// * `Unauthorized` - If caller is not the admin.
    /// * `MigrationPending` - If an upgrade is already pending.
    /// * `SameWasmHash` - If new_wasm_hash matches the currently stored WASM hash.
    /// * `Overflow` - If unlock timestamp calculation overflows.
    pub fn propose_upgrade(
        env: Env,
        caller: Address,
        new_wasm_hash: BytesN<32>,
        delay_seconds: u64,
    ) -> Result<(), UpgradeError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;

        if env.storage().instance().has(&StorageKey::PendingUpgrade) {
            return Err(UpgradeError::MigrationPending);
        }

        if let Some(current_hash) = Self::get_wasm_hash(env.clone()) {
            if current_hash == new_wasm_hash {
                return Err(UpgradeError::SameWasmHash);
            }
        }

        let now = env.ledger().timestamp();
        let unlock_timestamp = now
            .checked_add(delay_seconds)
            .ok_or(UpgradeError::Overflow)?;

        let proposal = PendingUpgradeProposal {
            new_wasm_hash: new_wasm_hash.clone(),
            unlock_timestamp,
        };

        env.storage()
            .instance()
            .set(&StorageKey::PendingUpgrade, &proposal);

        env.events().publish(
            (Symbol::new(&env, "prop_upg"), caller),
            (new_wasm_hash, unlock_timestamp),
        );

        Ok(())
    }

    /// Executes a pending WASM contract upgrade after the timelock expires.
    ///
    /// # Arguments
    /// * `env` - Soroban execution environment.
    /// * `caller` - Admin address executing the upgrade.
    ///
    /// # Errors
    /// * `NotInitialized` - If contract is not initialized.
    /// * `Unauthorized` - If caller is not the admin.
    /// * `NoUpgradePending` - If no upgrade proposal is pending.
    /// * `TimelockNotExpired` - If unlock timestamp has not been reached.
    /// * `Overflow` - If version increment overflows.
    pub fn execute_upgrade(env: Env, caller: Address) -> Result<(), UpgradeError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;

        let proposal: PendingUpgradeProposal = env
            .storage()
            .instance()
            .get(&StorageKey::PendingUpgrade)
            .ok_or(UpgradeError::NoUpgradePending)?;

        let now = env.ledger().timestamp();
        if now < proposal.unlock_timestamp {
            return Err(UpgradeError::TimelockNotExpired);
        }

        // Apply WASM update via deployer interface
        env.deployer()
            .update_current_contract_wasm(proposal.new_wasm_hash.clone());

        let current_version: u32 = env
            .storage()
            .instance()
            .get(&StorageKey::Version)
            .unwrap_or(1);

        let new_version = current_version
            .checked_add(1)
            .ok_or(UpgradeError::Overflow)?;

        env.storage()
            .instance()
            .set(&StorageKey::WasmHash, &proposal.new_wasm_hash);
        env.storage()
            .instance()
            .set(&StorageKey::Version, &new_version);
        env.storage().instance().remove(&StorageKey::PendingUpgrade);

        env.events().publish(
            (Symbol::new(&env, "upgraded"), caller),
            (proposal.new_wasm_hash, new_version),
        );

        Ok(())
    }

    /// Cancels a pending WASM contract upgrade.
    ///
    /// # Arguments
    /// * `env` - Soroban execution environment.
    /// * `caller` - Admin address cancelling the upgrade.
    ///
    /// # Errors
    /// * `NotInitialized` - If contract is not initialized.
    /// * `Unauthorized` - If caller is not the admin.
    /// * `NoUpgradePending` - If no upgrade proposal exists.
    pub fn cancel_upgrade(env: Env, caller: Address) -> Result<(), UpgradeError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;

        if !env.storage().instance().has(&StorageKey::PendingUpgrade) {
            return Err(UpgradeError::NoUpgradePending);
        }

        env.storage().instance().remove(&StorageKey::PendingUpgrade);

        env.events().publish(
            (Symbol::new(&env, "upg_canc"), caller),
            (),
        );

        Ok(())
    }

    /// Returns the current admin address.
    pub fn get_admin(env: Env) -> Result<Address, UpgradeError> {
        env.storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(UpgradeError::NotInitialized)
    }

    /// Returns the current version number.
    pub fn get_version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&StorageKey::Version)
            .unwrap_or(0)
    }

    /// Returns the currently stored WASM hash, if any.
    pub fn get_wasm_hash(env: Env) -> Option<BytesN<32>> {
        env.storage()
            .instance()
            .get(&StorageKey::WasmHash)
    }

    /// Returns the pending upgrade proposal, if any.
    pub fn get_pending_upgrade(env: Env) -> Option<PendingUpgradeProposal> {
        env.storage()
            .instance()
            .get(&StorageKey::PendingUpgrade)
    }

    /// Internal helper to verify that the caller is the configured admin.
    fn require_admin(env: &Env, caller: &Address) -> Result<(), UpgradeError> {
        let admin = Self::get_admin(env.clone())?;
        if caller != &admin {
            return Err(UpgradeError::Unauthorized);
        }
        Ok(())
    }
}

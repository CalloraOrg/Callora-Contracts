#![no_std]

mod errors;
mod types;

#[cfg(any(test))]
mod test;
#[cfg(any(test))]
mod test_ttl_bump;

#[cfg(test)]
use soroban_sdk::testutils::storage::{Instance as _, Persistent as _};
use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};

pub use errors::RefundError;
pub use types::*;

/// Minimum threshold of remaining ledgers before instance storage TTL is extended (~30 days).
pub const INSTANCE_BUMP_THRESHOLD: u32 = 17_280 * 30;

/// Number of ledgers to extend instance storage TTL by (~60 days).
pub const INSTANCE_BUMP_AMOUNT: u32 = 17_280 * 60;

/// Minimum threshold of remaining ledgers before persistent storage TTL is extended.
pub const PERSISTENT_BUMP_THRESHOLD: u32 = 50_000;

/// Number of ledgers to extend persistent storage TTL by.
pub const PERSISTENT_BUMP_AMOUNT: u32 = 50_000;

#[contract]
pub struct RefundContract;

#[contractimpl]
impl RefundContract {
    /// Initialize the refund contract.
    ///
    /// # Arguments
    /// * `admin` - Address that will hold admin privileges; must authorize.
    /// * `fee_bps` - Fee rate in basis points (0-10000).
    /// * `min_refund_amount` - Minimum refund amount allowed (>= 0).
    ///
    /// # Errors
    /// * `AlreadyInitialized` - Contract already initialized.
    /// * `FeeTooHigh` - `fee_bps` exceeds 10000.
    /// * `InvalidAmount` - `min_refund_amount` is negative.
    pub fn init(
        env: Env,
        admin: Address,
        fee_bps: u32,
        min_refund_amount: i128,
    ) -> Result<(), RefundError> {
        admin.require_auth();

        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(RefundError::AlreadyInitialized);
        }
        if fee_bps > 10_000 {
            return Err(RefundError::FeeTooHigh);
        }
        if min_refund_amount < 0 {
            return Err(RefundError::InvalidAmount);
        }

        let inst = env.storage().instance();
        inst.set(&StorageKey::Admin, &admin);
        inst.set(&StorageKey::RefundCounter, &0u64);
        inst.set(&StorageKey::TotalRefunds, &0i128);
        inst.set(
            &StorageKey::RefundConfig,
            &RefundConfig {
                fee_bps,
                min_refund_amount,
            },
        );

        Self::emit_initialized(&env, &admin, fee_bps, min_refund_amount);

        Ok(())
    }

    /// Request a refund.
    ///
    /// # Arguments
    /// * `requester` - Address requesting the refund; must authorize.
    /// * `token` - Token contract address for the refund.
    /// * `amount` - Amount to refund; must be > 0 and >= min_refund_amount.
    /// * `reason` - Reason for the refund request.
    ///
    /// # Returns
    /// The unique request ID assigned to this refund request.
    ///
    /// # Errors
    /// * `NotInitialized` - Contract not initialized.
    /// * `InvalidAmount` - Amount is not positive.
    /// * `AmountTooLow` - Amount is below minimum refund amount.
    /// * `Overflow` - Refund counter would overflow.
    pub fn request_refund(
        env: Env,
        requester: Address,
        token: Address,
        amount: i128,
        reason: Symbol,
    ) -> Result<u64, RefundError> {
        requester.require_auth();
        Self::ensure_initialized(&env)?;

        if amount <= 0 {
            return Err(RefundError::InvalidAmount);
        }

        let config: RefundConfig = env
            .storage()
            .instance()
            .get(&StorageKey::RefundConfig)
            .ok_or(RefundError::NotInitialized)?;
        if amount < config.min_refund_amount {
            return Err(RefundError::AmountTooLow);
        }

        let counter: u64 = env
            .storage()
            .instance()
            .get(&StorageKey::RefundCounter)
            .unwrap_or(0);
        let request_id = counter
            .checked_add(1)
            .ok_or(RefundError::Overflow)?;

        let request = RefundRequest {
            id: request_id,
            requester: requester.clone(),
            token: token.clone(),
            amount,
            reason: reason.clone(),
            status: RefundStatus::Pending,
            created_at: env.ledger().timestamp(),
            processed_at: None,
        };

        let key = StorageKey::PendingRefund(request_id);
        env.storage().persistent().set(&key, &request);
        env.storage()
            .persistent()
            .extend_ttl(&key, PERSISTENT_BUMP_THRESHOLD, PERSISTENT_BUMP_AMOUNT);

        env.storage()
            .instance()
            .set(&StorageKey::RefundCounter, &request_id);

        Self::emit_refund_requested(&env, request_id, &requester, &token, amount, &reason);

        Ok(request_id)
    }

    /// Approve a pending refund request (admin only).
    ///
    /// # Arguments
    /// * `admin` - Admin address; must authorize and match stored admin.
    /// * `request_id` - ID of the refund request to approve.
    ///
    /// # Errors
    /// * `NotInitialized` - Contract not initialized.
    /// * `Unauthorized` - Caller is not the admin.
    /// * `NotFound` - Refund request not found.
    /// * `InvalidStatus` - Request is not in Pending status.
    pub fn approve_refund(
        env: Env,
        admin: Address,
        request_id: u64,
    ) -> Result<(), RefundError> {
        admin.require_auth();
        Self::ensure_initialized(&env)?;

        let stored_admin: Address = Self::get_admin_internal(&env)?;
        if admin != stored_admin {
            return Err(RefundError::Unauthorized);
        }

        let key = StorageKey::PendingRefund(request_id);
        let mut request: RefundRequest = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(RefundError::NotFound)?;

        if request.status != RefundStatus::Pending {
            return Err(RefundError::InvalidStatus);
        }

        request.status = RefundStatus::Approved;
        env.storage().persistent().set(&key, &request);
        env.storage()
            .persistent()
            .extend_ttl(&key, PERSISTENT_BUMP_THRESHOLD, PERSISTENT_BUMP_AMOUNT);

        Self::emit_refund_processed(
            &env,
            request_id,
            &admin,
            request.amount,
            RefundStatus::Approved,
        );

        Ok(())
    }

    /// Reject a pending refund request (admin only).
    ///
    /// # Arguments
    /// * `admin` - Admin address; must authorize and match stored admin.
    /// * `request_id` - ID of the refund request to reject.
    ///
    /// # Errors
    /// * `NotInitialized` - Contract not initialized.
    /// * `Unauthorized` - Caller is not the admin.
    /// * `NotFound` - Refund request not found.
    /// * `InvalidStatus` - Request is not in Pending status.
    pub fn reject_refund(
        env: Env,
        admin: Address,
        request_id: u64,
    ) -> Result<(), RefundError> {
        admin.require_auth();
        Self::ensure_initialized(&env)?;

        let stored_admin: Address = Self::get_admin_internal(&env)?;
        if admin != stored_admin {
            return Err(RefundError::Unauthorized);
        }

        let key = StorageKey::PendingRefund(request_id);
        let mut request: RefundRequest = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(RefundError::NotFound)?;

        if request.status != RefundStatus::Pending {
            return Err(RefundError::InvalidStatus);
        }

        request.status = RefundStatus::Rejected;
        request.processed_at = Some(env.ledger().timestamp());
        env.storage().persistent().set(&key, &request);
        env.storage()
            .persistent()
            .extend_ttl(&key, PERSISTENT_BUMP_THRESHOLD, PERSISTENT_BUMP_AMOUNT);

        Self::emit_refund_processed(
            &env,
            request_id,
            &admin,
            request.amount,
            RefundStatus::Rejected,
        );

        Ok(())
    }

    /// Process an approved refund request (admin only).
    ///
    /// This marks the refund as processed and updates the total refunds counter.
    ///
    /// # Arguments
    /// * `admin` - Admin address; must authorize and match stored admin.
    /// * `request_id` - ID of the refund request to process.
    ///
    /// # Errors
    /// * `NotInitialized` - Contract not initialized.
    /// * `Unauthorized` - Caller is not the admin.
    /// * `NotFound` - Refund request not found.
    /// * `InvalidStatus` - Request is not in Approved status.
    /// * `Overflow` - Total refunds counter would overflow.
    pub fn process_refund(
        env: Env,
        admin: Address,
        request_id: u64,
    ) -> Result<(), RefundError> {
        admin.require_auth();
        Self::ensure_initialized(&env)?;

        let stored_admin: Address = Self::get_admin_internal(&env)?;
        if admin != stored_admin {
            return Err(RefundError::Unauthorized);
        }

        let key = StorageKey::PendingRefund(request_id);
        let mut request: RefundRequest = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(RefundError::NotFound)?;

        if request.status != RefundStatus::Approved {
            return Err(RefundError::InvalidStatus);
        }

        request.status = RefundStatus::Processed;
        request.processed_at = Some(env.ledger().timestamp());
        env.storage().persistent().set(&key, &request);
        env.storage()
            .persistent()
            .extend_ttl(&key, PERSISTENT_BUMP_THRESHOLD, PERSISTENT_BUMP_AMOUNT);

        let total: i128 = env
            .storage()
            .instance()
            .get(&StorageKey::TotalRefunds)
            .unwrap_or(0);
        let new_total = total
            .checked_add(request.amount)
            .ok_or(RefundError::Overflow)?;
        env.storage()
            .instance()
            .set(&StorageKey::TotalRefunds, &new_total);

        Self::emit_refund_processed(
            &env,
            request_id,
            &admin,
            request.amount,
            RefundStatus::Processed,
        );

        Ok(())
    }

    /// Update refund configuration (admin only).
    ///
    /// # Arguments
    /// * `admin` - Admin address; must authorize and match stored admin.
    /// * `fee_bps` - New fee rate in basis points (0-10000).
    /// * `min_refund_amount` - New minimum refund amount (>= 0).
    ///
    /// # Errors
    /// * `NotInitialized` - Contract not initialized.
    /// * `Unauthorized` - Caller is not the admin.
    /// * `FeeTooHigh` - `fee_bps` exceeds 10000.
    /// * `InvalidAmount` - `min_refund_amount` is negative.
    pub fn update_config(
        env: Env,
        admin: Address,
        fee_bps: u32,
        min_refund_amount: i128,
    ) -> Result<(), RefundError> {
        admin.require_auth();
        Self::ensure_initialized(&env)?;

        let stored_admin: Address = Self::get_admin_internal(&env)?;
        if admin != stored_admin {
            return Err(RefundError::Unauthorized);
        }
        if fee_bps > 10_000 {
            return Err(RefundError::FeeTooHigh);
        }
        if min_refund_amount < 0 {
            return Err(RefundError::InvalidAmount);
        }

        env.storage().instance().set(
            &StorageKey::RefundConfig,
            &RefundConfig {
                fee_bps,
                min_refund_amount,
            },
        );

        Self::emit_config_updated(&env, &admin, fee_bps, min_refund_amount);

        Ok(())
    }

    /// Get the stored admin address.
    ///
    /// Bumps instance storage TTL on read to prevent premature archival.
    pub fn get_admin(env: Env) -> Result<Address, RefundError> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        Self::get_admin_internal(&env)
    }

    /// Get the current refund configuration.
    ///
    /// Bumps instance storage TTL on read to prevent premature archival.
    pub fn get_config(env: Env) -> Result<RefundConfig, RefundError> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        env.storage()
            .instance()
            .get(&StorageKey::RefundConfig)
            .ok_or(RefundError::NotInitialized)
    }

    /// Get a refund request by ID.
    ///
    /// Bumps instance and persistent storage TTL on read to prevent premature archival.
    pub fn get_refund_request(
        env: Env,
        request_id: u64,
    ) -> Result<RefundRequest, RefundError> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        let key = StorageKey::PendingRefund(request_id);
        if env.storage().persistent().has(&key) {
            env.storage()
                .persistent()
                .extend_ttl(&key, PERSISTENT_BUMP_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
        }
        env.storage()
            .persistent()
            .get(&key)
            .ok_or(RefundError::NotFound)
    }

    /// Get total refunds processed.
    ///
    /// Bumps instance storage TTL on read to prevent premature archival.
    pub fn get_total_refunds(env: Env) -> Result<i128, RefundError> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        Self::ensure_initialized(&env)?;
        Ok(env
            .storage()
            .instance()
            .get(&StorageKey::TotalRefunds)
            .unwrap_or(0))
    }

    /// Get the current refund counter (next request ID).
    ///
    /// Bumps instance storage TTL on read to prevent premature archival.
    pub fn get_refund_counter(env: Env) -> Result<u64, RefundError> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        Self::ensure_initialized(&env)?;
        Ok(env
            .storage()
            .instance()
            .get(&StorageKey::RefundCounter)
            .unwrap_or(0))
    }

    /// Internal helper to get admin without TTL bump (for internal use).
    fn get_admin_internal(env: &Env) -> Result<Address, RefundError> {
        env.storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(RefundError::NotInitialized)
    }

    /// Internal helper to ensure contract is initialized.
    fn ensure_initialized(env: &Env) -> Result<(), RefundError> {
        if !env.storage().instance().has(&StorageKey::Admin) {
            return Err(RefundError::NotInitialized);
        }
        Ok(())
    }

    /// Emit the initialized event.
    fn emit_initialized(
        env: &Env,
        admin: &Address,
        fee_bps: u32,
        min_refund_amount: i128,
    ) {
        env.events().publish(
            (Symbol::new(env, "initialized"),),
            InitializedEvent {
                admin: admin.clone(),
                fee_bps,
                min_refund_amount,
            },
        );
    }

    /// Emit the refund requested event.
    fn emit_refund_requested(
        env: &Env,
        request_id: u64,
        requester: &Address,
        token: &Address,
        amount: i128,
        reason: &Symbol,
    ) {
        env.events().publish(
            (Symbol::new(env, "refund_requested"),),
            RefundRequestedEvent {
                request_id,
                requester: requester.clone(),
                token: token.clone(),
                amount,
                reason: reason.clone(),
            },
        );
    }

    /// Emit the refund processed event.
    fn emit_refund_processed(
        env: &Env,
        request_id: u64,
        processor: &Address,
        amount: i128,
        status: RefundStatus,
    ) {
        env.events().publish(
            (Symbol::new(env, "refund_processed"),),
            RefundProcessedEvent {
                request_id,
                processor: processor.clone(),
                amount,
                status,
            },
        );
    }

    /// Emit the config updated event.
    fn emit_config_updated(
        env: &Env,
        admin: &Address,
        fee_bps: u32,
        min_refund_amount: i128,
    ) {
        env.events().publish(
            (Symbol::new(env, "config_updated"),),
            RefundConfigUpdatedEvent {
                admin: admin.clone(),
                fee_bps,
                min_refund_amount,
            },
        );
    }
}

/// Event types for the refund contract.
#[soroban_sdk::contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitializedEvent {
    pub admin: Address,
    pub fee_bps: u32,
    pub min_refund_amount: i128,
}

/// Event emitted when a refund is requested.
#[soroban_sdk::contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefundRequestedEvent {
    pub request_id: u64,
    pub requester: Address,
    pub token: Address,
    pub amount: i128,
    pub reason: Symbol,
}

/// Event emitted when a refund is processed (approved, rejected, or processed).
#[soroban_sdk::contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefundProcessedEvent {
    pub request_id: u64,
    pub processor: Address,
    pub amount: i128,
    pub status: RefundStatus,
}

/// Event emitted when refund configuration is updated.
#[soroban_sdk::contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefundConfigUpdatedEvent {
    pub admin: Address,
    pub fee_bps: u32,
    pub min_refund_amount: i128,
}

/// Generate contract client for testing.
#[cfg(feature = "testutils")]
pub mod testutils {
    use super::*;
    use soroban_sdk::{contractclient, Address, Env, Symbol};

    #[contractclient(name = "RefundContractClient")]
    pub trait RefundContract {
        fn init(
            env: Env,
            admin: Address,
            fee_bps: u32,
            min_refund_amount: i128,
        ) -> Result<(), RefundError>;
        fn request_refund(
            env: Env,
            requester: Address,
            token: Address,
            amount: i128,
            reason: Symbol,
        ) -> Result<u64, RefundError>;
        fn approve_refund(
            env: Env,
            admin: Address,
            request_id: u64,
        ) -> Result<(), RefundError>;
        fn reject_refund(
            env: Env,
            admin: Address,
            request_id: u64,
        ) -> Result<(), RefundError>;
        fn process_refund(
            env: Env,
            admin: Address,
            request_id: u64,
        ) -> Result<(), RefundError>;
        fn update_config(
            env: Env,
            admin: Address,
            fee_bps: u32,
            min_refund_amount: i128,
        ) -> Result<(), RefundError>;
        fn get_admin(env: Env) -> Result<Address, RefundError>;
        fn get_config(env: Env) -> Result<RefundConfig, RefundError>;
        fn get_refund_request(env: Env, request_id: u64) -> Result<RefundRequest, RefundError>;
        fn get_total_refunds(env: Env) -> Result<i128, RefundError>;
        fn get_refund_counter(env: Env) -> Result<u64, RefundError>;
    }
}
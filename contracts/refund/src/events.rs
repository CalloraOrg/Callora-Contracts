use soroban_sdk::{contracttype, Address, Symbol};

/// Event emitted when the contract is initialized.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitializedEvent {
    pub admin: Address,
    pub fee_bps: u32,
    pub min_refund_amount: i128,
}

/// Event emitted when a refund is requested.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefundRequestedEvent {
    pub request_id: u64,
    pub requester: Address,
    pub token: Address,
    pub amount: i128,
    pub reason: Symbol,
}

/// Event emitted when a refund is processed (approved, rejected, or processed).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefundProcessedEvent {
    pub request_id: u64,
    pub processor: Address,
    pub amount: i128,
    pub status: crate::types::RefundStatus,
}

/// Event emitted when refund configuration is updated.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefundConfigUpdatedEvent {
    pub admin: Address,
    pub fee_bps: u32,
    pub min_refund_amount: i128,
}
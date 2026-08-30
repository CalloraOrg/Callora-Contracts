use soroban_sdk::contracterror;

/// Stable, machine-readable error codes for the revenue pool contract.
///
/// Numeric discriminants are part of the public contract interface and must
/// remain stable over time. Callers and indexers can branch on these `u32`
/// codes instead of parsing panic strings.
///
/// | Code | Variant                       | Meaning                                             |
/// |------|-------------------------------|-----------------------------------------------------|
/// | 1    | BatchEmpty                    | Batch distribution received no payment legs         |
/// | 2    | BatchTooLarge                 | Batch distribution exceeded `MAX_BATCH_SIZE`        |
/// | 3    | NotInitialized                | A function was called before `init`                  |
/// | 4    | AlreadyInitialized            | `init` was called more than once                     |
/// | 5    | Unauthorized                  | Caller is not authorized for the operation           |
/// | 6    | Paused                        | Distribution is blocked while the pool is paused     |
/// | 7    | AlreadyPaused                 | `pause` was called while already paused              |
/// | 8    | NotPaused                     | `unpause` was called while not paused                |
/// | 9    | InvalidUsdcToken              | USDC address conflicts with the pool or admin        |
/// | 10   | NoAdminTransferPending        | No admin transfer is pending                         |
/// | 11   | NoPauseGuardian               | No pause guardian is configured                     |
/// | 12   | AmountNotPositive             | Amount must be greater than zero                     |
/// | 13   | AmountExceedsMaxDistribute    | Amount exceeds the configured per-leg cap           |
/// | 14   | InvalidRecipient              | Recipient is the revenue pool contract              |
/// | 15   | InsufficientBalance           | Pool USDC balance is below the requested amount      |
/// | 16   | DuplicateRecipient            | A batch contains the same recipient more than once   |
/// | 17   | Overflow                      | Checked arithmetic detected an overflow             |
/// | 18   | MaxDistributeNotPositive      | Distribution cap must be greater than zero           |
/// | 19   | MessageEmpty                  | Admin broadcast message is empty                    |
/// | 20   | MessageTooLong                | Admin broadcast message exceeds the length limit     |
/// | 21   | NoPendingEmergencyDrain       | No emergency drain proposal is pending               |
/// | 22   | TimelockNotExpired            | Emergency drain timelock has not elapsed             |
/// | 23   | EmergencyPaused               | Recovery-only emergency mode is active               |
/// | 24   | AlreadyEmergencyPaused        | Emergency pause was already active                   |
/// | 25   | NotEmergencyPaused            | Emergency recovery was requested while inactive      |
/// | 26   | BelowMinDistribute            | Payout amount is below the configured minimum        |
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum RevenuePoolError {
    /// Batch distribution received no payment legs (code 1).
    BatchEmpty = 1,
    /// Batch distribution exceeded `MAX_BATCH_SIZE` (code 2).
    BatchTooLarge = 2,
    /// A function was called before `init` (code 3).
    NotInitialized = 3,
    /// `init` was called more than once (code 4).
    AlreadyInitialized = 4,
    /// Caller is not authorized for the operation (code 5).
    Unauthorized = 5,
    /// Distribution is blocked while the pool is paused (code 6).
    Paused = 6,
    /// `pause` was called while the pool was already paused (code 7).
    AlreadyPaused = 7,
    /// `unpause` was called while the pool was not paused (code 8).
    NotPaused = 8,
    /// USDC address conflicts with the pool or admin address (code 9).
    InvalidUsdcToken = 9,
    /// No admin transfer is pending (code 10).
    NoAdminTransferPending = 10,
    /// No pause guardian is configured (code 11).
    NoPauseGuardian = 11,
    /// Amount must be greater than zero (code 12).
    AmountNotPositive = 12,
    /// Amount exceeds the configured per-leg cap (code 13).
    AmountExceedsMaxDistribute = 13,
    /// Recipient is the revenue pool contract (code 14).
    InvalidRecipient = 14,
    /// Pool USDC balance is below the requested amount (code 15).
    InsufficientBalance = 15,
    /// A batch contains the same recipient more than once (code 16).
    DuplicateRecipient = 16,
    /// Checked arithmetic detected an overflow (code 17).
    Overflow = 17,
    /// Distribution cap must be greater than zero (code 18).
    MaxDistributeNotPositive = 18,
    /// Admin broadcast message is empty (code 19).
    MessageEmpty = 19,
    /// Admin broadcast message exceeds the length limit (code 20).
    MessageTooLong = 20,
    /// No emergency drain proposal is pending (code 21).
    NoPendingEmergencyDrain = 21,
    /// Emergency drain timelock has not elapsed (code 22).
    TimelockNotExpired = 22,
    /// Recovery-only emergency mode is active (code 23).
    EmergencyPaused = 23,
    /// Emergency pause was already active (code 24).
    AlreadyEmergencyPaused = 24,
    /// Emergency recovery was requested while inactive (code 25).
    NotEmergencyPaused = 25,
    /// Payout amount is below the configured minimum transfer unit (code 26).
    BelowMinDistribute = 26,
}

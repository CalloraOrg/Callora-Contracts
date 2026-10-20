# Contract Error Codes

Stable, semantic `u32` error codes used by the GrantFox smart contracts.
These numeric discriminants are part of each contract's public interface and
must not be reassigned once released.

## Stability rules

- Preserve every existing numeric code for its current semantic meaning.
- Add new variants only with new, previously unused codes in that contract.
- Do not reuse a removed code for a different error.
- `cargo test --workspace` enforces code stability and duplicate-code checks.

## Vault

| Code | Variant | Contract | Meaning |
|------|---------|----------|---------|
| 1 | `NotInitialized` | Vault | Vault has not been initialized |
| 2 | `AlreadyInitialized` | Vault | `init` was called more than once |
| 3 | `Unauthorized` | Vault | Caller is not authorized for the operation |
| 4 | `Paused` | Vault | State-changing action is blocked while paused |
| 5 | `InsufficientBalance` | Vault | Vault balance is too low for the requested operation |
| 6 | `AmountNotPositive` | Vault | Amount must be greater than zero |
| 7 | `ExceedsMaxDeduct` | Vault | Deduct amount exceeds the configured cap |
| 8 | `BelowMinDeposit` | Vault | Deposit amount is below the configured minimum |
| 9 | `Overflow` | Vault | Arithmetic overflow was detected |
| 10 | `InitialBalanceNegative` | Vault | Initial balance must be non-negative |
| 11 | `MinDepositNotPositive` | Vault | Minimum deposit must be greater than zero |
| 12 | `MaxDeductNotPositive` | Vault | Maximum deduct must be greater than zero |
| 13 | `MinDepositExceedsMaxDeduct` | Vault | Minimum deposit cannot exceed maximum deduct |
| 14 | `UsdcTokenCannotBeVault` | Vault | USDC token address cannot be the vault contract |
| 15 | `RevenuePoolCannotBeVault` | Vault | Revenue pool address cannot be the vault contract |
| 16 | `AuthorizedCallerCannotBeVault` | Vault | Authorized caller cannot be the vault contract |
| 17 | `InitialBalanceExceedsOnLedger` | Vault | Initial tracked balance exceeds on-ledger USDC |
| 18 | `AlreadyPaused` | Vault | Contract is already paused |
| 19 | `NotPaused` | Vault | Contract is not paused |
| 20 | `SettlementNotSet` | Vault | Settlement address has not been configured |
| 21 | `BatchEmpty` | Vault | Batch deduct received no items |
| 22 | `BatchTooLarge` | Vault | Batch deduct exceeds the maximum allowed size |
| 23 | `NewOwnerSameAsCurrent` | Vault | Proposed owner matches the current owner |
| 24 | `NoOwnershipTransferPending` | Vault | No ownership transfer is pending |
| 25 | `NoAdminTransferPending` | Vault | No admin transfer is pending |
| 26 | `OfferingIdTooLong` | Vault | Offering ID exceeds the maximum length |
| 27 | `MetadataTooLong` | Vault | Metadata exceeds the maximum length |
| 28 | `PriceParseError` | Vault | Price is invalid or non-positive |
| 29 | `DuplicateRequestId` | Vault | Request ID has already been processed |
| 30 | `OfferingIdInvalid` | Vault | Offering ID is empty or contains invalid characters |
| 31 | `MetadataInvalid` | Vault | Metadata is empty or contains invalid characters |
| 32 | `StaleNonce` | Vault | Rotation nonce does not match the stored current nonce |
| 33 | `NewRevenuePoolSameAsCurrent` | Vault | Proposed revenue pool matches the current revenue pool |
| 34 | `NoRevenuePoolTransferPending` | Vault | No revenue-pool transfer is pending |
| 35 | `Slippage` | Vault | Calculated fee in basis points exceeds the caller-supplied `max_fee_bps` limit |
| 36 | `RateLimited` | Vault | Developer exceeded the configured rate limit |
| 37 | `PausedState` | Vault | Operation is rejected because the vault is paused |
| 38 | `InvalidHotBps` | Vault | Hot BPS must be between 1 and 10000 |
| 39 | `InvalidRebalanceThreshold` | Vault | Rebalance threshold must be between 1 and 10000 |
| 40 | _reserved_ (was `ColdSignersEmpty`) | Vault | Retired: never returned; removed to keep `VaultError` within the 50-case contract error spec limit |
| 41 | _reserved_ (was `InvalidColdThreshold`) | Vault | Retired: never returned; removed to keep `VaultError` within the 50-case contract error spec limit |
| 42 | _reserved_ (was `DuplicateColdSigner`) | Vault | Retired: never returned; removed to keep `VaultError` within the 50-case contract error spec limit |
| 43 | `ExceedsReserveCap` | Vault | Deposit would exceed the configured per-token reserve cap |
| 44 | `CallerNotInAllowlist` | Vault | Caller is not in allowlist and not owner |
| 45 | `InvalidRecipient` | Vault | Withdrawal recipient is the vault address or USDC token address |
| 46 | `ZeroAddressRecipient` | Vault | Withdrawal recipient is the zero address |
| 47 | `InvalidHotBps` | Vault | Hot BPS must be between 1 and 10000 |
| 48 | `InvalidRebalanceThreshold` | Vault | Rebalance threshold must be between 1 and 10000 |
| 49 | `AdminCooldownActive` | Vault | A critical admin action is still inside the global cool-off window |
| 50 | `InvalidAdminCooldown` | Vault | Admin cool-off window is outside the accepted bounds |
| 51 | `ProposalNotFound` | Vault | No pending timelock proposal for the requested action |
| 52 | `TimelockNotExpired` | Vault | Action attempted before the timelock window has elapsed |
| 53 | `TimelockOverflow` | Vault | `proposed_at + window` overflowed `u64` |
| 54 | `InvalidTimelockWindow` | Vault | Proposed timelock window is outside the allowed `MIN..=MAX` bounds |
| 55 | `BelowMinTransferAmount` | Vault | Amount is below the vault's configured minimum transfer unit |
| 56 | `SettlementCannotBeVault` | Vault | Settlement address cannot be the vault contract itself |
| 57 | `SettlementCannotBeToken` | Vault | Settlement address cannot be the USDC token contract |
| 58 | `ProposalExpired` | Vault | Proposal has passed its grace period and is no longer executable |

## Settlement

| Code | Variant | Contract | Meaning |
|------|---------|----------|---------|
| 1 | `NotInitialized` | Settlement | A function was called before `init` |
| 2 | `AlreadyInitialized` | Settlement | `init` was called more than once |
| 3 | `Unauthorized` | Settlement | Caller is not the vault or current admin |
| 4 | `AmountNotPositive` | Settlement | Amount must be greater than zero |
| 5 | `DeveloperRequired` | Settlement | `to_pool=false` requires a developer address |
| 6 | `DeveloperMustBeNone` | Settlement | `to_pool=true` forbids a developer address |
| 7 | `PoolOverflow` | Settlement | Global pool credit would overflow `i128` |
| 8 | `DeveloperOverflow` | Settlement | Developer balance credit would overflow `i128` |
| 9 | `UsdcTokenNotConfigured` | Settlement | USDC token address is not configured |
| 10 | `InsufficientDeveloperBalance` | Settlement | Developer balance is lower than the withdrawal |
| 11 | `DeveloperBalanceUnderflow` | Settlement | Developer balance debit would underflow |
| 12 | `InsufficientContractBalance` | Settlement | Contract USDC balance is lower than requested amount |
| 13 | `DailyWithdrawCapExceeded` | Settlement | Daily developer withdrawal cap would be exceeded |
| 14 | `GasExhaustionRisk` | Settlement | Full scan is too large; use paginated access |
| 15 | _reserved_ (was `ReasonTooLong`) | Settlement | Retired: never returned |
| 16 | `MigrationSameAddress` | Settlement | Migration source and target are identical |
| 17 | `InvalidMigrationTarget` | Settlement | Migration target is the settlement contract |
| 18 | `NoDeveloperBalance` | Settlement | Migration source has no positive balance |
| 19 | `TimelockOverflow` | Settlement | Timelock timestamp addition overflowed |
| 20 | `MigrationNotFound` | Settlement | No migration is pending for the source |
| 21 | `TimelockNotExpired` | Settlement | Migration delay has not elapsed |
| 22 | `MigrationBalanceChanged` | Settlement | Approved amount is no longer available |
| 23 | `OverDraft` | Settlement | Withdrawal amount exceeds the developer's balance |
| 24 | `InvalidClaimWindow` | Settlement | Claim window parameters are invalid |
| 25 | `ClaimWindowClosed` | Settlement | Developer claim window is not currently open |
| 26 | `MinBalanceViolation` | Settlement | Withdrawal would leave balance below the minimum |
| 27 | `ReplayDetected` | Settlement | Settlement request reused or regressed the replay-guard ledger sequence |
| 28 | `BatchEmpty` | Settlement | Batch operation received an empty vector |
| 29 | `BatchTooLarge` | Settlement | Batch operation exceeded the maximum allowed size |
| 30 | `DeveloperFrozen` | Settlement | Developer is frozen and cannot withdraw |
| 31 | `DeveloperNotFrozen` | Settlement | Developer is not frozen; cannot unfreeze |
| 32 | `FreezeUnauthorized` | Settlement | Caller is not authorized to freeze/unfreeze |
| 33 | `WriteRateLimitExceeded` | Settlement | Admin wrote prices too frequently |
| 34 | `InvalidConfigDistinct` | Settlement | Init config requires distinct admin and vault |
| 35 | `InvalidConfigAdminContract` | Settlement | Init config forbids the admin being the contract |
| 36 | `InvalidConfigVaultContract` | Settlement | Init config forbids the vault being the contract |
| 37 | `InvalidUsdcToken` | Settlement | USDC token address is invalid |
| 38 | `InvalidRecipient` | Settlement | Withdrawal recipient cannot be the contract |
| 39 | `NoAdminTransferPending` | Settlement | No admin transfer is pending |
| 40 | `InvalidVault` | Settlement | Vault address is invalid |
| 41 | `NoVaultRotationPending` | Settlement | No vault rotation is pending |
| 42 | `BroadcastMessageTooLong` | Settlement | Admin broadcast message exceeds the maximum length |
| 43 | `CrossTenantBatch` | Settlement | Batch settlement mixed multiple developers |
| 44 | `NoUpgradePending` | Settlement | No upgrade proposal is currently pending |
| 45 | `ZeroWasmHash` | Settlement | Proposed WASM hash is all-zero (rejected) |
| 46 | `UpgradeTimelockNotExpired` | Settlement | Upgrade timelock delay has not yet elapsed |
| 47 | `UnsupportedToken` | Settlement | Token is not enabled for settlement payments |
| 48 | `DuplicateRequestId` | Settlement | Deduction request ID has already been recorded |
| 49 | `LengthMismatch` | Settlement | Paired batch vectors have different lengths |
| 50 | `InvalidCursor` | Settlement | Batch cursor is past the end or the listing cursor is past the end |
| 51 | `TooManyResults` | Settlement | Unpaginated developer-balance view exceeds its configured cap |
| 52 | `UpgradeProposalExpired` | Settlement | Upgrade proposal is past its expiry (`execute_after + grace`) |

## Revenue Pool

| Code | Variant | Contract | Meaning |
|------|---------|----------|---------|
| 1 | `BatchEmpty` | Revenue Pool | `batch_distribute` received an empty `payments` vector |
| 2 | `BatchTooLarge` | Revenue Pool | `batch_distribute` exceeded `MAX_BATCH_SIZE` |
| 3 | `NotInitialized` | Revenue Pool | A function was called before `init` |
| 4 | `AlreadyInitialized` | Revenue Pool | `init` was called more than once |
| 5 | `Unauthorized` | Revenue Pool | Caller is not authorized for the operation |
| 6 | `Paused` | Revenue Pool | Distribution is blocked while the pool is paused |
| 7 | `AlreadyPaused` | Revenue Pool | `pause` was called while the pool was already paused |
| 8 | `NotPaused` | Revenue Pool | `unpause` was called while the pool was not paused |
| 9 | `InvalidUsdcToken` | Revenue Pool | USDC address conflicts with the pool or admin address |
| 10 | `NoAdminTransferPending` | Revenue Pool | No admin transfer is pending |
| 11 | `NoPauseGuardian` | Revenue Pool | No pause guardian is configured |
| 12 | `AmountNotPositive` | Revenue Pool | Amount must be greater than zero |
| 13 | `AmountExceedsMaxDistribute` | Revenue Pool | Amount exceeds the configured per-leg cap |
| 14 | `InvalidRecipient` | Revenue Pool | Recipient is the revenue pool contract |
| 15 | `InsufficientBalance` | Revenue Pool | Pool USDC balance is below the requested amount |
| 16 | `DuplicateRecipient` | Revenue Pool | A batch contains the same recipient more than once |
| 17 | `Overflow` | Revenue Pool | Checked arithmetic detected an overflow |
| 18 | `MaxDistributeNotPositive` | Revenue Pool | Distribution cap must be greater than zero |
| 19 | `MessageEmpty` | Revenue Pool | Admin broadcast message is empty |
| 20 | `MessageTooLong` | Revenue Pool | Admin broadcast message exceeds the length limit |
| 21 | `NoPendingEmergencyDrain` | Revenue Pool | No emergency drain proposal is pending |
| 22 | `TimelockNotExpired` | Revenue Pool | Emergency drain timelock has not elapsed |
| 23 | `EmergencyPaused` | Revenue Pool | Recovery-only emergency mode is active |
| 24 | `AlreadyEmergencyPaused` | Revenue Pool | Emergency pause was already active |
| 25 | `NotEmergencyPaused` | Revenue Pool | Emergency recovery was requested while inactive |

## Upgrade

| Code | Variant | Contract | Meaning |
|------|---------|----------|---------|
| 1 | `NotInitialized` | Upgrade | Contract has not been initialized yet |
| 2 | `AlreadyInitialized` | Upgrade | `init` was called more than once |
| 3 | `Unauthorized` | Upgrade | Caller is not authorized for the operation |
| 4 | `InvalidWasmHash` | Upgrade | Provided WASM hash is zero or invalid |

## Freeze

| Code | Variant | Contract | Meaning |
|------|---------|----------|---------|
| 1 | `NotInitialized` | Freeze | Contract has not been initialized yet |
| 2 | `AlreadyInitialized` | Freeze | `init` was called more than once |
| 3 | `Unauthorized` | Freeze | Caller is not authorized for the operation |
| 4 | `AlreadyFrozen` | Freeze | Contract is already frozen |
| 5 | `NotFrozen` | Freeze | Contract is not currently frozen |
| 6 | `Overflow` | Freeze | Arithmetic overflow detected |

# Contract Error Codes

Stable, semantic `u32` error codes used by the GrantFox smart contracts.
These numeric discriminants are part of each contract's public interface and
must not be reassigned once released.

## Stability rules

- Preserve every existing numeric code for its current semantic meaning.
- Add new variants only with new, previously unused codes in that contract.
- Do not reuse a removed code for a different error.
- `cargo test --workspace` enforces code stability and duplicate-code checks.
- `cargo test -p callora-contracts-e2e --test error_codes_doc` verifies that this document matches the real `#[contracterror]` enums in both directions.

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
| 10 | _reserved_ (was `InitialBalanceNegative`) | Vault | Retired: never returned |
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
| 23 | _reserved_ (was `NewOwnerSameAsCurrent`) | Vault | Retired: never returned |
| 24 | `NoOwnershipTransferPending` | Vault | No ownership transfer is pending |
| 25 | `NoAdminTransferPending` | Vault | No admin transfer is pending |
| 26 | _reserved_ (was `OfferingIdTooLong`) | Vault | Retired: never returned |
| 27 | _reserved_ (was `MetadataTooLong`) | Vault | Retired: never returned |
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
| 60 | `Frozen` | Vault | System is globally frozen via the configured freeze contract |
| 59 | `SettlementEqualsRevenuePool` | Vault | Settlement and revenue pool addresses must be distinct |
| 60 | `TooManyIds` | Vault | `prune_processed_requests` received more ids than `MAX_PRUNE_IDS` (50) allows; split into smaller batches |

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
| 14 | _reserved_ (was `GasExhaustionRisk`) | Settlement | Retired: never returned |
| 15 | _reserved_ (was `ReasonTooLong`) | Settlement | Retired: never returned |
| 16 | `MigrationSameAddress` | Settlement | Migration source and target are identical |
| 17 | `InvalidMigrationTarget` | Settlement | Migration target is the settlement contract |
| 18 | `NoDeveloperBalance` | Settlement | Migration source has no positive balance |
| 19 | `TimelockOverflow` | Settlement | Timelock timestamp addition overflowed |
| 20 | `MigrationNotFound` | Settlement | No migration is pending for the source |
| 21 | `TimelockNotExpired` | Settlement | Migration delay has not elapsed |
| 22 | `MigrationBalanceChanged` | Settlement | Approved amount is no longer available |
| 23 | _reserved_ (was `OverDraft`) | Settlement | Retired: never returned |
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
| 38 | `InvalidRecipient` | Settlement | Withdrawal recipient or payout name is invalid |
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
| 50 | `InvalidCursor` | Settlement | Batch cursor is invalid or listing cursor is past the end |
| 51 | `TooManyResults` | Settlement | Unpaginated developer-balance view exceeds its configured cap |
| 52 | `InvalidOfferingId` | Settlement | Offering ID failed `callora-validators` validation |
| 53 | `InvalidPrice` | Settlement | Price string failed validation |
| 52 | `RecipientRegistryNotSet` | Settlement | A payout name is set but no recipient registry is configured |
| 53 | `PayoutNameNotFound` | Settlement | The configured payout name no longer resolves in the registry |
| 52 | `SystemFrozen` | Settlement | System is globally frozen via the configured freeze contract |

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
| 26 | `SharesEmpty` | Revenue Pool | Share distribution received no recipients |
| 27 | `InvalidShareSum` | Revenue Pool | Share distribution basis points did not sum to 10_000 |
| 28 | `InvalidShare` | Revenue Pool | A share was zero or exceeded 10_000 basis points |

## Upgrade

| Code | Variant | Contract | Meaning |
|------|---------|----------|---------|
| 1 | `NotInitialized` | Upgrade | Contract has not been initialized yet |
| 2 | `AlreadyInitialized` | Upgrade | `init` was called more than once |
| 3 | `Unauthorized` | Upgrade | Caller is not authorized for the operation |
| 4 | `InvalidWasmHash` | Upgrade | Provided WASM hash is zero or invalid |
| 5 | `UpgradeNotAllowed` | Upgrade | Upgrade operation is currently disabled |
| 6 | `MigrationPending` | Upgrade | A migration or upgrade is already pending |
| 7 | `TimelockNotExpired` | Upgrade | Required timelock delay has not elapsed |
| 8 | `SameWasmHash` | Upgrade | New WASM hash is identical to current WASM hash |
| 9 | `SameVersion` | Upgrade | Proposed version matches current version |
| 10 | `InvalidVersion` | Upgrade | Proposed version number is invalid or non-increasing |
| 11 | `Overflow` | Upgrade | Arithmetic calculation overflowed |
| 12 | `AlreadyUpgraded` | Upgrade | Contract has already been upgraded to this state |
| 13 | `StaleNonce` | Upgrade | Transaction nonce is stale or invalid |
| 14 | `MigrationSameAddress` | Upgrade | Target migration contract address matches source |
| 15 | `InvalidMigrationTarget` | Upgrade | Target migration contract address is invalid |
| 16 | `NoUpgradePending` | Upgrade | No pending upgrade was found to execute or cancel |
| 17 | `CooldownNotElapsed` | Upgrade | The cooldown period for upgrades has not yet elapsed |
| 18 | `InvalidCooldown` | Upgrade | Requested cooldown is outside the allowed bounds |

## Freeze

| Code | Variant | Contract | Meaning |
|------|---------|----------|---------|
| 1 | `NotInitialized` | Freeze | Contract has not been initialized yet |
| 2 | `AlreadyInitialized` | Freeze | `init` was called more than once |
| 3 | `Unauthorized` | Freeze | Caller is not authorized for the operation |
| 4 | `AlreadyFrozen` | Freeze | Contract is already frozen |
| 5 | `NotFrozen` | Freeze | Contract is not currently frozen |
| 6 | `Overflow` | Freeze | Arithmetic overflow detected |

## Refund

| Code | Variant | Contract | Meaning |
|------|---------|----------|---------|
| 1 | `AlreadyInitialized` | Refund | Contract has already been initialized |
| 2 | `NotInitialized` | Refund | Contract has not been initialized |
| 3 | `Unauthorized` | Refund | Caller is not authorized to perform this action |
| 4 | `NotFound` | Refund | Requested refund not found |
| 5 | `FeeTooHigh` | Refund | Fee basis points exceeds maximum (`10000`) |
| 6 | `InvalidAmount` | Refund | Amount is not positive or minimum amount is negative |
| 7 | `AmountTooLow` | Refund | Amount is below minimum refund amount |
| 8 | `Overflow` | Refund | Arithmetic overflow would occur |
| 9 | `InvalidStatus` | Refund | Refund request is not in the expected status |
| 10 | `InsufficientBalance` | Refund | Contract balance or accumulated fees are insufficient |
| 11 | `InvalidRecipient` | Refund | Recipient address is invalid (self or token address) |
| 12 | `TooManyOpenRequests` | Refund | Requester has reached the maximum number of open requests |
| 13 | `ApprovedAmountInvalid` | Refund | Partial approved amount is `<= 0` or `> request.amount` |
| 14 | `DisputeWindowClosed` | Refund | Dispute was submitted after the configured dispute window elapsed |
| 15 | `AlreadyDisputed` | Refund | Request has already been disputed once |

## Escrow

| 1 | `NotInitialized` | Escrow | Contract has not been initialized yet |
| 2 | `AlreadyInitialized` | Escrow | `init` was called more than once |
| 3 | `Unauthorized` | Escrow | Caller is not authorized for this operation |
| 4 | `CooldownActive` | Escrow | A critical action is still inside its cool-off window |
| 5 | `InvalidCooldown` | Escrow | Proposed cooldown value is outside the accepted range |
| 6 | `NoPendingAdmin` | Escrow | No admin transfer is currently pending |
| 7 | `Overflow` | Escrow | Arithmetic overflow detected |
| 8 | `AssetNotApproved` | Escrow | Payment asset is not approved for escrow creation |
| 9 | `InvalidInput` | Escrow | A supplied input is malformed or forbidden |
| 10 | `EscrowExists` | Escrow | Escrow already exists for the given payment asset and recipient |
| 11 | `Paused` | Escrow | Operation is rejected because the contract is paused |
| 12 | `NotFound` | Escrow | Escrow record not found for the given payment asset and recipient |
| 13 | `BatchEmpty` | Escrow | `batch_release` was called with an empty `entries` list |
| 14 | `BatchTooLarge` | Escrow | `batch_release` exceeded `MAX_BATCH_RELEASE` entries |
| 15 | `DuplicateEntry` | Escrow | `batch_release` lists the same `(payment_asset, recipient)` pair twice |


| 5 | `FeeTooHigh` | Refund | Fee basis points exceeds maximum (10000) |
| 6 | `InvalidAmount` | Refund | Amount is not positive |
| 10 | `InsufficientBalance` | Refund | Contract token balance or accumulated fees are insufficient for payout or withdrawal |
| 11 | `InvalidRecipient` | Refund | Recipient address is invalid (e.g. self or token address) |
| 12 | `TooManyOpenRequests` | Refund | Requester has reached the maximum number of open refund requests |
| 7 | `FreezeUntilNotInFuture` | Freeze | `freeze_until` deadline is not in the future (`until <= now`) |

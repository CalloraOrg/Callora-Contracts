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
| 15 | `ReasonTooLong` | Settlement | Reason `Symbol` exceeds the allowed length |
| 16 | `MigrationSameAddress` | Settlement | Migration source and target are identical |
| 17 | `InvalidMigrationTarget` | Settlement | Migration target is the settlement contract |
| 18 | `NoDeveloperBalance` | Settlement | Migration source has no positive balance |
| 19 | `TimelockOverflow` | Settlement | Timelock timestamp addition overflowed |
| 20 | `MigrationNotFound` | Settlement | No migration is pending for the source |
| 21 | `TimelockNotExpired` | Settlement | Migration delay has not elapsed |
| 22 | `MigrationBalanceChanged` | Settlement | Approved amount is no longer available |

## Revenue Pool

| Code | Variant | Contract | Meaning |
|------|---------|----------|---------|
| 1 | `BatchEmpty` | Revenue Pool | `batch_distribute` received an empty `payments` vector |
| 2 | `BatchTooLarge` | Revenue Pool | `batch_distribute` exceeded `MAX_BATCH_SIZE` |

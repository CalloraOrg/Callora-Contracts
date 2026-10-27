use soroban_sdk::{xdr::ScErrorType, Error};

/// Stable, machine-readable error codes for the settlement contract.
///
/// The numeric discriminants in this enum are part of the contract interface and
/// must remain stable over time. Callers and indexers may branch on these `u32`
/// codes instead of parsing panic strings.
///
/// | Code | Variant | Meaning |
/// |------|---------|---------|
/// | 1 | NotInitialized | Called before `init` |
/// | 2 | AlreadyInitialized | `init` called more than once |
/// | 3 | Unauthorized | Caller is not the vault or current admin |
/// | 4 | AmountNotPositive | Amount must be greater than zero |
/// | 5 | DeveloperRequired | `to_pool=false` requires a developer address |
/// | 6 | DeveloperMustBeNone | `to_pool=true` forbids a developer address |
/// | 7 | PoolOverflow | Global pool credit would overflow `i128` |
/// | 8 | DeveloperOverflow | Developer balance credit would overflow `i128` |
/// | 9 | UsdcTokenNotConfigured | USDC token address is not configured |
/// | 10 | InsufficientDeveloperBalance | Developer balance is lower than the withdrawal |
/// | 11 | DeveloperBalanceUnderflow | Developer balance debit would underflow |
/// | 12 | InsufficientContractBalance | Contract USDC balance is lower than requested amount |
/// | 13 | DailyWithdrawCapExceeded | Daily developer withdrawal cap would be exceeded |
/// | 14 | GasExhaustionRisk | Full scan is too large; use paginated access |
/// | 15 | ReasonTooLong | Reason `Symbol` exceeds the allowed length |
/// | 16 | MigrationSameAddress | Migration source and target are identical |
/// | 17 | InvalidMigrationTarget | Migration target is the settlement contract |
/// | 18 | NoDeveloperBalance | Migration source has no positive balance |
/// | 19 | TimelockOverflow | Timelock timestamp addition overflowed |
/// | 20 | MigrationNotFound | No migration is pending for the source |
/// | 21 | TimelockNotExpired | Migration delay has not elapsed |
/// | 22 | MigrationBalanceChanged | Approved amount is no longer available |
/// | 23 | OverDraft | Withdrawal amount exceeds the developer's balance |
/// | 24 | InvalidClaimWindow | Claim window parameters are invalid |
/// | 25 | ClaimWindowClosed | Developer claim window is not currently open |
/// | 26 | MinBalanceViolation | Withdrawal would leave balance below the minimum |
/// | 27 | ReplayDetected | Settlement claim was replayed or out of order |
/// | 28 | BatchEmpty | Batch operation received an empty vector |
/// | 29 | BatchTooLarge | Batch operation exceeded the maximum allowed size |
/// | 30 | DeveloperFrozen | Developer is frozen and cannot withdraw |
/// | 31 | DeveloperNotFrozen | Developer is not frozen; cannot unfreeze |
/// | 32 | FreezeUnauthorized | Caller is not authorized to freeze/unfreeze |
/// | 33 | WriteRateLimitExceeded | Admin wrote prices too frequently |
/// | 34 | InvalidConfigDistinct | Init config requires distinct admin and vault |
/// | 35 | InvalidConfigAdminContract | Init config forbids the admin being the contract |
/// | 36 | InvalidConfigVaultContract | Init config forbids the vault being the contract |
/// | 37 | InvalidUsdcToken | USDC token address is invalid |
/// | 38 | InvalidRecipient | Withdrawal recipient cannot be the contract |
/// | 39 | NoAdminTransferPending | No admin transfer is pending |
/// | 40 | InvalidVault | Vault address is invalid |
/// | 41 | NoVaultRotationPending | No vault rotation is pending |
/// | 42 | BroadcastMessageTooLong | Admin broadcast message exceeds the maximum length |
/// | 43 | CrossTenantBatch | Batch settlement mixes developers from different tenants |
/// | 44 | NoUpgradePending | No upgrade proposal is currently pending |
/// | 45 | ZeroWasmHash | Proposed WASM hash is all-zero (rejected) |
/// | 46 | UpgradeTimelockNotExpired | Upgrade timelock delay has not yet elapsed |
/// | 47 | UnsupportedToken | Token is not enabled for settlement payments |
/// | 48 | DuplicateRequestId | Deduction request ID has already been recorded |
/// | 49 | LengthMismatch | Paired batch vectors have different lengths |
/// | 50 | InvalidCursor | Batch cursor is past the end or the limit is zero |
/// | 51 | UpgradeProposalExpired | Upgrade proposal is past its expiry |
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum SettlementError {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    Unauthorized = 3,
    AmountNotPositive = 4,
    DeveloperRequired = 5,
    DeveloperMustBeNone = 6,
    PoolOverflow = 7,
    DeveloperOverflow = 8,
    UsdcTokenNotConfigured = 9,
    InsufficientDeveloperBalance = 10,
    DeveloperBalanceUnderflow = 11,
    InsufficientContractBalance = 12,
    DailyWithdrawCapExceeded = 13,
    GasExhaustionRisk = 14,
    ReasonTooLong = 15,
    MigrationSameAddress = 16,
    InvalidMigrationTarget = 17,
    NoDeveloperBalance = 18,
    TimelockOverflow = 19,
    MigrationNotFound = 20,
    TimelockNotExpired = 21,
    MigrationBalanceChanged = 22,
    OverDraft = 23,
    InvalidClaimWindow = 24,
    ClaimWindowClosed = 25,
    MinBalanceViolation = 26,
    ReplayDetected = 27,
    BatchEmpty = 28,
    BatchTooLarge = 29,
    DeveloperFrozen = 30,
    DeveloperNotFrozen = 31,
    FreezeUnauthorized = 32,
    WriteRateLimitExceeded = 33,
    InvalidConfigDistinct = 34,
    InvalidConfigAdminContract = 35,
    InvalidConfigVaultContract = 36,
    InvalidUsdcToken = 37,
    InvalidRecipient = 38,
    NoAdminTransferPending = 39,
    InvalidVault = 40,
    NoVaultRotationPending = 41,
    BroadcastMessageTooLong = 42,
    CrossTenantBatch = 43,
    NoUpgradePending = 44,
    ZeroWasmHash = 45,
    UpgradeTimelockNotExpired = 46,
    UnsupportedToken = 47,
    DuplicateRequestId = 48,
    LengthMismatch = 49,
    InvalidCursor = 50,
    UpgradeProposalExpired = 51,
}

impl From<SettlementError> for Error {
    fn from(val: SettlementError) -> Self {
        Error::from_contract_error(val as u32)
    }
}

impl From<&SettlementError> for Error {
    fn from(val: &SettlementError) -> Self {
        Error::from_contract_error((*val) as u32)
    }
}

impl TryFrom<Error> for SettlementError {
    type Error = Error;

    fn try_from(error: Error) -> Result<Self, Self::Error> {
        if error.is_type(ScErrorType::Contract) {
            let code = error.get_code();
            Ok(match code {
                1 => Self::NotInitialized,
                2 => Self::AlreadyInitialized,
                3 => Self::Unauthorized,
                4 => Self::AmountNotPositive,
                5 => Self::DeveloperRequired,
                6 => Self::DeveloperMustBeNone,
                7 => Self::PoolOverflow,
                8 => Self::DeveloperOverflow,
                9 => Self::UsdcTokenNotConfigured,
                10 => Self::InsufficientDeveloperBalance,
                11 => Self::DeveloperBalanceUnderflow,
                12 => Self::InsufficientContractBalance,
                13 => Self::DailyWithdrawCapExceeded,
                14 => Self::GasExhaustionRisk,
                15 => Self::ReasonTooLong,
                16 => Self::MigrationSameAddress,
                17 => Self::InvalidMigrationTarget,
                18 => Self::NoDeveloperBalance,
                19 => Self::TimelockOverflow,
                20 => Self::MigrationNotFound,
                21 => Self::TimelockNotExpired,
                22 => Self::MigrationBalanceChanged,
                23 => Self::OverDraft,
                24 => Self::InvalidClaimWindow,
                25 => Self::ClaimWindowClosed,
                26 => Self::MinBalanceViolation,
                27 => Self::ReplayDetected,
                28 => Self::BatchEmpty,
                29 => Self::BatchTooLarge,
                30 => Self::DeveloperFrozen,
                31 => Self::DeveloperNotFrozen,
                32 => Self::FreezeUnauthorized,
                33 => Self::WriteRateLimitExceeded,
                34 => Self::InvalidConfigDistinct,
                35 => Self::InvalidConfigAdminContract,
                36 => Self::InvalidConfigVaultContract,
                37 => Self::InvalidUsdcToken,
                38 => Self::InvalidRecipient,
                39 => Self::NoAdminTransferPending,
                40 => Self::InvalidVault,
                41 => Self::NoVaultRotationPending,
                42 => Self::BroadcastMessageTooLong,
                43 => Self::CrossTenantBatch,
                44 => Self::NoUpgradePending,
                45 => Self::ZeroWasmHash,
                46 => Self::UpgradeTimelockNotExpired,
                47 => Self::UnsupportedToken,
                48 => Self::DuplicateRequestId,
                49 => Self::LengthMismatch,
                50 => Self::InvalidCursor,
                51 => Self::UpgradeProposalExpired,
                _ => return Err(error),
            })
        } else {
            Err(error)
        }
    }
}

impl TryFrom<&Error> for SettlementError {
    type Error = Error;

    fn try_from(error: &Error) -> Result<Self, Self::Error> {
        <_ as TryFrom<Error>>::try_from(*error)
    }
}

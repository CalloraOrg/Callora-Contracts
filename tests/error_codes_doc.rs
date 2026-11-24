//! Verifies that `docs/ERROR_CODES.md` matches the real `#[contracterror]` enums
//! across all documented Callora contracts in both directions (no missing, no extra,
//! no renumbering).
//!
//! Run with: `cargo test -p callora-contracts-e2e --test error_codes_doc`

use std::collections::BTreeMap;
use std::fmt::Debug;

use callora_escrow::errors::EscrowError;
use callora_freeze::FreezeError;
use callora_refund::RefundError;
use callora_revenue_pool::RevenuePoolError;
use callora_settlement::SettlementError;
use callora_upgrade::UpgradeError;
use callora_vault::VaultError;

const DOC: &str = include_str!("../docs/ERROR_CODES.md");

/// Parse `docs/ERROR_CODES.md` into `section -> (code -> variant)`.
///
/// Reserved rows (e.g. `_reserved_ (was ...)`) are skipped as non-enum entries,
/// while duplicate codes within the same section cause an immediate panic.
fn parse_doc(doc: &str) -> BTreeMap<String, BTreeMap<u32, String>> {
    let mut sections: BTreeMap<String, BTreeMap<u32, String>> = BTreeMap::new();
    let mut current_section: Option<String> = None;

    for (line_idx, raw_line) in doc.lines().enumerate() {
        let line = raw_line.trim();
        if let Some(heading) = line.strip_prefix("## ") {
            let name = heading.trim().to_string();
            current_section = Some(name);
            continue;
        }

        let Some(ref section) = current_section else {
            continue;
        };

        if !line.starts_with('|') {
            continue;
        }

        let cells: Vec<&str> = line
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect();

        if cells.len() < 3 {
            continue;
        }

        // Skip table header and separator rows.
        if cells[0] == "Code" || cells[0].chars().all(|c| c == '-' || c == ':') {
            continue;
        }

        let code: u32 = cells[0].parse().unwrap_or_else(|e| {
            panic!(
                "docs/ERROR_CODES.md:{}: invalid numeric code {:?} in section {:?}: {}",
                line_idx + 1,
                cells[0],
                section,
                e
            )
        });

        let variant_cell = cells[1];
        if variant_cell.starts_with("_reserved_") {
            continue;
        }

        let variant = variant_cell.trim_matches('`').to_string();
        let table = sections.entry(section.clone()).or_default();
        if let Some(prev) = table.insert(code, variant.clone()) {
            panic!(
                "docs/ERROR_CODES.md:{}: duplicate code {} in section {:?} (was `{}`, also `{}`)",
                line_idx + 1,
                code,
                section,
                prev,
                variant
            );
        }
    }

    sections
}

fn assert_section_matches<E: Copy + Debug>(
    sections: &BTreeMap<String, BTreeMap<u32, String>>,
    section: &str,
    variants: &[E],
    to_u32: fn(E) -> u32,
) {
    let doc_map = sections.get(section).unwrap_or_else(|| {
        panic!("docs/ERROR_CODES.md is missing section `## {}`", section);
    });

    let mut code_map: BTreeMap<u32, String> = BTreeMap::new();
    for &variant in variants {
        let code = to_u32(variant);
        let name = format!("{:?}", variant);
        if let Some(prev) = code_map.insert(code, name.clone()) {
            panic!(
                "{}: duplicate discriminant {} in enum (`{}` and `{}`)",
                section, code, prev, name
            );
        }
    }

    let mut errors: Vec<String> = Vec::new();

    // Check every enum variant is documented with the exact same code and name.
    for (code, variant) in &code_map {
        match doc_map.get(code) {
            None => errors.push(format!(
                "{}: code {} (`{}`) exists in code but is missing from docs/ERROR_CODES.md",
                section, code, variant
            )),
            Some(doc_variant) if doc_variant != variant => errors.push(format!(
                "{}: code {} drifted — code has `{}` but docs/ERROR_CODES.md has `{}`",
                section, code, variant, doc_variant
            )),
            Some(_) => {}
        }
    }

    // Check every documented entry exists in the enum.
    for (code, doc_variant) in doc_map {
        if !code_map.contains_key(code) {
            errors.push(format!(
                "{}: code {} (`{}`) is in docs/ERROR_CODES.md but does not exist in the contract enum",
                section, code, doc_variant
            ));
        }
    }

    assert!(
        errors.is_empty(),
        "Error code documentation drift detected for {}:\n  - {}",
        section,
        errors.join("\n  - ")
    );
}

#[test]
fn error_codes_doc_matches_contract_enums() {
    let sections = parse_doc(DOC);

    // Exhaustive match helpers ensure adding a new variant to any enum fails compilation
    // until it is also registered here and documented in docs/ERROR_CODES.md.
    let vault_variants = [
        VaultError::NotInitialized,
        VaultError::AlreadyInitialized,
        VaultError::Unauthorized,
        VaultError::Paused,
        VaultError::InsufficientBalance,
        VaultError::AmountNotPositive,
        VaultError::ExceedsMaxDeduct,
        VaultError::BelowMinDeposit,
        VaultError::Overflow,
        VaultError::MinDepositNotPositive,
        VaultError::MaxDeductNotPositive,
        VaultError::MinDepositExceedsMaxDeduct,
        VaultError::UsdcTokenCannotBeVault,
        VaultError::RevenuePoolCannotBeVault,
        VaultError::AuthorizedCallerCannotBeVault,
        VaultError::InitialBalanceExceedsOnLedger,
        VaultError::AlreadyPaused,
        VaultError::NotPaused,
        VaultError::SettlementNotSet,
        VaultError::BatchEmpty,
        VaultError::BatchTooLarge,
        VaultError::NoOwnershipTransferPending,
        VaultError::NoAdminTransferPending,
        VaultError::PriceParseError,
        VaultError::DuplicateRequestId,
        VaultError::OfferingIdInvalid,
        VaultError::MetadataInvalid,
        VaultError::StaleNonce,
        VaultError::NewRevenuePoolSameAsCurrent,
        VaultError::NoRevenuePoolTransferPending,
        VaultError::Slippage,
        VaultError::RateLimited,
        VaultError::PausedState,
        VaultError::InvalidHotBps,
        VaultError::InvalidRebalanceThreshold,
        VaultError::ExceedsReserveCap,
        VaultError::CallerNotInAllowlist,
        VaultError::InvalidRecipient,
        VaultError::ZeroAddressRecipient,
        VaultError::AdminCooldownActive,
        VaultError::InvalidAdminCooldown,
        VaultError::ProposalNotFound,
        VaultError::TimelockNotExpired,
        VaultError::TimelockOverflow,
        VaultError::InvalidTimelockWindow,
        VaultError::BelowMinTransferAmount,
        VaultError::SettlementCannotBeVault,
        VaultError::SettlementCannotBeToken,
        VaultError::ProposalExpired,
        VaultError::SettlementEqualsRevenuePool,
    ];
    for &v in &vault_variants {
        match v {
            VaultError::NotInitialized
            | VaultError::AlreadyInitialized
            | VaultError::Unauthorized
            | VaultError::Paused
            | VaultError::InsufficientBalance
            | VaultError::AmountNotPositive
            | VaultError::ExceedsMaxDeduct
            | VaultError::BelowMinDeposit
            | VaultError::Overflow
            | VaultError::MinDepositNotPositive
            | VaultError::MaxDeductNotPositive
            | VaultError::MinDepositExceedsMaxDeduct
            | VaultError::UsdcTokenCannotBeVault
            | VaultError::RevenuePoolCannotBeVault
            | VaultError::AuthorizedCallerCannotBeVault
            | VaultError::InitialBalanceExceedsOnLedger
            | VaultError::AlreadyPaused
            | VaultError::NotPaused
            | VaultError::SettlementNotSet
            | VaultError::BatchEmpty
            | VaultError::BatchTooLarge
            | VaultError::NoOwnershipTransferPending
            | VaultError::NoAdminTransferPending
            | VaultError::PriceParseError
            | VaultError::DuplicateRequestId
            | VaultError::OfferingIdInvalid
            | VaultError::MetadataInvalid
            | VaultError::StaleNonce
            | VaultError::NewRevenuePoolSameAsCurrent
            | VaultError::NoRevenuePoolTransferPending
            | VaultError::Slippage
            | VaultError::RateLimited
            | VaultError::PausedState
            | VaultError::InvalidHotBps
            | VaultError::InvalidRebalanceThreshold
            | VaultError::ExceedsReserveCap
            | VaultError::CallerNotInAllowlist
            | VaultError::InvalidRecipient
            | VaultError::ZeroAddressRecipient
            | VaultError::AdminCooldownActive
            | VaultError::InvalidAdminCooldown
            | VaultError::ProposalNotFound
            | VaultError::TimelockNotExpired
            | VaultError::TimelockOverflow
            | VaultError::InvalidTimelockWindow
            | VaultError::BelowMinTransferAmount
            | VaultError::SettlementCannotBeVault
            | VaultError::SettlementCannotBeToken
            | VaultError::ProposalExpired
            | VaultError::SettlementEqualsRevenuePool => {}
        }
    }
    assert_section_matches(&sections, "Vault", &vault_variants, |e| e as u32);

    let settlement_variants = [
        SettlementError::NotInitialized,
        SettlementError::AlreadyInitialized,
        SettlementError::Unauthorized,
        SettlementError::AmountNotPositive,
        SettlementError::DeveloperRequired,
        SettlementError::DeveloperMustBeNone,
        SettlementError::PoolOverflow,
        SettlementError::DeveloperOverflow,
        SettlementError::UsdcTokenNotConfigured,
        SettlementError::InsufficientDeveloperBalance,
        SettlementError::DeveloperBalanceUnderflow,
        SettlementError::InsufficientContractBalance,
        SettlementError::DailyWithdrawCapExceeded,
        SettlementError::GasExhaustionRisk,
        SettlementError::MigrationSameAddress,
        SettlementError::InvalidMigrationTarget,
        SettlementError::NoDeveloperBalance,
        SettlementError::TimelockOverflow,
        SettlementError::MigrationNotFound,
        SettlementError::TimelockNotExpired,
        SettlementError::MigrationBalanceChanged,
        SettlementError::OverDraft,
        SettlementError::InvalidClaimWindow,
        SettlementError::ClaimWindowClosed,
        SettlementError::MinBalanceViolation,
        SettlementError::ReplayDetected,
        SettlementError::BatchEmpty,
        SettlementError::BatchTooLarge,
        SettlementError::DeveloperFrozen,
        SettlementError::DeveloperNotFrozen,
        SettlementError::FreezeUnauthorized,
        SettlementError::WriteRateLimitExceeded,
        SettlementError::InvalidConfigDistinct,
        SettlementError::InvalidConfigAdminContract,
        SettlementError::InvalidConfigVaultContract,
        SettlementError::InvalidUsdcToken,
        SettlementError::InvalidRecipient,
        SettlementError::NoAdminTransferPending,
        SettlementError::InvalidVault,
        SettlementError::NoVaultRotationPending,
        SettlementError::BroadcastMessageTooLong,
        SettlementError::CrossTenantBatch,
        SettlementError::NoUpgradePending,
        SettlementError::ZeroWasmHash,
        SettlementError::UpgradeTimelockNotExpired,
        SettlementError::UnsupportedToken,
        SettlementError::DuplicateRequestId,
        SettlementError::LengthMismatch,
        SettlementError::InvalidCursor,
        SettlementError::TooManyResults,
    ];
    for &v in &settlement_variants {
        match v {
            SettlementError::NotInitialized
            | SettlementError::AlreadyInitialized
            | SettlementError::Unauthorized
            | SettlementError::AmountNotPositive
            | SettlementError::DeveloperRequired
            | SettlementError::DeveloperMustBeNone
            | SettlementError::PoolOverflow
            | SettlementError::DeveloperOverflow
            | SettlementError::UsdcTokenNotConfigured
            | SettlementError::InsufficientDeveloperBalance
            | SettlementError::DeveloperBalanceUnderflow
            | SettlementError::InsufficientContractBalance
            | SettlementError::DailyWithdrawCapExceeded
            | SettlementError::GasExhaustionRisk
            | SettlementError::MigrationSameAddress
            | SettlementError::InvalidMigrationTarget
            | SettlementError::NoDeveloperBalance
            | SettlementError::TimelockOverflow
            | SettlementError::MigrationNotFound
            | SettlementError::TimelockNotExpired
            | SettlementError::MigrationBalanceChanged
            | SettlementError::OverDraft
            | SettlementError::InvalidClaimWindow
            | SettlementError::ClaimWindowClosed
            | SettlementError::MinBalanceViolation
            | SettlementError::ReplayDetected
            | SettlementError::BatchEmpty
            | SettlementError::BatchTooLarge
            | SettlementError::DeveloperFrozen
            | SettlementError::DeveloperNotFrozen
            | SettlementError::FreezeUnauthorized
            | SettlementError::WriteRateLimitExceeded
            | SettlementError::InvalidConfigDistinct
            | SettlementError::InvalidConfigAdminContract
            | SettlementError::InvalidConfigVaultContract
            | SettlementError::InvalidUsdcToken
            | SettlementError::InvalidRecipient
            | SettlementError::NoAdminTransferPending
            | SettlementError::InvalidVault
            | SettlementError::NoVaultRotationPending
            | SettlementError::BroadcastMessageTooLong
            | SettlementError::CrossTenantBatch
            | SettlementError::NoUpgradePending
            | SettlementError::ZeroWasmHash
            | SettlementError::UpgradeTimelockNotExpired
            | SettlementError::UnsupportedToken
            | SettlementError::DuplicateRequestId
            | SettlementError::LengthMismatch
            | SettlementError::InvalidCursor
            | SettlementError::TooManyResults => {}
        }
    }
    assert_section_matches(&sections, "Settlement", &settlement_variants, |e| e as u32);

    let revenue_pool_variants = [
        RevenuePoolError::BatchEmpty,
        RevenuePoolError::BatchTooLarge,
        RevenuePoolError::NotInitialized,
        RevenuePoolError::AlreadyInitialized,
        RevenuePoolError::Unauthorized,
        RevenuePoolError::Paused,
        RevenuePoolError::AlreadyPaused,
        RevenuePoolError::NotPaused,
        RevenuePoolError::InvalidUsdcToken,
        RevenuePoolError::NoAdminTransferPending,
        RevenuePoolError::NoPauseGuardian,
        RevenuePoolError::AmountNotPositive,
        RevenuePoolError::AmountExceedsMaxDistribute,
        RevenuePoolError::InvalidRecipient,
        RevenuePoolError::InsufficientBalance,
        RevenuePoolError::DuplicateRecipient,
        RevenuePoolError::Overflow,
        RevenuePoolError::MaxDistributeNotPositive,
        RevenuePoolError::MessageEmpty,
        RevenuePoolError::MessageTooLong,
        RevenuePoolError::NoPendingEmergencyDrain,
        RevenuePoolError::TimelockNotExpired,
        RevenuePoolError::EmergencyPaused,
        RevenuePoolError::AlreadyEmergencyPaused,
        RevenuePoolError::NotEmergencyPaused,
        RevenuePoolError::SharesEmpty,
        RevenuePoolError::InvalidShareSum,
        RevenuePoolError::InvalidShare,
    ];
    for &v in &revenue_pool_variants {
        match v {
            RevenuePoolError::BatchEmpty
            | RevenuePoolError::BatchTooLarge
            | RevenuePoolError::NotInitialized
            | RevenuePoolError::AlreadyInitialized
            | RevenuePoolError::Unauthorized
            | RevenuePoolError::Paused
            | RevenuePoolError::AlreadyPaused
            | RevenuePoolError::NotPaused
            | RevenuePoolError::InvalidUsdcToken
            | RevenuePoolError::NoAdminTransferPending
            | RevenuePoolError::NoPauseGuardian
            | RevenuePoolError::AmountNotPositive
            | RevenuePoolError::AmountExceedsMaxDistribute
            | RevenuePoolError::InvalidRecipient
            | RevenuePoolError::InsufficientBalance
            | RevenuePoolError::DuplicateRecipient
            | RevenuePoolError::Overflow
            | RevenuePoolError::MaxDistributeNotPositive
            | RevenuePoolError::MessageEmpty
            | RevenuePoolError::MessageTooLong
            | RevenuePoolError::NoPendingEmergencyDrain
            | RevenuePoolError::TimelockNotExpired
            | RevenuePoolError::EmergencyPaused
            | RevenuePoolError::AlreadyEmergencyPaused
            | RevenuePoolError::NotEmergencyPaused
            | RevenuePoolError::SharesEmpty
            | RevenuePoolError::InvalidShareSum
            | RevenuePoolError::InvalidShare => {}
        }
    }
    assert_section_matches(
        &sections,
        "Revenue Pool",
        &revenue_pool_variants,
        |e| e as u32,
    );

    let upgrade_variants = [
        UpgradeError::NotInitialized,
        UpgradeError::AlreadyInitialized,
        UpgradeError::Unauthorized,
        UpgradeError::InvalidWasmHash,
        UpgradeError::UpgradeNotAllowed,
        UpgradeError::MigrationPending,
        UpgradeError::TimelockNotExpired,
        UpgradeError::SameWasmHash,
        UpgradeError::SameVersion,
        UpgradeError::InvalidVersion,
        UpgradeError::Overflow,
        UpgradeError::AlreadyUpgraded,
        UpgradeError::StaleNonce,
        UpgradeError::MigrationSameAddress,
        UpgradeError::InvalidMigrationTarget,
        UpgradeError::NoUpgradePending,
        UpgradeError::CooldownNotElapsed,
        UpgradeError::InvalidCooldown,
    ];
    for &v in &upgrade_variants {
        match v {
            UpgradeError::NotInitialized
            | UpgradeError::AlreadyInitialized
            | UpgradeError::Unauthorized
            | UpgradeError::InvalidWasmHash
            | UpgradeError::UpgradeNotAllowed
            | UpgradeError::MigrationPending
            | UpgradeError::TimelockNotExpired
            | UpgradeError::SameWasmHash
            | UpgradeError::SameVersion
            | UpgradeError::InvalidVersion
            | UpgradeError::Overflow
            | UpgradeError::AlreadyUpgraded
            | UpgradeError::StaleNonce
            | UpgradeError::MigrationSameAddress
            | UpgradeError::InvalidMigrationTarget
            | UpgradeError::NoUpgradePending
            | UpgradeError::CooldownNotElapsed
            | UpgradeError::InvalidCooldown => {}
        }
    }
    assert_section_matches(&sections, "Upgrade", &upgrade_variants, |e| e as u32);

    let freeze_variants = [
        FreezeError::NotInitialized,
        FreezeError::AlreadyInitialized,
        FreezeError::Unauthorized,
        FreezeError::AlreadyFrozen,
        FreezeError::NotFrozen,
        FreezeError::Overflow,
    ];
    for &v in &freeze_variants {
        match v {
            FreezeError::NotInitialized
            | FreezeError::AlreadyInitialized
            | FreezeError::Unauthorized
            | FreezeError::AlreadyFrozen
            | FreezeError::NotFrozen
            | FreezeError::Overflow => {}
        }
    }
    assert_section_matches(&sections, "Freeze", &freeze_variants, |e| e as u32);

    let escrow_variants = [
        EscrowError::NotInitialized,
        EscrowError::AlreadyInitialized,
        EscrowError::Unauthorized,
        EscrowError::CooldownActive,
        EscrowError::InvalidCooldown,
        EscrowError::NoPendingAdmin,
        EscrowError::Overflow,
        EscrowError::AssetNotApproved,
        EscrowError::InvalidInput,
        EscrowError::EscrowExists,
        EscrowError::Paused,
        EscrowError::NotFound,
        EscrowError::BatchEmpty,
        EscrowError::BatchTooLarge,
        EscrowError::DuplicateEntry,
    ];
    for &v in &escrow_variants {
        match v {
            EscrowError::NotInitialized
            | EscrowError::AlreadyInitialized
            | EscrowError::Unauthorized
            | EscrowError::CooldownActive
            | EscrowError::InvalidCooldown
            | EscrowError::NoPendingAdmin
            | EscrowError::Overflow
            | EscrowError::AssetNotApproved
            | EscrowError::InvalidInput
            | EscrowError::EscrowExists
            | EscrowError::Paused
            | EscrowError::NotFound
            | EscrowError::BatchEmpty
            | EscrowError::BatchTooLarge
            | EscrowError::DuplicateEntry => {}
        }
    }
    assert_section_matches(&sections, "Escrow", &escrow_variants, |e| e as u32);

    let refund_variants = [
        RefundError::AlreadyInitialized,
        RefundError::NotInitialized,
        RefundError::Unauthorized,
        RefundError::NotFound,
        RefundError::FeeTooHigh,
        RefundError::InvalidAmount,
        RefundError::AmountTooLow,
        RefundError::Overflow,
        RefundError::InvalidStatus,
        RefundError::InsufficientBalance,
        RefundError::InvalidRecipient,
        RefundError::TooManyOpenRequests,
    ];
    for &v in &refund_variants {
        match v {
            RefundError::AlreadyInitialized
            | RefundError::NotInitialized
            | RefundError::Unauthorized
            | RefundError::NotFound
            | RefundError::FeeTooHigh
            | RefundError::InvalidAmount
            | RefundError::AmountTooLow
            | RefundError::Overflow
            | RefundError::InvalidStatus
            | RefundError::InsufficientBalance
            | RefundError::InvalidRecipient
            | RefundError::TooManyOpenRequests => {}
        }
    }
    assert_section_matches(&sections, "Refund", &refund_variants, |e| e as u32);
}

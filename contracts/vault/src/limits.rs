/// Default maximum single-deduction amount.
///
/// When no explicit cap is configured via [`CalloraVault::set_max_deduct`] this
/// value is returned by [`CalloraVault::get_max_deduct`] and used as the upper
/// bound in all deduct/batch_deduct calls.  Setting it to `i128::MAX` is
/// effectively uncapped.
pub const DEFAULT_MAX_DEDUCT: i128 = i128::MAX;

/// Enforce the per-transaction deduction cap.
///
/// Returns `Err(VaultError::ExceedsMaxDeduct)` when `amount > max_deduct`.
///
/// # Arguments
/// * `amount`     – the requested deduction amount; must already be validated as positive.
/// * `max_deduct` – the configured cap (read from storage via `get_max_deduct`).
///
/// # Errors
/// - [`VaultError::ExceedsMaxDeduct`] when `amount > max_deduct`.
#[inline]
pub fn check_max_deduct(amount: i128, max_deduct: i128) -> Result<(), crate::VaultError> {
    if amount > max_deduct {
        return Err(crate::VaultError::ExceedsMaxDeduct);
    }
    Ok(())
}

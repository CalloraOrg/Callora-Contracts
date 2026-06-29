#[cfg(test)]
mod tests {
    use crate::limits::{check_max_deduct, DEFAULT_MAX_DEDUCT};
    use crate::VaultError;

    #[test]
    fn default_max_deduct_is_i128_max() {
        assert_eq!(DEFAULT_MAX_DEDUCT, i128::MAX);
    }

    #[test]
    fn check_max_deduct_amount_at_cap_succeeds() {
        assert!(check_max_deduct(500, 500).is_ok());
    }

    #[test]
    fn check_max_deduct_amount_below_cap_succeeds() {
        assert!(check_max_deduct(1, 1_000).is_ok());
    }

    #[test]
    fn check_max_deduct_amount_above_cap_fails() {
        assert_eq!(
            check_max_deduct(501, 500),
            Err(VaultError::ExceedsMaxDeduct)
        );
    }

    #[test]
    fn check_max_deduct_default_cap_never_rejects_positive_amounts() {
        assert!(check_max_deduct(i128::MAX, DEFAULT_MAX_DEDUCT).is_ok());
    }

    #[test]
    fn check_max_deduct_cap_of_one_accepts_one() {
        assert!(check_max_deduct(1, 1).is_ok());
    }

    #[test]
    fn check_max_deduct_cap_of_one_rejects_two() {
        assert_eq!(check_max_deduct(2, 1), Err(VaultError::ExceedsMaxDeduct));
    }
}

/// Focused tests for [`SettlementError::OverDraft`] (code 23).
///
/// `OverDraft` is returned by `withdraw_developer_balance` when the requested
/// `amount` exceeds the developer's tracked persistent balance. These tests
/// cover the exact boundary, various overdraft amounts, zero-balance state,
/// and confirm that a successful withdrawal after partial credit never triggers
/// the error.
#[cfg(test)]
mod overdraft_tests {
    extern crate std;

    use crate::{CalloraSettlement, CalloraSettlementClient, SettlementError};
    use soroban_sdk::testutils::{Address as _, Ledger as _};
    use soroban_sdk::token as token_mod;
    use soroban_sdk::{Address, Env, Error, InvokeError};

    // ── helpers ──────────────────────────────────────────────────────────────

    fn is_overdraft<V, CE: Into<Error>, E: Into<Error>>(
        result: Result<Result<V, CE>, Result<E, InvokeError>>,
    ) -> bool {
        let expected_code = SettlementError::OverDraft as u32;
        match result {
            Err(Ok(e)) => e.into().get_code() == expected_code,
            _ => false,
        }
    }

    fn create_usdc<'a>(
        env: &'a Env,
        admin: &Address,
    ) -> (
        Address,
        token_mod::Client<'a>,
        token_mod::StellarAssetClient<'a>,
    ) {
        let contract_address = env.register_stellar_asset_contract_v2(admin.clone());
        let address = contract_address.address();
        let client = token_mod::Client::new(env, &address);
        let admin_client = token_mod::StellarAssetClient::new(env, &address);
        (address, client, admin_client)
    }

    // ── OverDraft error code stability ────────────────────────────────────────

    /// `OverDraft` discriminant must be exactly 23. Changing it would be a
    /// breaking change to the on-chain contract interface.
    #[test]
    fn test_overdraft_error_code_is_stable() {
        assert_eq!(
            SettlementError::OverDraft as u32,
            23,
            "OverDraft discriminant must remain 23 — it is part of the public contract interface"
        );
    }

    // ── Exact-balance boundary ────────────────────────────────────────────────

    /// Withdrawal of exactly the tracked balance succeeds (no overdraft).
    #[test]
    fn test_overdraft_exact_balance_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_700_000_000);
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let developer = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let (usdc, _, usdc_admin) = create_usdc(&env, &admin);
        let client = CalloraSettlementClient::new(&env, &addr);

        client.init(&admin, &vault);
        client.set_usdc_token(&admin, &usdc);
        client.receive_payment(&vault, &500i128, &false, &Some(developer.clone()), &usdc);
        usdc_admin.mint(&addr, &500i128);

        let result = client.try_withdraw_developer_balance(&developer, &500i128, &None);
        assert!(result.is_ok(), "exact-balance withdrawal must succeed");
        assert_eq!(client.get_developer_balance(&developer, &usdc), 0i128);
    }

    /// Withdrawal of `balance + 1` returns `OverDraft` (code 23).
    #[test]
    fn test_overdraft_by_one_stroop() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_700_000_000);
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let developer = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let (usdc, _, usdc_admin) = create_usdc(&env, &admin);
        let client = CalloraSettlementClient::new(&env, &addr);

        client.init(&admin, &vault);
        client.set_usdc_token(&admin, &usdc);
        client.receive_payment(&vault, &500i128, &false, &Some(developer.clone()), &usdc);
        usdc_admin.mint(&addr, &500i128);

        let result = client.try_withdraw_developer_balance(&developer, &501i128, &None);
        assert!(
            is_overdraft(result),
            "amount == balance + 1 must return OverDraft (code 23)"
        );
        // Balance is unchanged after rejection.
        assert_eq!(client.get_developer_balance(&developer, &usdc), 500i128);
    }

    // ── Zero-balance overdraft ────────────────────────────────────────────────

    /// Any positive withdrawal from a developer with zero balance returns `OverDraft`.
    #[test]
    fn test_overdraft_zero_balance_any_positive_amount() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_700_000_000);
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let developer = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let (usdc, _, _) = create_usdc(&env, &admin);
        let client = CalloraSettlementClient::new(&env, &addr);

        client.init(&admin, &vault);
        client.set_usdc_token(&admin, &usdc);
        // No credits — developer balance is 0.

        let result = client.try_withdraw_developer_balance(&developer, &1i128, &None);
        assert!(
            is_overdraft(result),
            "withdrawal from zero balance must return OverDraft (code 23)"
        );
    }

    /// `i128::MAX` withdrawal from zero balance returns `OverDraft`, not an
    /// arithmetic overflow, confirming the check is balance-first.
    #[test]
    fn test_overdraft_i128_max_against_zero_balance() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_700_000_000);
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let developer = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let (usdc, _, _) = create_usdc(&env, &admin);
        let client = CalloraSettlementClient::new(&env, &addr);

        client.init(&admin, &vault);
        client.set_usdc_token(&admin, &usdc);

        let result = client.try_withdraw_developer_balance(&developer, &i128::MAX, &None);
        assert!(
            is_overdraft(result),
            "i128::MAX withdrawal from zero balance must return OverDraft (code 23)"
        );
    }

    // ── Large excess ─────────────────────────────────────────────────────────

    /// A withdrawal much larger than the balance returns `OverDraft`.
    #[test]
    fn test_overdraft_large_excess() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_700_000_000);
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let developer = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let (usdc, _, usdc_admin) = create_usdc(&env, &admin);
        let client = CalloraSettlementClient::new(&env, &addr);

        client.init(&admin, &vault);
        client.set_usdc_token(&admin, &usdc);
        client.receive_payment(&vault, &100i128, &false, &Some(developer.clone()), &usdc);
        usdc_admin.mint(&addr, &100i128);

        let result = client.try_withdraw_developer_balance(&developer, &1_000_000i128, &None);
        assert!(
            is_overdraft(result),
            "large excess withdrawal must return OverDraft (code 23)"
        );
        assert_eq!(client.get_developer_balance(&developer, &usdc), 100i128);
    }

    // ── Post-partial-withdrawal overdraft ─────────────────────────────────────

    /// After a partial withdrawal succeeds, a follow-up that exceeds the
    /// remaining balance returns `OverDraft` and leaves the balance intact.
    #[test]
    fn test_overdraft_after_partial_withdrawal() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_700_000_000);
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let developer = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let (usdc, _, usdc_admin) = create_usdc(&env, &admin);
        let client = CalloraSettlementClient::new(&env, &addr);

        client.init(&admin, &vault);
        client.set_usdc_token(&admin, &usdc);
        client.receive_payment(&vault, &300i128, &false, &Some(developer.clone()), &usdc);
        usdc_admin.mint(&addr, &300i128);

        // First withdrawal of 200 succeeds.
        let ok = client.try_withdraw_developer_balance(&developer, &200i128, &None);
        assert!(ok.is_ok(), "partial withdrawal must succeed");
        assert_eq!(client.get_developer_balance(&developer, &usdc), 100i128);

        // Second withdrawal of 101 exceeds remaining 100 → OverDraft.
        let err = client.try_withdraw_developer_balance(&developer, &101i128, &None);
        assert!(
            is_overdraft(err),
            "withdrawal exceeding remaining balance must return OverDraft (code 23)"
        );
        // Remaining balance is still intact.
        assert_eq!(client.get_developer_balance(&developer, &usdc), 100i128);
    }

    // ── State immutability on OverDraft ───────────────────────────────────────

    /// Multiple consecutive overdraft attempts do not corrupt the balance.
    #[test]
    fn test_overdraft_repeated_attempts_do_not_corrupt_balance() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_700_000_000);
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let developer = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let (usdc, _, usdc_admin) = create_usdc(&env, &admin);
        let client = CalloraSettlementClient::new(&env, &addr);

        client.init(&admin, &vault);
        client.set_usdc_token(&admin, &usdc);
        client.receive_payment(&vault, &50i128, &false, &Some(developer.clone()), &usdc);
        usdc_admin.mint(&addr, &50i128);

        // Three overdraft attempts.
        for _ in 0..3 {
            let result = client.try_withdraw_developer_balance(&developer, &51i128, &None);
            assert!(is_overdraft(result), "each overdraft attempt must return OverDraft (code 23)");
        }

        // Balance must still be 50.
        assert_eq!(client.get_developer_balance(&developer, &usdc), 50i128);

        // The exact withdrawal still succeeds after the failed attempts.
        let ok = client.try_withdraw_developer_balance(&developer, &50i128, &None);
        assert!(ok.is_ok(), "exact withdrawal must succeed after overdraft attempts");
        assert_eq!(client.get_developer_balance(&developer, &usdc), 0i128);
    }
}

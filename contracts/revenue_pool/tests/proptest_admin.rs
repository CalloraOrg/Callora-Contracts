//! Property test asserting the revenue pool's admin two-step transfer
//! invariant holds across arbitrary sequences of `set_admin`,
//! `accept_admin`/`claim_admin`, and `cancel_admin_transfer` calls, made by
//! both legitimate and impostor callers.
//!
//! ## Invariant under test
//!
//! The contract's `admin` value only ever changes to an address that was
//! the most recently nominated `pending_admin` via a successful
//! `set_admin` call from the *current* admin, and only via a subsequent
//! successful `accept_admin`/`claim_admin` call from that exact nominee.
//! No other caller, and no other entrypoint, can ever change who the admin
//! is. Likewise, `pending_admin` only ever changes via a successful
//! `set_admin` (by the current admin) or is cleared via a successful
//! `accept_admin`/`claim_admin`/`cancel_admin_transfer` — never anything
//! else.
//!
//! This is checked by maintaining a "shadow" model of the expected admin
//! and pending-admin state purely in the test, updated only when the
//! *real* contract call that would legitimately produce that change is
//! both attempted and does not panic, then asserting the live contract
//! state matches the shadow state after every single action in the
//! sequence.

extern crate std;

use callora_revenue_pool::{RevenuePool, RevenuePoolClient};
use proptest::prelude::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};
use std::panic::{catch_unwind, AssertUnwindSafe};

#[derive(Clone, Copy, Debug)]
enum AdminAction {
    /// Attempt `set_admin(caller, target)` using addresses drawn from the
    /// shared pool by index, so both legitimate and impostor callers (and
    /// both sensible and arbitrary nominees) are exercised.
    SetAdmin {
        caller_idx: usize,
        target_idx: usize,
    },
    /// Attempt `accept_admin(caller)` (the primary entrypoint).
    AcceptAdmin { caller_idx: usize },
    /// Attempt `claim_admin(caller)` (the documented legacy alias for
    /// `accept_admin` - must behave identically).
    ClaimAdmin { caller_idx: usize },
    /// Attempt `cancel_admin_transfer(caller)`.
    CancelTransfer { caller_idx: usize },
}

/// Number of addresses in the shared pool that actions draw callers/targets
/// from. Kept small and fixed so proptest can meaningfully explore
/// "does caller/target happen to be the current admin or pending admin"
/// rather than always generating fresh, never-repeating addresses.
const POOL_SIZE: usize = 4;

fn admin_action_strategy() -> impl Strategy<Value = AdminAction> {
    prop_oneof![
        3 => (0_usize..POOL_SIZE, 0_usize..POOL_SIZE)
            .prop_map(|(caller_idx, target_idx)| AdminAction::SetAdmin { caller_idx, target_idx }),
        3 => (0_usize..POOL_SIZE).prop_map(|caller_idx| AdminAction::AcceptAdmin { caller_idx }),
        2 => (0_usize..POOL_SIZE).prop_map(|caller_idx| AdminAction::ClaimAdmin { caller_idx }),
        2 => (0_usize..POOL_SIZE).prop_map(|caller_idx| AdminAction::CancelTransfer { caller_idx }),
    ]
}

fn create_pool(env: &Env) -> (Address, RevenuePoolClient<'_>) {
    let address = env.register(RevenuePool, ());
    let client = RevenuePoolClient::new(env, &address);
    (address, client)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn admin_transfer_invariant_holds_across_arbitrary_actions(
        actions in prop::collection::vec(admin_action_strategy(), 1..=64)
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let pool_addresses: std::vec::Vec<Address> =
            (0..POOL_SIZE).map(|_| Address::generate(&env)).collect();
        let (_pool_addr, pool) = create_pool(&env);

        // Address at index 0 is the genesis admin.
        pool.init(&pool_addresses[0], &Address::generate(&env));

        // Shadow model of expected contract state, updated only when a
        // real call that should legitimately cause that change both runs
        // and does not panic.
        let mut expected_admin = pool_addresses[0].clone();
        let mut expected_pending: Option<Address> = None;

        for action in actions {
            match action {
                AdminAction::SetAdmin { caller_idx, target_idx } => {
                    let caller = &pool_addresses[caller_idx % pool_addresses.len()];
                    let target = &pool_addresses[target_idx % pool_addresses.len()];
                    let is_authorized = *caller == expected_admin;

                    let result = catch_unwind(AssertUnwindSafe(|| {
                        pool.set_admin(caller, target);
                    }));

                    if is_authorized {
                        prop_assert!(
                            result.is_ok(),
                            "the current admin must always be able to call set_admin"
                        );
                        expected_pending = Some(target.clone());
                    } else {
                        prop_assert!(
                            result.is_err(),
                            "a non-admin caller must never be able to call set_admin successfully"
                        );
                    }
                }
                AdminAction::AcceptAdmin { caller_idx } | AdminAction::ClaimAdmin { caller_idx } => {
                    let caller = &pool_addresses[caller_idx % pool_addresses.len()];
                    let is_authorized =
                        expected_pending.as_ref().is_some_and(|pending| pending == caller);

                    let result = catch_unwind(AssertUnwindSafe(|| {
                        if matches!(action, AdminAction::ClaimAdmin { .. }) {
                            pool.claim_admin(caller);
                        } else {
                            pool.accept_admin(caller);
                        }
                    }));

                    if is_authorized {
                        prop_assert!(
                            result.is_ok(),
                            "the exact nominated pending admin must always be able to accept/claim admin"
                        );
                        expected_admin = caller.clone();
                        expected_pending = None;
                    } else {
                        prop_assert!(
                            result.is_err(),
                            "only the exact nominated pending admin may ever accept/claim admin \
                             (caller was not the pending admin, or no transfer was pending)"
                        );
                    }
                }
                AdminAction::CancelTransfer { caller_idx } => {
                    let caller = &pool_addresses[caller_idx % pool_addresses.len()];
                    let is_authorized = *caller == expected_admin && expected_pending.is_some();

                    let result = catch_unwind(AssertUnwindSafe(|| {
                        pool.cancel_admin_transfer(caller);
                    }));

                    if is_authorized {
                        prop_assert!(
                            result.is_ok(),
                            "the current admin must always be able to cancel a pending transfer"
                        );
                        expected_pending = None;
                    } else {
                        prop_assert!(
                            result.is_err(),
                            "cancel_admin_transfer must fail for non-admin callers or when \
                             no transfer is pending"
                        );
                    }
                }
            }

            // Core invariant: after every single action, the live contract
            // state must exactly match the shadow model - regardless of
            // which actions succeeded or panicked above.
            prop_assert_eq!(
                pool.get_admin(),
                expected_admin.clone(),
                "live admin diverged from the expected shadow admin"
            );
            prop_assert_eq!(
                pool.get_pending_admin(),
                expected_pending.clone(),
                "live pending_admin diverged from the expected shadow pending_admin"
            );
        }
    }
}

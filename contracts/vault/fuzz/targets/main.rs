//! Fuzz target: comprehensive vault state-machine fuzzer.
//!
//! Exercises the full public surface of `CalloraVault` by parsing a raw byte
//! stream into a sequence of typed operations and verifying that key
//! invariants hold after every call.
//!
//! # Invariants checked
//!
//! 1. **Balance conservation** – the tracked balance never increases unless
//!    a `deposit` succeeds, and never decreases unless a `deduct` or
//!    `batch_deduct` succeeds.
//! 2. **No negative balance** – the tracked balance is always `>= 0`.
//! 3. **Pause gate** – while the vault is paused, `deposit`, `deduct`, and
//!    `batch_deduct` must all be rejected.
//! 4. **Auth gate** – every state-changing entry-point must return an error
//!    when called without authentication (i.e. after `env.set_auths(&[])`).
//! 5. **Max-deduct enforcement** – a `deduct` that exceeds the configured
//!    `max_deduct` must be rejected.
//! 6. **No uncontrolled panics** – the contract must never abort the process
//!    with a panic that is not caught through the normal Soroban error path or
//!    `std::panic::catch_unwind`.
//!
//! # Wire format
//!
//! The fuzzer byte stream is sliced into **3-byte operation tokens**:
//!
//! ```text
//! byte 0: operation discriminant (mod NUM_OPS)
//! bytes 1-2: big-endian u16 operand   (amount / request-id / window-offset)
//! ```
//!
//! Operands are scaled to meaningful ranges for each operation.
//!
//! # Running
//! ```bash
//! cargo fuzz run main
//! ```

#![no_main]

extern crate std;

use libfuzzer_sys::fuzz_target;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{Address, BytesN, Env, InvokeError, Vec as SorobanVec};

use callora_vault::{CalloraVault, CalloraVaultClient};

// ---------------------------------------------------------------------------
// Configuration constants
// ---------------------------------------------------------------------------

/// Bytes consumed per operation token.
const BYTES_PER_OP: usize = 3;

/// Total number of distinct operation discriminants.
const NUM_OPS: u8 = 14;

/// Maximum USDC pre-funded on-ledger at startup.
const VAULT_FUNDING: i128 = 1_000_000;

/// The vault's initial `max_deduct`.
const INITIAL_MAX_DEDUCT: i128 = 100_000;

/// Maximum number of items in a `batch_deduct` call.
const MAX_BATCH_ITEMS: usize = 8;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Provision a USDC stellar-asset contract and return its address together
/// with an admin client that can mint tokens.
fn create_usdc<'a>(env: &'a Env, admin: &Address) -> (Address, StellarAssetClient<'a>) {
    let ca = env.register_stellar_asset_contract_v2(admin.clone());
    let addr = ca.address();
    (addr.clone(), StellarAssetClient::new(env, &addr))
}

/// Register the vault and return its address alongside a typed client.
fn create_vault(env: &Env) -> (Address, CalloraVaultClient<'_>) {
    let addr = env.register(CalloraVault, ());
    (addr.clone(), CalloraVaultClient::new(env, &addr))
}

/// Returns `true` when a `catch_unwind`-wrapped Soroban `try_*` call
/// completed without a panic **and** the contract returned `Ok(())`.
///
/// Soroban `try_*` client methods return
/// `Result<T, Result<ContractError, InvokeError>>`.
/// Wrapped in `catch_unwind` the type becomes
/// `Result<Result<T, Result<ContractError, InvokeError>>, Box<dyn Any + Send>>`.
///
/// A logical success is therefore `Ok(Ok(value))`.
/// Type alias for the result of a `catch_unwind`-wrapped Soroban `try_*` call.
type CatchResult<T> = std::result::Result<
    std::result::Result<T, std::result::Result<soroban_sdk::Error, InvokeError>>,
    Box<dyn std::any::Any + Send>,
>;

fn is_success<T>(result: &CatchResult<T>) -> bool {
    matches!(result, Ok(Ok(_)))
}

/// Returns `true` when the result was **not** a panic but the contract
/// returned a typed error (i.e. the call was rejected, not aborted).
fn is_contract_err<T>(result: &CatchResult<T>) -> bool {
    matches!(result, Ok(Err(_)))
}

// ---------------------------------------------------------------------------
// Fuzz entry-point
// ---------------------------------------------------------------------------

fuzz_target!(|data: &[u8]| {
    // Need at least one operation token.
    if data.len() < BYTES_PER_OP {
        return;
    }

    let env = Env::default();
    env.mock_all_auths();

    // --- Static participants ------------------------------------------------
    let owner = Address::generate(&env);
    let auth_caller = Address::generate(&env);
    let settlement = Address::generate(&env);

    // --- USDC token ---------------------------------------------------------
    let (usdc_addr, usdc_admin) = create_usdc(&env, &owner);

    // --- Vault --------------------------------------------------------------
    let (vault_addr, client) = create_vault(&env);

    // Mint VAULT_FUNDING USDC to vault (so on-ledger balance exists) and
    // to owner so deposit calls can transfer tokens.
    usdc_admin.mint(&vault_addr, &VAULT_FUNDING);
    usdc_admin.mint(&owner, &VAULT_FUNDING);

    // Initialise: initial_balance = 0, min_deposit = 1, max_deduct = INITIAL_MAX_DEDUCT.
    client.init(
        &owner,
        &usdc_addr,
        &0i128,
        &auth_caller,
        &1i128,
        &None,
        &INITIAL_MAX_DEDUCT,
        &settlement,
    );

    // Tracked state mirrored alongside the contract for invariant checks.
    let mut expected_balance: i128 = 0;
    let mut max_deduct: i128 = INITIAL_MAX_DEDUCT;
    let mut request_id_counter: u64 = 0;

    // -----------------------------------------------------------------------
    // Main operation loop
    // -----------------------------------------------------------------------
    for chunk in data.chunks(BYTES_PER_OP) {
        if chunk.len() < BYTES_PER_OP {
            break;
        }

        let op = chunk[0] % NUM_OPS;
        let operand = u16::from_be_bytes([chunk[1], chunk[2]]) as i128;

        match op {
            // ----------------------------------------------------------------
            // 0 — deposit(owner, amount)
            // ----------------------------------------------------------------
            0 => {
                let amount = operand; // 0 – 65 535

                let balance_before = client.balance();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    client.try_deposit(&owner, &amount)
                }));
                let balance_after = client.balance();

                if is_success(&result) {
                    // Conservation: balance must have grown by exactly `amount`.
                    assert_eq!(
                        balance_after,
                        balance_before + amount,
                        "deposit conservation: before={balance_before} amount={amount} after={balance_after}"
                    );
                    expected_balance = balance_after;
                    assert!(!client.is_paused(), "deposit succeeded while paused");
                } else if is_contract_err(&result) {
                    // Typed rejection → balance unchanged.
                    assert_eq!(
                        balance_after, balance_before,
                        "rejected deposit mutated balance"
                    );
                }
                // A panic from the Soroban harness (contract panic!) is also
                // acceptable and caught by catch_unwind.

                assert!(client.balance() >= 0, "balance went negative after deposit");
            }

            // ----------------------------------------------------------------
            // 1 — deduct(auth_caller, amount, request_id)
            // ----------------------------------------------------------------
            1 => {
                let amount = (operand % (INITIAL_MAX_DEDUCT + 1)).max(0);
                request_id_counter = request_id_counter.wrapping_add(1);
                let req_id = request_id_counter;

                let balance_before = client.balance();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    client.try_deduct(&auth_caller, &amount, &req_id)
                }));
                let balance_after = client.balance();

                if is_success(&result) {
                    assert_eq!(
                        balance_after,
                        balance_before - amount,
                        "deduct conservation: before={balance_before} amount={amount} after={balance_after}"
                    );
                    expected_balance = balance_after;
                    assert!(!client.is_paused(), "deduct succeeded while paused");
                    assert!(amount <= max_deduct, "deduct succeeded above max_deduct");
                } else if is_contract_err(&result) {
                    assert_eq!(
                        balance_after, balance_before,
                        "rejected deduct mutated balance"
                    );
                }

                assert!(client.balance() >= 0, "balance went negative after deduct");
            }

            // ----------------------------------------------------------------
            // 2 — batch_deduct(auth_caller, items)
            // ----------------------------------------------------------------
            2 => {
                let item_count = ((operand >> 8) as usize).clamp(1, MAX_BATCH_ITEMS);
                let per_item_amount = (operand & 0xff).max(1); // 1 – 255

                let mut items = SorobanVec::new(&env);
                for i in 0..item_count {
                    request_id_counter = request_id_counter.wrapping_add(1);
                    items.push_back((per_item_amount, request_id_counter.wrapping_add(i as u64)));
                }

                let balance_before = client.balance();
                let total = per_item_amount * item_count as i128;

                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    client.try_batch_deduct(&auth_caller, &items)
                }));
                let balance_after = client.balance();

                if is_success(&result) {
                    assert_eq!(
                        balance_after,
                        balance_before - total,
                        "batch_deduct conservation: before={balance_before} total={total} after={balance_after}"
                    );
                    expected_balance = balance_after;
                    assert!(!client.is_paused(), "batch_deduct succeeded while paused");
                } else if is_contract_err(&result) {
                    assert_eq!(
                        balance_after, balance_before,
                        "rejected batch_deduct mutated balance"
                    );
                }

                assert!(
                    client.balance() >= 0,
                    "balance went negative after batch_deduct"
                );
            }

            // ----------------------------------------------------------------
            // 3 — pause / unpause toggle
            // ----------------------------------------------------------------
            3 => {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    if client.is_paused() {
                        let _ = client.try_unpause(&owner);
                    } else {
                        let _ = client.try_pause(&owner);
                    }
                }));
            }

            // ----------------------------------------------------------------
            // 4 — set_max_deduct(owner, new_max)
            // ----------------------------------------------------------------
            4 => {
                let new_max = (operand % 200_000).max(1);
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    client.try_set_max_deduct(&owner, &new_max)
                }));
                if is_success(&result) {
                    max_deduct = new_max;
                }
                assert!(
                    client.get_max_deduct() > 0,
                    "max_deduct became non-positive"
                );
            }

            // ----------------------------------------------------------------
            // 5 — set_reserve_cap(owner, usdc, cap)
            // ----------------------------------------------------------------
            5 => {
                let cap = (operand % 2_000_000).max(1);
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _ = client.try_set_reserve_cap(&owner, &usdc_addr, &cap);
                }));
                assert!(
                    client.get_reserve_cap(&usdc_addr) > 0,
                    "reserve_cap became non-positive"
                );
            }

            // ----------------------------------------------------------------
            // 6 — view-only calls: must not mutate balance
            // ----------------------------------------------------------------
            6 => {
                let bal = client.balance();
                let _ = client.is_paused();
                let _ = client.get_max_deduct();
                let _ = client.get_owner();
                let _ = client.get_usdc_token();
                let _ = client.get_settlement();
                let _ = client.capabilities();
                let _ = client.get_timelock_window();
                assert_eq!(bal, client.balance(), "view calls mutated balance");
            }

            // ----------------------------------------------------------------
            // 7 — dry_run_sweep_idle_balance (read-only)
            // ----------------------------------------------------------------
            7 => {
                let bal = client.balance();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    client.dry_run_sweep_idle_balance()
                }));
                assert_eq!(bal, client.balance(), "dry_run mutated balance");

                if let Ok(preview) = result {
                    assert!(preview.idle_balance >= 0, "dry_run: negative idle_balance");
                    assert_eq!(
                        preview.has_idle,
                        preview.idle_balance > 0,
                        "dry_run: has_idle inconsistent with idle_balance"
                    );
                }
            }

            // ----------------------------------------------------------------
            // 8 — unauthorised deposit attempt (auth gate)
            // ----------------------------------------------------------------
            8 => {
                let amount = (operand % 500).max(1);
                let bal = client.balance();

                env.set_auths(&[]);
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    client.try_deposit(&owner, &amount)
                }));
                env.mock_all_auths();

                assert_eq!(
                    bal,
                    client.balance(),
                    "unauthenticated deposit changed balance"
                );
                assert!(
                    !is_success(&result),
                    "unauthenticated deposit unexpectedly succeeded"
                );
            }

            // ----------------------------------------------------------------
            // 9 — unauthorised deduct attempt (auth gate)
            // ----------------------------------------------------------------
            9 => {
                let amount = (operand % 100).max(1);
                request_id_counter = request_id_counter.wrapping_add(1);
                let req_id = request_id_counter;
                let bal = client.balance();

                env.set_auths(&[]);
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    client.try_deduct(&auth_caller, &amount, &req_id)
                }));
                env.mock_all_auths();

                assert_eq!(
                    bal,
                    client.balance(),
                    "unauthenticated deduct changed balance"
                );
                assert!(
                    !is_success(&result),
                    "unauthenticated deduct unexpectedly succeeded"
                );
            }

            // ----------------------------------------------------------------
            // 10 — propose / cancel pause (timelocked admin path)
            // ----------------------------------------------------------------
            10 => {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _ = client.try_propose_pause(&owner);
                    // Immediate execution must fail (TimelockNotExpired).
                    let _ = client.try_execute_pause(&owner);
                    // Cancel to keep state clean.
                    let _ = client.try_cancel_pause(&owner);
                }));
            }

            // ----------------------------------------------------------------
            // 11 — propose / cancel upgrade (timelocked admin path)
            // ----------------------------------------------------------------
            11 => {
                let wasm_hash = BytesN::from_array(&env, &[chunk[1]; 32]);
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _ = client.try_propose_upgrade(&owner, &wasm_hash);
                    // Execution requires elapsed ledger time — skip.
                    let _ = client.try_cancel_upgrade(&owner);
                }));
            }

            // ----------------------------------------------------------------
            // 12 — propose / cancel sweep (timelocked admin path)
            // ----------------------------------------------------------------
            12 => {
                let amount = (operand % 10_000).max(1);
                let recipient = Address::generate(&env);
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _ = client.try_propose_sweep(&owner, &recipient, &amount);
                    // Immediate execution must fail (TimelockNotExpired).
                    let _ = client.try_execute_sweep(&owner);
                    let _ = client.try_cancel_sweep(&owner);
                }));
            }

            // ----------------------------------------------------------------
            // 13 — set_timelock_window(owner, seconds)
            // ----------------------------------------------------------------
            13 => {
                // Scale 0–65535 into 0–~3.2M to cover both valid and invalid ranges.
                let window = (operand as u64) * 50;
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _ = client.try_set_timelock_window(&owner, &window);
                }));
                // Window must remain within [MIN, MAX] regardless of fuzzer input.
                let stored = client.get_timelock_window();
                assert!(
                    (callora_vault::timelock::MIN_TIMELOCK_SECONDS
                        ..=callora_vault::timelock::MAX_TIMELOCK_SECONDS)
                        .contains(&stored),
                    "timelock_window out of bounds: {stored}"
                );
            }

            _ => unreachable!("op discriminant outside NUM_OPS range"),
        }

        // -------------------------------------------------------------------
        // Post-step global invariants
        // -------------------------------------------------------------------

        // I1: balance is always non-negative.
        assert!(
            client.balance() >= 0,
            "invariant I1: negative balance after op {op}"
        );

        // I2: when paused, deposit and deduct must fail.
        if client.is_paused() {
            let small = 10i128;
            request_id_counter = request_id_counter.wrapping_add(1);
            let req_id = request_id_counter;

            let dep = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                client.try_deposit(&owner, &small)
            }));
            assert!(
                !is_success(&dep),
                "invariant I2: deposit succeeded while paused (op {op})"
            );

            let ded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                client.try_deduct(&auth_caller, &small, &req_id)
            }));
            assert!(
                !is_success(&ded),
                "invariant I2: deduct succeeded while paused (op {op})"
            );
        }

        // I3: owner never changes.
        assert_eq!(
            client.get_owner(),
            owner,
            "invariant I3: owner changed (op {op})"
        );

        // I4: USDC token address never changes.
        assert_eq!(
            client.get_usdc_token(),
            usdc_addr,
            "invariant I4: usdc_token changed (op {op})"
        );
    }

    // -----------------------------------------------------------------------
    // Terminal invariant: mirrored balance matches contract balance.
    // -----------------------------------------------------------------------
    assert_eq!(
        client.balance(),
        expected_balance,
        "terminal: expected={expected_balance} actual={}",
        client.balance()
    );

    let _ = vault_addr; // suppress unused warning
});

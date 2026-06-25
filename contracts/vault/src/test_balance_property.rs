//! Property-based test: random vault operation sequences preserve
//! `VaultMeta.balance == on-ledger USDC` after every step.
//!
//! # Invariant
//! After each operation (successful or expected failure), the vault's tracked
//! `meta.balance` must equal `token::Client::balance(vault_address)`.
//!
//! # Generator
//! A deterministic LCG PRNG (no `std`, no `rand` crate) drives 64 seeded traces
//! (`seed` 0..=63) of length 32.  Operations include deposit, deduct,
//! batch_deduct, withdraw, withdraw_to, distribute, plus edge-case drivers:
//! pause/unpause, allowed-depositor toggles, and request-id reuse.
//!
//! # `distribute` and surplus
//! `distribute` moves on-ledger USDC without updating `meta.balance` (it is for
//! untracked surplus recovery).  When the generator picks `distribute`, it injects
//! surplus via direct mint if needed, then distributes the **entire** surplus so
//! `meta.balance` and on-ledger USDC remain equal after the step.
//!
//! # Reproduction
//! On failure the full step trace is printed so the failing seed and operation
//! sequence can be replayed trivially.

extern crate std;

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{token, Address, Env, Symbol, Vec};

use super::{CalloraVault, CalloraVaultClient, DeductItem};

use callora_settlement::CalloraSettlement;

/// Minimum number of operations per seeded trace (acceptance criteria).
const TRACE_LENGTH: u32 = 32;

/// Number of deterministic seeds: 0..=SEED_COUNT-1 (acceptance: 64 traces).
const SEED_COUNT: u64 = 64;

/// Starting tracked balance and on-ledger USDC for each trace.
const INITIAL_BALANCE: i128 = 100_000;

/// Per-operation amount ceiling (keeps traces fast and amounts realistic).
const AMOUNT_CAP: i128 = 5_000;

// ---------------------------------------------------------------------------
// Deterministic PRNG (no std / no rand crate)
// ---------------------------------------------------------------------------

/// 64-bit LCG — same family as glibc; fully deterministic from `seed`.
struct Prng {
    state: u64,
}

impl Prng {
    fn new(seed: u64) -> Self {
        // Avoid zero state which would stall the LCG.
        Self {
            state: seed.wrapping_add(0x9E37_79B9_7F4A_7C15),
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        self.state
    }

    /// Inclusive range for `i128` bounds that fit comfortably in `u64`.
    fn gen_range_i128(&mut self, min: i128, max: i128) -> i128 {
        if min >= max {
            return min;
        }
        let span = (max - min) as u64 + 1;
        min + (self.next_u64() % span) as i128
    }

    fn gen_range_usize(&mut self, min: usize, max: usize) -> usize {
        if min >= max {
            return min;
        }
        min + (self.next_u64() as usize) % (max - min)
    }

    fn next_bool(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }
}

// ---------------------------------------------------------------------------
// Trace recording (printed on invariant violation)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct TraceStep {
    index: u32,
    label: &'static str,
    detail: std::string::String,
}

struct Trace {
    seed: u64,
    steps: std::vec::Vec<TraceStep>,
}

impl Trace {
    fn new(seed: u64) -> Self {
        Self {
            seed,
            steps: std::vec::Vec::new(),
        }
    }

    fn push(&mut self, index: u32, label: &'static str, detail: impl Into<std::string::String>) {
        self.steps.push(TraceStep {
            index,
            label,
            detail: detail.into(),
        });
    }

    fn panic_invariant(&self, step: u32, meta: i128, ledger: i128) -> ! {
        let mut msg = std::format!(
            "INVARIANT VIOLATION: meta.balance ({meta}) != on-ledger USDC ({ledger})\n\
             seed={} step={step}\n\
             --- trace ---\n",
            self.seed
        );
        for s in &self.steps {
            msg.push_str(&std::format!("  [{}] {} — {}\n", s.index, s.label, s.detail));
        }
        panic!("{msg}");
    }
}

// ---------------------------------------------------------------------------
// Harness helpers
// ---------------------------------------------------------------------------

fn create_usdc<'a>(
    env: &'a Env,
    admin: &Address,
) -> (Address, token::Client<'a>, token::StellarAssetClient<'a>) {
    let ca = env.register_stellar_asset_contract_v2(admin.clone());
    let addr = ca.address();
    (
        addr.clone(),
        token::Client::new(env, &addr),
        token::StellarAssetClient::new(env, &addr),
    )
}

fn create_vault(env: &Env) -> (Address, CalloraVaultClient<'_>) {
    let addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(env, &addr);
    (addr, client)
}

fn create_settlement(env: &Env, admin: &Address, vault_address: &Address) -> Address {
    let settlement_address = env.register(CalloraSettlement, ());
    let settlement_client =
        callora_settlement::CalloraSettlementClient::new(env, &settlement_address);
    settlement_client.init(admin, vault_address);
    settlement_address
}

/// Assert `meta.balance == token::Client::balance(vault)`; panic with trace on mismatch.
fn assert_balance_in_sync(
    client: &CalloraVaultClient<'_>,
    usdc: &token::Client<'_>,
    vault_addr: &Address,
    trace: &Trace,
    step: u32,
) {
    let meta = client.balance();
    let ledger = usdc.balance(vault_addr);
    if meta != ledger {
        trace.panic_invariant(step, meta, ledger);
    }
}

/// Static pool of request-id symbols (Soroban `Symbol::new` requires `&'static str`).
const RID_POOL: [&str; 64] = [
    "rid00", "rid01", "rid02", "rid03", "rid04", "rid05", "rid06", "rid07", "rid08", "rid09",
    "rid10", "rid11", "rid12", "rid13", "rid14", "rid15", "rid16", "rid17", "rid18", "rid19",
    "rid20", "rid21", "rid22", "rid23", "rid24", "rid25", "rid26", "rid27", "rid28", "rid29",
    "rid30", "rid31", "rid32", "rid33", "rid34", "rid35", "rid36", "rid37", "rid38", "rid39",
    "rid40", "rid41", "rid42", "rid43", "rid44", "rid45", "rid46", "rid47", "rid48", "rid49",
    "rid50", "rid51", "rid52", "rid53", "rid54", "rid55", "rid56", "rid57", "rid58", "rid59",
    "rid60", "rid61", "rid62", "rid63",
];

/// Return a request-id symbol from the static pool (indexed by a monotonic counter).
fn make_request_id(env: &Env, counter: u32) -> Symbol {
    Symbol::new(env, RID_POOL[(counter as usize) % RID_POOL.len()])
}

// ---------------------------------------------------------------------------
// Operation kinds (generator alphabet)
// ---------------------------------------------------------------------------

#[repr(u8)]
enum OpKind {
    Deposit = 0,
    Deduct = 1,
    BatchDeduct = 2,
    Withdraw = 3,
    WithdrawTo = 4,
    Distribute = 5,
    PauseToggle = 6,
    DepositorFlip = 7,
    RequestIdRetry = 8,
}

const OP_COUNT: u64 = 9;

// ---------------------------------------------------------------------------
// Core property runner
// ---------------------------------------------------------------------------

/// Run one deterministic trace of `TRACE_LENGTH` steps for `seed`.
fn run_property_trace(seed: u64) {
    let env = Env::default();
    env.mock_all_auths();

    let mut trace = Trace::new(seed);
    let mut rng = Prng::new(seed);

    let owner = Address::generate(&env);
    let depositor = Address::generate(&env);
    let recipient = Address::generate(&env);
    let (vault_addr, client) = create_vault(&env);
    let (usdc_addr, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    usdc_admin.mint(&vault_addr, &INITIAL_BALANCE);
    client.init(
        &owner,
        &usdc_addr,
        &Some(INITIAL_BALANCE),
        &None,
        &Some(1),
        &None,
        &Some(AMOUNT_CAP),
    );
    let settlement = create_settlement(&env, &owner, &vault_addr);
    client.set_settlement(&owner, &settlement);

    // Fund depositors and approve the vault for pulls.
    let reserve: i128 = INITIAL_BALANCE * 10;
    usdc_admin.mint(&owner, &reserve);
    usdc_admin.mint(&depositor, &reserve);
    usdc_client.approve(&owner, &vault_addr, &i128::MAX, &999_999);
    usdc_client.approve(&depositor, &vault_addr, &i128::MAX, &999_999);

    let mut paused = false;
    let mut depositor_allowed = false;
    // Track request_ids consumed by successful deducts for reuse attempts.
    let mut used_request_ids: std::vec::Vec<Symbol> = std::vec::Vec::new();
    let mut rid_counter: u32 = 0;

    assert_balance_in_sync(&client, &usdc_client, &vault_addr, &trace, 0);

    for step in 1..=TRACE_LENGTH {
        let op = (rng.next_u64() % OP_COUNT) as u8;
        let balance_before = client.balance();

        match op {
            x if x == OpKind::Deposit as u8 => {
                let use_alt = depositor_allowed && rng.next_bool();
                let who = if use_alt {
                    depositor.clone()
                } else {
                    owner.clone()
                };
                let max_amt = AMOUNT_CAP.min(balance_before + reserve);
                let amount = rng.gen_range_i128(1, max_amt);
                if paused {
                    let result = client.try_deposit(&who, &amount);
                    trace.push(
                        step,
                        "deposit (paused, expect fail)",
                        std::format!("caller={use_alt:?} amount={amount} result={result:?}"),
                    );
                    assert!(result.is_err(), "deposit must fail while paused");
                } else {
                    client.deposit(&who, &amount);
                    trace.push(
                        step,
                        "deposit",
                        std::format!("caller={} amount={amount}", if use_alt { "alt" } else { "owner" }),
                    );
                }
            }

            x if x == OpKind::Deduct as u8 => {
                let amount = rng.gen_range_i128(1, AMOUNT_CAP);
                let with_id = rng.next_bool();
                let rid = if with_id {
                    let id = make_request_id(&env, rid_counter);
                    rid_counter += 1;
                    Some(id)
                } else {
                    None
                };
                if paused {
                    let result = client.try_deduct(&owner, &amount, &rid);
                    trace.push(
                        step,
                        "deduct (paused, expect fail)",
                        std::format!("amount={amount} rid={with_id:?} result={result:?}"),
                    );
                    assert!(result.is_err());
                } else if balance_before >= amount {
                    client.deduct(&owner, &amount, &rid);
                    if let Some(ref id) = rid {
                        used_request_ids.push(id.clone());
                    }
                    trace.push(
                        step,
                        "deduct",
                        std::format!("amount={amount} rid={with_id:?}"),
                    );
                } else {
                    let result = client.try_deduct(&owner, &amount, &rid);
                    trace.push(
                        step,
                        "deduct (insufficient, expect fail)",
                        std::format!("amount={amount} result={result:?}"),
                    );
                    assert!(result.is_err());
                }
            }

            x if x == OpKind::BatchDeduct as u8 => {
                let n = rng.gen_range_usize(1, 5);
                let mut items = Vec::new(&env);
                let mut total: i128 = 0;
                let mut valid = true;
                for _sub in 0..n {
                    let amt = rng.gen_range_i128(1, AMOUNT_CAP);
                    total = match total.checked_add(amt) {
                        Some(t) => t,
                        None => {
                            valid = false;
                            break;
                        }
                    };
                    let use_id = rng.next_bool();
                    let rid = if use_id {
                        let id = make_request_id(&env, rid_counter);
                        rid_counter += 1;
                        Some(id)
                    } else {
                        None
                    };
                    items.push_back(DeductItem {
                        amount: amt,
                        request_id: rid,
                    });
                }
                if paused {
                    let result = client.try_batch_deduct(&owner, &items);
                    trace.push(
                        step,
                        "batch_deduct (paused, expect fail)",
                        std::format!("n={n} result={result:?}"),
                    );
                    assert!(result.is_err());
                } else if valid && balance_before >= total {
                    client.batch_deduct(&owner, &items);
                    for item in items.iter() {
                        if let Some(ref id) = item.request_id {
                            used_request_ids.push(id.clone());
                        }
                    }
                    trace.push(step, "batch_deduct", std::format!("n={n} total={total}"));
                } else {
                    let result = client.try_batch_deduct(&owner, &items);
                    trace.push(
                        step,
                        "batch_deduct (expect fail)",
                        std::format!("n={n} total={total} result={result:?}"),
                    );
                    assert!(result.is_err());
                }
            }

            x if x == OpKind::Withdraw as u8 => {
                if balance_before > 0 {
                    let amount = rng.gen_range_i128(1, balance_before.min(AMOUNT_CAP));
                    client.withdraw(&amount);
                    trace.push(step, "withdraw", std::format!("amount={amount}"));
                } else {
                    trace.push(step, "withdraw (skipped)", "balance=0");
                }
            }

            x if x == OpKind::WithdrawTo as u8 => {
                if balance_before > 0 {
                    let amount = rng.gen_range_i128(1, balance_before.min(AMOUNT_CAP));
                    client.withdraw_to(&recipient, &amount);
                    trace.push(step, "withdraw_to", std::format!("amount={amount}"));
                } else {
                    trace.push(step, "withdraw_to (skipped)", "balance=0");
                }
            }

            x if x == OpKind::Distribute as u8 => {
                let ledger = usdc_client.balance(&vault_addr);
                let meta = client.balance();
                let mut surplus = ledger.saturating_sub(meta);
                // `distribute` only moves untracked surplus; inject some if none exists.
                if surplus == 0 {
                    surplus = rng.gen_range_i128(1, AMOUNT_CAP);
                    usdc_admin.mint(&vault_addr, &surplus);
                    trace.push(
                        step,
                        "distribute_setup",
                        std::format!("injected_surplus={surplus}"),
                    );
                }
                // Distribute the full surplus so meta.balance stays equal to on-ledger USDC.
                client.distribute(&owner, &recipient, &surplus);
                trace.push(
                    step,
                    "distribute",
                    std::format!("amount={surplus}"),
                );
            }

            x if x == OpKind::PauseToggle as u8 => {
                if paused {
                    client.unpause(&owner);
                    paused = false;
                    trace.push(step, "unpause", "");
                } else {
                    client.pause(&owner);
                    paused = true;
                    trace.push(step, "pause", "");
                }
            }

            x if x == OpKind::DepositorFlip as u8 => {
                if depositor_allowed {
                    client.clear_allowed_depositors(&owner);
                    depositor_allowed = false;
                    trace.push(step, "clear_allowed_depositors", "");
                } else {
                    client.set_allowed_depositor(&owner, &Some(depositor.clone()));
                    depositor_allowed = true;
                    trace.push(
                        step,
                        "set_allowed_depositor",
                        std::format!("depositor={depositor:?}"),
                    );
                }
            }

            x if x == OpKind::RequestIdRetry as u8 => {
                if used_request_ids.is_empty() {
                    // No prior id — perform a fresh deduct with id, then retry.
                    let amount = rng.gen_range_i128(1, AMOUNT_CAP.min(balance_before.max(1)));
                    if !paused && balance_before >= amount {
                        let rid = make_request_id(&env, rid_counter);
                        rid_counter += 1;
                        client.deduct(&owner, &amount, &Some(rid.clone()));
                        let retry = client.try_deduct(&owner, &amount, &Some(rid.clone()));
                        trace.push(
                            step,
                            "request_id_reuse",
                            std::format!("amount={amount} first=ok retry={retry:?}"),
                        );
                        assert!(retry.is_err(), "duplicate request_id must fail");
                        used_request_ids.push(rid);
                    } else {
                        trace.push(step, "request_id_reuse (skipped)", "preconditions");
                    }
                } else {
                    let idx = rng.gen_range_usize(0, used_request_ids.len());
                    let rid = used_request_ids[idx].clone();
                    let amount = rng.gen_range_i128(1, AMOUNT_CAP);
                    let retry = client.try_deduct(&owner, &amount, &Some(rid.clone()));
                    trace.push(
                        step,
                        "request_id_reuse",
                        std::format!("rid=reused amount={amount} retry={retry:?}"),
                    );
                    assert!(retry.is_err(), "reused request_id must fail");
                }
            }

            _ => unreachable!(),
        }

        assert_balance_in_sync(&client, &usdc_client, &vault_addr, &trace, step);
    }

    if paused {
        client.unpause(&owner);
        assert_balance_in_sync(&client, &usdc_client, &vault_addr, &trace, TRACE_LENGTH + 1);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Run all 64 deterministic seeded traces (seeds 0..=63), each of length 32.
#[test]
fn test_balance_property_seeded_traces() {
    for seed in 0..SEED_COUNT {
        run_property_trace(seed);
    }
}

/// Edge case: trace with forced pause at midpoint (seed 42, pause injected at step 16).
#[test]
fn test_balance_property_pause_mid_sequence() {
    let env = Env::default();
    env.mock_all_auths();

    let owner = Address::generate(&env);
    let (vault_addr, client) = create_vault(&env);
    let (usdc_addr, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    usdc_admin.mint(&vault_addr, &INITIAL_BALANCE);
    client.init(
        &owner,
        &usdc_addr,
        &Some(INITIAL_BALANCE),
        &None,
        &Some(1),
        &None,
        &Some(AMOUNT_CAP),
    );
    let settlement = create_settlement(&env, &owner, &vault_addr);
    client.set_settlement(&owner, &settlement);
    usdc_admin.mint(&owner, &INITIAL_BALANCE);
    usdc_client.approve(&owner, &vault_addr, &i128::MAX, &999_999);

    assert_balance_in_sync(&client, &usdc_client, &vault_addr, &Trace::new(42), 0);

    client.deposit(&owner, &100);
    assert_balance_in_sync(&client, &usdc_client, &vault_addr, &Trace::new(42), 1);

    client.pause(&owner);
    assert!(client.try_deposit(&owner, &50).is_err());
    assert!(client.try_deduct(&owner, &10, &None).is_err());
    assert_balance_in_sync(&client, &usdc_client, &vault_addr, &Trace::new(42), 2);

    // Withdraw is allowed while paused.
    client.withdraw(&50);
    assert_balance_in_sync(&client, &usdc_client, &vault_addr, &Trace::new(42), 3);

    client.unpause(&owner);
    client.deduct(&owner, &25, &None);
    assert_balance_in_sync(&client, &usdc_client, &vault_addr, &Trace::new(42), 4);
}

/// Edge case: allowed-depositor list toggled mid-sequence.
#[test]
fn test_balance_property_depositor_flips() {
    let env = Env::default();
    env.mock_all_auths();

    let owner = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (vault_addr, client) = create_vault(&env);
    let (usdc_addr, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    usdc_admin.mint(&vault_addr, &INITIAL_BALANCE);
    client.init(
        &owner,
        &usdc_addr,
        &Some(INITIAL_BALANCE),
        &None,
        &Some(1),
        &None,
        &Some(AMOUNT_CAP),
    );
    usdc_admin.mint(&depositor, &10_000);
    usdc_admin.mint(&owner, &10_000);
    usdc_client.approve(&depositor, &vault_addr, &i128::MAX, &999_999);
    usdc_client.approve(&owner, &vault_addr, &i128::MAX, &999_999);

    client.set_allowed_depositor(&owner, &Some(depositor.clone()));
    client.deposit(&depositor, &500);
    assert_balance_in_sync(&client, &usdc_client, &vault_addr, &Trace::new(7), 1);

    client.clear_allowed_depositors(&owner);
    assert!(client.try_deposit(&depositor, &100).is_err());
    assert_balance_in_sync(&client, &usdc_client, &vault_addr, &Trace::new(7), 2);

    client.deposit(&owner, &200);
    assert_balance_in_sync(&client, &usdc_client, &vault_addr, &Trace::new(7), 3);
}

/// Edge case: request_id cannot be reused after a successful deduct.
#[test]
fn test_balance_property_request_id_reuse() {
    let env = Env::default();
    env.mock_all_auths();

    let owner = Address::generate(&env);
    let (vault_addr, client) = create_vault(&env);
    let (usdc_addr, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    usdc_admin.mint(&vault_addr, &INITIAL_BALANCE);
    client.init(
        &owner,
        &usdc_addr,
        &Some(INITIAL_BALANCE),
        &None,
        &Some(1),
        &None,
        &Some(AMOUNT_CAP),
    );
    let settlement = create_settlement(&env, &owner, &vault_addr);
    client.set_settlement(&owner, &settlement);

    let rid = Symbol::new(&env, "reuse_test_id");
    client.deduct(&owner, &100, &Some(rid.clone()));
    assert_balance_in_sync(&client, &usdc_client, &vault_addr, &Trace::new(13), 1);

    let retry = client.try_deduct(&owner, &50, &Some(rid.clone()));
    assert!(retry.is_err());
    assert_balance_in_sync(&client, &usdc_client, &vault_addr, &Trace::new(13), 2);
    assert_eq!(client.balance(), INITIAL_BALANCE - 100);
}

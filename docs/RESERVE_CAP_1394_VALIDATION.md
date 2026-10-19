# Reserve cap validation for issue #1394

Final upstream base: `87fba99a7d7cc9eca9354e5a0aa408f9dfe4549b`. Its vault production sources are unchanged from the regression baseline.

Baseline: `f4ab47e03e9f7eb45ec6c5f022b1a1badad13ee2`, matching fetched `origin/main` at the start of this work. The baseline checkout's production `lib.rs` and `limits.rs` have no Git diff. A baseline-only integration harness uses the current test setup, 900 balance / 1,000 cap / 101 deposit regression, and budget test, without compiling the unrelated unit-test modules.

Baseline command: `cargo test -p callora-vault --test reserve_cap_regression -- --nocapture` (isolated target directory; unchanged production sources).

Actual regression output:

```text
assertion `left == right` failed: unexpected result: Ok(Ok(()))
  left: None
 right: Some(43)
test result: FAILED. 1 passed; 1 failed
```

Updated command: `cargo test -p callora-vault reserve_cap -- --nocapture`.
Result: 18 matching unit tests and 2 matching error-stability integration tests passed; no failures. The same over-cap regression now passes, and checks tracked balance, both token balances, and the events captured immediately after rejection.

## Deposit CPU budget

| Cap | Baseline | Updated | Delta | Change |
| --- | ---: | ---: | ---: | ---: |
| Unset | 272,314 | 272,457 | +143 | +0.053% |
| Explicit `i128::MAX` | 278,427 | 280,700 | +2,273 | +0.816% |
| Finite (1,000) | 277,838 | 279,806 | +1,968 | +0.708% |

Both runs use balance 900, deposit 100, identical native registered contracts and `mock_all_auths_allowing_non_root_auth`, and reset the SDK budget immediately before the deposit. Minting, approvals, initialization and cap configuration are outside measurement. The counter is read immediately after deposit, before balance assertions. Baseline `Env::budget()` and the current `Env::cost_estimate().budget()` expose the same host budget; the latter avoids a deprecation warning.

These are Soroban SDK CPU-budget instruction estimates for native test contracts, excluding Wasm VM instantiation/execution. They are not hardware instruction counts or production fee measurements.

The added guard performs exactly one SDK `instance().get(ReserveCap(token))` lookup. In installed SDK 22.0.11, `Storage::get` calls `has_contract_data` and, when present, `get_contract_data`: one logical lookup is not one raw host call. The guard adds no separate `has`, write, or TTL bump. The duplicate pause lookup is removed. Cap-key encoding, address/value conversion, and instance-map searches explain the small measured overhead; no additional ledger entry is introduced. Unset/unlimited caps return before cap addition; finite sums use checked addition and return `Overflow` before comparison if unrepresentable.

## Operational and API implications

Previously stored USDC caps become effective when this code is adopted. Lowering a cap below balance is allowed and blocks positive deposits until the resulting balance fits or the cap is raised. Withdrawals and deductions retain their existing rules. Other token caps cannot be newly configured; old unrelated-token entries are left untouched and do not limit USDC deposits. No storage layout or dependency changes are needed.

The setter returns `ReserveCapError` with legacy numeric errors preserved and unused code 59 for `CapTokenNotUsdc`. `VaultError` already occupies the XDR error-enum limit of 50 cases. Rust callers explicitly expecting `VaultError` from the setter need a source update; existing on-chain numeric errors remain stable.

## Verification on the final upstream base

- `cargo test -p callora-vault reserve_cap -- --nocapture`: passed (18 unit tests and 2 integration tests).
- `cargo fmt -p callora-vault -- --check`: passed.
- `cargo build --release --target wasm32v1-none -p callora-vault -p callora-settlement -p callora-revenue-pool`: passed.
- `cargo fmt --all -- --check`: blocked by the unchanged `contracts/yield/src/lib.rs:26` inner `#![no_std]` attribute following an outer doc comment.
- `cargo test --workspace`: blocked by seven compilation errors in unchanged `contracts/checkpoint/src/lib.rs` (migration API arity, missing `IntoVal`, private address constructor and value type mismatches).
- `cargo clippy --all-targets --all-features -- -D warnings`: blocked by those checkpoint errors and the unchanged registry `needless_question_mark` at `contracts/registry/src/lib.rs:276`.

Before upstream advanced, the complete workspace suite passed on baseline `f4ab47e03e9f7eb45ec6c5f022b1a1badad13ee2` plus this change: 1,873 passed, 0 failed, 170 existing ignored. Global formatting also passed there. These earlier results do not establish a passing workspace on final base `87fba99a7d7cc9eca9354e5a0aa408f9dfe4549b`. The diff does not modify checkpoint, yield or registry. Unrelated upstream failures are left for their respective fixes.

## Function-specific coverage

Tool: cargo-llvm-cov 0.9.1 with Rust 1.98.1 and its matching LLVM 22.1.8 llvm-tools-preview component, on Windows MSVC. Command run from the repository root:

```text
cargo llvm-cov --lib -p callora-vault --json --output-path coverage.json -- reserve_cap
```

The actual output path was the external validation evidence directory; no generated coverage artifacts are committed. All 18 instrumented matching unit tests passed. `llvm-cov report --show-functions` on the emitted test executable and merged profile gives these individual production function results, without substituting crate totals or generated client wrappers:

| Function | Covered lines / instrumented lines | Line coverage | Covered regions / regions |
| --- | ---: | ---: | ---: |
| `CalloraVault::deposit` (`lib.rs:448`) | 38 / 38 | 100.00% | 65 / 65 |
| `limits::check` (`limits.rs:71`) | 18 / 18 | 100.00% | 20 / 20 |

Both exceed the requested 95% line-coverage target. This stable-toolchain run does not instrument LLVM branch coverage; branch semantics are separately pinned by the targeted tests. The uninitialized-token panic closure is a separate generated function, not included in the deposit row.

Reproduce the function report after the coverage command (substitute the emitted executable hash and profile basename for your checkout):

```text
llvm-cov report target/llvm-cov-target/debug/deps/callora_vault-<hash>.exe --show-functions contracts/vault/src/lib.rs contracts/vault/src/limits.rs -instr-profile=target/llvm-cov-target/<checkout>.profdata
```

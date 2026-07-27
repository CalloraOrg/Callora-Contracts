//! Property tests for the validators crate core invariant.
//!
//! # Invariant
//! Across arbitrary byte sequences (and sequences of independent checks):
//!
//! 1. `is_visible_ascii_metadata(s) == normalize_visible_ascii(s).is_ok()`
//! 2. Acceptance matches the independent oracle [`bytes_are_visible_ascii`]
//! 3. Accepted buffers are byte-identical to the input (NFC-stable for ASCII)
//! 4. Rejection is stable under re-check (idempotent reject / accept)
//!
//! # Strategy
//! - `proptest` drives random byte vectors plus focused edge cases
//! - A deterministic LCG also runs 64 seeded action sequences of length ≥ 32
//!   so CI has reproducible traces without relying solely on proptest shrinking
//!
//! Closes CalloraOrg/Callora-Contracts#691.

extern crate std;

use callora_validators::{
    bytes_are_visible_ascii, is_visible_ascii_metadata, normalize_visible_ascii,
    MAX_VALIDATED_STRING_LEN,
};
use proptest::prelude::*;
use soroban_sdk::{Env, String};
use std::string::String as StdString;

/// Number of deterministic LCG traces (acceptance: ≥ 64).
const SEED_COUNT: u64 = 64;
/// Steps per LCG trace (acceptance: ≥ 32).
const TRACE_LENGTH: u32 = 32;

/// 64-bit LCG — deterministic, no `std` dependency in production paths.
struct Prng {
    state: u64,
}

impl Prng {
    fn new(seed: u64) -> Self {
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

    fn gen_len(&mut self, max_inclusive: usize) -> usize {
        (self.next_u64() as usize) % (max_inclusive + 1)
    }

    fn gen_byte(&mut self) -> u8 {
        self.next_u64() as u8
    }
}

fn sdk_string_from_ascii(env: &Env, bytes: &[u8]) -> String {
    let s = StdString::from_utf8(bytes.to_vec()).expect("ASCII is valid UTF-8");
    String::from_str(env, &s)
}

/// Assert all core invariants for one input.
fn assert_invariants(env: &Env, bytes: &[u8], seed: Option<u64>, step: Option<u32>) {
    let oracle = bytes_are_visible_ascii(bytes);

    // Host path only for valid UTF-8 (Soroban String requirement).
    if let Ok(text) = core::str::from_utf8(bytes) {
        let s = String::from_str(env, text);
        let accepted = is_visible_ascii_metadata(&s);
        let normalized = normalize_visible_ascii(&s);

        assert_eq!(
            accepted,
            normalized.is_ok(),
            "seed={seed:?} step={step:?} predicate/normalize disagree for {bytes:?}"
        );
        assert_eq!(
            accepted, oracle,
            "seed={seed:?} step={step:?} host validator disagreed with byte oracle for {bytes:?}"
        );

        if let Ok(buf) = normalized {
            let len = bytes.len();
            assert_eq!(
                &buf[..len],
                bytes,
                "seed={seed:?} step={step:?} accepted buffer must equal input bytes"
            );
            // Idempotence: re-checking the same string stays accepted.
            assert!(
                is_visible_ascii_metadata(&s),
                "seed={seed:?} step={step:?} acceptance must be idempotent"
            );
        } else {
            assert!(
                !is_visible_ascii_metadata(&s),
                "seed={seed:?} step={step:?} rejection must be idempotent"
            );
        }
    } else {
        // Non-UTF-8 can never be constructed as Soroban String; oracle must reject
        // anything outside visible ASCII (which non-UTF-8 always is, since UTF-8
        // multi-byte sequences use bytes > 0x7F).
        assert!(
            !oracle,
            "seed={seed:?} step={step:?} non-UTF-8 bytes must fail the oracle"
        );
    }
}

/// Run one deterministic action sequence for `seed`.
fn run_trace(seed: u64) {
    let env = Env::default();
    let mut rng = Prng::new(seed);

    for step in 1..=TRACE_LENGTH {
        let kind = rng.next_u64() % 6;
        let bytes: std::vec::Vec<u8> = match kind {
            // Uniform random bytes (may be non-UTF-8).
            0 => {
                let len = rng.gen_len(MAX_VALIDATED_STRING_LEN as usize + 8);
                (0..len).map(|_| rng.gen_byte()).collect()
            }
            // Visible ASCII body, possibly with leading/trailing space.
            1 => {
                let len = rng.gen_len(MAX_VALIDATED_STRING_LEN as usize).max(1);
                let mut v: std::vec::Vec<u8> = (0..len)
                    .map(|_| 0x21 + (rng.gen_byte() % (0x7e - 0x21 + 1)))
                    .collect();
                if rng.next_u64().is_multiple_of(2) {
                    v.insert(0, b' ');
                }
                if rng.next_u64().is_multiple_of(2) {
                    v.push(b' ');
                }
                v
            }
            // Empty.
            2 => std::vec::Vec::new(),
            // Exact max-length valid ASCII.
            3 => (0..MAX_VALIDATED_STRING_LEN as usize)
                .map(|_| 0x41 + (rng.gen_byte() % 26))
                .collect(),
            // Over-long ASCII.
            4 => (0..(MAX_VALIDATED_STRING_LEN as usize + 1 + (rng.gen_len(16))))
                .map(|_| 0x61 + (rng.gen_byte() % 26))
                .collect(),
            // Control / DEL injection into otherwise-valid ASCII.
            5 => {
                let len = rng.gen_len(64).max(1);
                let mut v: std::vec::Vec<u8> =
                    (0..len).map(|_| 0x41 + (rng.gen_byte() % 26)).collect();
                let idx = (rng.next_u64() as usize) % v.len();
                v[idx] = if rng.next_u64().is_multiple_of(2) {
                    rng.gen_byte() % 0x20
                } else {
                    0x7f
                };
                v
            }
            _ => unreachable!(),
        };

        assert_invariants(&env, &bytes, Some(seed), Some(step));
    }
}

// ---------------------------------------------------------------------------
// Deterministic seeded traces
// ---------------------------------------------------------------------------

#[test]
fn test_validators_invariant_seeded_traces() {
    for seed in 0..SEED_COUNT {
        run_trace(seed);
    }
}

// ---------------------------------------------------------------------------
// Focused edge cases
// ---------------------------------------------------------------------------

#[test]
fn empty_string_rejected() {
    let env = Env::default();
    assert_invariants(&env, b"", None, None);
}

#[test]
fn single_space_rejected() {
    let env = Env::default();
    assert_invariants(&env, b" ", None, None);
}

#[test]
fn leading_and_trailing_space_rejected() {
    let env = Env::default();
    assert_invariants(&env, b" hello", None, None);
    assert_invariants(&env, b"hello ", None, None);
    assert_invariants(&env, b" hello ", None, None);
}

#[test]
fn control_and_del_rejected() {
    let env = Env::default();
    assert_invariants(&env, b"a\x00b", None, None);
    assert_invariants(&env, b"a\x1fb", None, None);
    assert_invariants(&env, b"a\x7fb", None, None);
}

#[test]
fn valid_visible_ascii_accepted() {
    let env = Env::default();
    for sample in [b"ipfs://cid".as_slice(), b"A", b"~!@#"] {
        let s = sdk_string_from_ascii(&env, sample);
        assert!(is_visible_ascii_metadata(&s));
        assert_invariants(&env, sample, None, None);
    }
}

#[test]
fn exact_max_length_accepted_overlong_rejected() {
    let env = Env::default();
    let ok = vec![b'x'; MAX_VALIDATED_STRING_LEN as usize];
    let over = vec![b'x'; MAX_VALIDATED_STRING_LEN as usize + 1];
    assert_invariants(&env, &ok, None, None);
    assert_invariants(&env, &over, None, None);
}

#[test]
fn unicode_multibyte_rejected_by_oracle_and_host() {
    let env = Env::default();
    // Cyrillic lookalike / combining accent — multi-byte UTF-8, not visible ASCII.
    assert_invariants(&env, "раypal".as_bytes(), None, None);
    assert_invariants(&env, "cafe\u{0301}".as_bytes(), None, None);
    assert_invariants(&env, "meta\u{200b}data".as_bytes(), None, None);
}

// ---------------------------------------------------------------------------
// proptest-driven random inputs
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Random byte vectors preserve the validator ↔ oracle invariant.
    #[test]
    fn prop_random_bytes_match_oracle(bytes in prop::collection::vec(any::<u8>(), 0..=280)) {
        let env = Env::default();
        assert_invariants(&env, &bytes, None, None);
    }

    /// Random visible-ASCII bodies (with optional edge padding) stay consistent.
    #[test]
    fn prop_ascii_bodies(
        body in prop::collection::vec(0x21u8..=0x7eu8, 0..=260),
        lead_space in any::<bool>(),
        trail_space in any::<bool>(),
    ) {
        let env = Env::default();
        let mut bytes = std::vec::Vec::new();
        if lead_space {
            bytes.push(b' ');
        }
        bytes.extend_from_slice(&body);
        if trail_space {
            bytes.push(b' ');
        }
        assert_invariants(&env, &bytes, None, None);
    }
}

# callora-validators

Reusable, semantic input validators for the Callora contracts.

This crate replaces generic panics and opaque `Result<_, ()>` error values in
validation code with a stable, machine-readable [`ValidatorError`] enum. Every
rejection carries a specific reason, so on-chain callers can branch on numeric
error codes instead of parsing panic strings.

All validators are **pure and stateless** — they neither read nor write
contract storage and expose no state-changing entrypoints. There is therefore
nothing to authorize (`require_auth` is not applicable), and all arithmetic uses
overflow-safe checked operations. No production path uses `unwrap()`/`expect()`.

## Error codes (`ValidatorError`)

The numeric discriminants are part of the public interface and are guaranteed to
remain stable (guarded by a code-stability test).

| Code | Variant              | Meaning                                                      |
|------|----------------------|--------------------------------------------------------------|
| 1    | `Empty`              | Input string is empty                                        |
| 2    | `TooLong`            | Input exceeds the maximum allowed length                     |
| 3    | `LeadingWhitespace`  | Input has leading whitespace                                 |
| 4    | `TrailingWhitespace` | Input has trailing whitespace                                |
| 5    | `NonVisibleAscii`    | Input contains non-visible or non-ASCII bytes               |
| 6    | `AmountNotPositive`  | Numeric amount must be greater than zero                    |
| 7    | `AmountNegative`     | Numeric amount must be non-negative                         |
| 8    | `Overflow`           | Arithmetic overflow was detected                            |
| 9    | `OutOfRange`         | Value falls outside the allowed inclusive `[min, max]` range |

## Public API

| Function | Signature | Errors |
|----------|-----------|--------|
| `normalize_visible_ascii` | `(&String) -> Result<[u8; 256], ValidatorError>` | `Empty`, `TooLong`, `LeadingWhitespace`, `TrailingWhitespace`, `NonVisibleAscii` |
| `is_visible_ascii_metadata` | `(&String) -> bool` | — (boolean wrapper) |
| `require_positive_amount` | `(i128) -> Result<i128, ValidatorError>` | `AmountNotPositive` |
| `require_non_negative_amount` | `(i128) -> Result<i128, ValidatorError>` | `AmountNegative` |
| `checked_add_amount` | `(i128, i128) -> Result<i128, ValidatorError>` | `Overflow` |
| `require_in_range` | `(i128, i128, i128) -> Result<i128, ValidatorError>` | `OutOfRange` |
| `capabilities` | `(&Env) -> u64` | — (read-only capability bitmap view) |

The new `capabilities()` view returns a stable `u64` bitmask for the validator
features exposed by the current crate version. Clients can compare bitmasks
across upgrades to detect capability deltas without needing to inspect the
contract implementation directly.

`MAX_VALIDATED_STRING_LEN` (256) is the maximum byte length accepted by the
string validators.

## Testing

```bash
cargo test --package callora-validators
```

Tests cover every enum variant and every function branch (happy paths, all
boundaries, and all error paths).

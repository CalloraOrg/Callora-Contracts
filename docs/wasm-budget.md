# Contract WASM Size Budget

Callora contract builds are gated by a per-contract WASM budget of 100 KB
(`102400` bytes). Keeping each publishable contract below this threshold limits
deployment cost, leaves room for emergency patches, and prevents accidental
dependency or feature growth from reaching production.

## Local Check

Run the same check used by CI from the repository root:

```bash
./scripts/check-wasm-size.sh
```

The script discovers publishable contract crates under `contracts/*/Cargo.toml`
by looking for `crate-type = ["cdylib", ...]`, builds only those crates for
`wasm32-unknown-unknown` in release mode, then checks each generated `.wasm`
artifact independently.

## Configuration

The default limit is `102400` bytes. To test a tighter or temporary budget:

```bash
WASM_SIZE_LIMIT_BYTES=90000 ./scripts/check-wasm-size.sh
```

For fixture-based checks where artifacts are already built, set
`SKIP_WASM_BUILD=1` and point `CARGO_TARGET_DIR` at the artifact tree:

```bash
SKIP_WASM_BUILD=1 CARGO_TARGET_DIR=/tmp/callora-target ./scripts/check-wasm-size.sh
```

## CI Gate

`.github/workflows/wasm-size.yml` runs on pull requests and relevant pushes.
Any missing artifact or artifact over the configured byte limit fails the job.
The main CI workflow also invokes the script during the release build job, so a
size regression blocks both focused and full contract checks.

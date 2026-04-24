#!/usr/bin/env bash
# Check that all publishable contract WASM binaries stay under the 64 KiB Soroban limit.

set -euo pipefail

MAX_SIZE_BYTES=$((64 * 1024))
TARGET_DIR="${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release"

contract_manifests=()
contract_packages=()
failed=0

if command -v cargo >/dev/null 2>&1; then
  CARGO_BIN=$(command -v cargo)
elif command -v cargo.exe >/dev/null 2>&1; then
  CARGO_BIN=$(command -v cargo.exe)
elif [ -n "${HOME:-}" ] && [ -x "${HOME}/.cargo/bin/cargo" ]; then
  CARGO_BIN="${HOME}/.cargo/bin/cargo"
elif [ -n "${HOME:-}" ] && [ -x "${HOME}/.cargo/bin/cargo.exe" ]; then
  CARGO_BIN="${HOME}/.cargo/bin/cargo.exe"
elif [ -n "${USERPROFILE:-}" ] && [ -x "${USERPROFILE}/.cargo/bin/cargo.exe" ]; then
  CARGO_BIN="${USERPROFILE}/.cargo/bin/cargo.exe"
elif [ -n "${USERNAME:-}" ] && [ -x "/c/Users/${USERNAME}/.cargo/bin/cargo.exe" ]; then
  CARGO_BIN="/c/Users/${USERNAME}/.cargo/bin/cargo.exe"
else
  echo "ERROR: cargo was not found on PATH and no fallback binary was detected"
  exit 1
fi

while IFS= read -r -d '' manifest; do
  contract_manifests+=("$manifest")
done < <(find contracts -mindepth 2 -maxdepth 2 -name Cargo.toml -print0 | sort -z)

if [ "${#contract_manifests[@]}" -eq 0 ]; then
  echo "ERROR: no contract manifests found under contracts/*/Cargo.toml"
  exit 1
fi

discover_contract_packages() {
  local manifest
  local package_name

  for manifest in "${contract_manifests[@]}"; do
    if ! grep -Eq 'crate-type\s*=\s*\[[^]]*"cdylib"' "$manifest"; then
      continue
    fi

    package_name=$(awk -F'"' '/^[[:space:]]*name[[:space:]]*=/{print $2; exit}' "$manifest")
    if [ -z "$package_name" ]; then
      echo "ERROR: unable to determine package name from $manifest"
      exit 1
    fi

    contract_packages+=("$package_name")
  done

  if [ "${#contract_packages[@]}" -eq 0 ]; then
    echo 'ERROR: no publishable contract crates with crate-type = ["cdylib", ...] were found'
    exit 1
  fi
}

check_wasm() {
  local crate="$1"
  local wasm_name="${crate//-/_}"
  local wasm_file="$TARGET_DIR/${wasm_name}.wasm"
  local size_bytes
  local size_kib
  local headroom_bytes
  local headroom_kib

  if [ ! -f "$wasm_file" ]; then
    echo "FAIL  $crate: missing artifact at $wasm_file"
    failed=1
    return
  fi

  size_bytes=$(wc -c < "$wasm_file")
  size_kib=$((size_bytes / 1024))

  if [ "$size_bytes" -gt "$MAX_SIZE_BYTES" ]; then
    echo "FAIL  $crate: ${size_bytes} bytes (${size_kib} KiB) exceeds 65536-byte limit"
    failed=1
    return
  fi

  headroom_bytes=$((MAX_SIZE_BYTES - size_bytes))
  headroom_kib=$((headroom_bytes / 1024))
  echo "OK    $crate: ${size_bytes} bytes (${size_kib} KiB, ${headroom_bytes} bytes / ${headroom_kib} KiB headroom)"
}

discover_contract_packages

echo "Building publishable contracts for wasm32-unknown-unknown (release)..."
cargo_args=(build --target wasm32-unknown-unknown --release)
for crate in "${contract_packages[@]}"; do
  cargo_args+=(-p "$crate")
done
"$CARGO_BIN" "${cargo_args[@]}"

echo ""
echo "WASM size check (limit: 65536 bytes / 64 KiB)"
echo "---------------------------------------------"
for crate in "${contract_packages[@]}"; do
  check_wasm "$crate"
done
echo ""

if [ "$failed" -ne 0 ]; then
  echo "One or more publishable contract WASM artifacts are missing or exceed the Soroban size limit."
  exit 1
fi

echo "All publishable contract WASM artifacts are within the Soroban size limit."

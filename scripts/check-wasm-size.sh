#!/bin/bash
# Check that all contract WASM binaries stay under the 64KB Soroban limit.

set -e

MAX_SIZE=$((64 * 1024))
FAILED=0

check_wasm() {
  local crate="$1"
  local wasm_name="$2"
  local wasm_file="target/wasm32-unknown-unknown/release/${wasm_name}.wasm"

  if [ ! -f "$wasm_file" ]; then
    echo "ERROR: $wasm_file not found — did the build run?"
    FAILED=1
    return
  fi

  local size
  size=$(wc -c < "$wasm_file")
  local size_kb=$((size / 1024))

  if [ "$size" -gt "$MAX_SIZE" ]; then
    echo "FAIL  $crate: ${size_kb}KB — exceeds 64KB limit"
    FAILED=1
  else
    local headroom=$(( (MAX_SIZE - size) / 1024 ))
    echo "OK    $crate: ${size_kb}KB (${headroom}KB headroom)"
  fi
}

echo "Building all contracts for wasm32-unknown-unknown (release)..."
cargo build --target wasm32-unknown-unknown --release \
  -p callora-vault \
  -p callora-revenue-pool \
  -p callora-settlement

echo ""
echo "WASM size check (limit: 64KB)"
echo "------------------------------"
check_wasm "callora-vault"        "callora_vault"
check_wasm "callora-revenue-pool" "callora_revenue_pool"
check_wasm "callora-settlement"   "callora_settlement"
echo ""

if [ $FAILED -ne 0 ]; then
  echo "One or more contracts exceed the size limit."
  exit 1
fi

echo "All contracts within size limit."

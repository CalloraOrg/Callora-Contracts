#!/bin/bash
# Check that all contract WASM binaries stay under 64KB limit

set -e

# Contracts to check
CONTRACTS=("callora-vault" "callora-revenue-pool" "callora-settlement")

# Path to WASM binaries
WASM_DIR="target/wasm32-unknown-unknown/release"

echo "Building all contracts..."
cargo build --target wasm32-unknown-unknown --release

EXIT_CODE=0

for CONTRACT in "${CONTRACTS[@]}"; do
    # Convert package name to file name (hyphens to underscores)
    WASM_NAME=$(echo "$CONTRACT" | tr '-' '_').wasm
    WASM_FILE="${WASM_DIR}/${WASM_NAME}"
    
    if [ ! -f "$WASM_FILE" ]; then
        echo "❌ ERROR: WASM file not found for $CONTRACT at $WASM_FILE"
        EXIT_CODE=1
        continue
    fi
    
    SIZE=$(wc -c < "$WASM_FILE")
    SIZE_KB=$((SIZE / 1024))
    MAX_SIZE=$((64 * 1024))  # 64KB in bytes
    
    echo "--------------------------------------------------"
    echo "Contract: $CONTRACT"
    echo "WASM size: $SIZE bytes (${SIZE_KB}KB)"
    echo "Maximum allowed: $MAX_SIZE bytes (64KB)"
    
    if [ "$SIZE" -gt "$MAX_SIZE" ]; then
        echo "❌ ERROR: WASM binary exceeds 64KB limit!"
        EXIT_CODE=1
    else
        REMAINING=$((MAX_SIZE - SIZE))
        REMAINING_KB=$((REMAINING / 1024))
        echo "✅ WASM size check passed!"
        echo "   Remaining headroom: ${REMAINING_KB}KB"
    fi
done

exit $EXIT_CODE

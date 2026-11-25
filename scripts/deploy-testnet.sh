#!/usr/bin/env bash

set -euo pipefail

SOURCE="deployer"
NETWORK="testnet"
USDC=""
DRY_RUN=false

usage() {
  cat <<'EOF'
Usage: scripts/deploy-testnet.sh [options]

Options:
  --source <key-name>  Stellar CLI key name (default: deployer)
  --network <network>  Stellar network name (default: testnet)
  --usdc <contract-id> Use an existing USDC SAC contract ID
  --dry-run            Print commands without running them
  -h, --help           Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --source)
      [[ $# -ge 2 ]] || { echo "ERROR: --source requires a value" >&2; exit 2; }
      SOURCE="$2"
      shift 2
      ;;
    --network)
      [[ $# -ge 2 ]] || { echo "ERROR: --network requires a value" >&2; exit 2; }
      NETWORK="$2"
      shift 2
      ;;
    --usdc)
      [[ $# -ge 2 ]] || { echo "ERROR: --usdc requires a value" >&2; exit 2; }
      USDC="$2"
      shift 2
      ;;
    --dry-run)
      DRY_RUN=true
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "ERROR: unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

log() { echo "[deploy-testnet] $*"; }
err() { echo "[deploy-testnet] ERROR: $*" >&2; }

if ! $DRY_RUN; then
  for dep in stellar jq; do
    if ! command -v "$dep" >/dev/null 2>&1; then
      err "'$dep' is required but not installed."
      exit 2
    fi
  done
fi

print_command() {
  printf '  '
  printf '%q ' "$@"
  printf '\n'
}

run_command() {
  print_command "$@" >&2
  "$@"
}

capture_contract_id() {
  local output="$1"
  local id

  id=$(printf '%s\n' "$output" | grep -Eo 'C[A-Z0-9]{55}' | tail -n 1 || true)
  if [[ -z "$id" ]]; then
    id=$(printf '%s\n' "$output" | tr -d '\r' | awk 'NF { line=$0 } END { gsub(/[[:space:]"]/, "", line); print line }')
  fi
  if [[ ! "$id" =~ ^C[A-Z0-9]{55}$ ]]; then
    err "could not parse a contract ID from Stellar CLI output"
    printf '%s\n' "$output" >&2
    return 1
  fi
  printf '%s' "$id"
}

if $DRY_RUN; then
  DEPLOYER_ADDRESS="<deployer-address>"
  SETTLEMENT_ID="<settlement-contract-id>"
  VAULT_ID="<vault-contract-id>"
  POOL_ID="<revenue-pool-contract-id>"
  USDC_ID="${USDC:-<usdc-contract-id>}"

  log "Dry run for source='$SOURCE', network='$NETWORK'"
  print_command stellar contract build --package callora-vault
  print_command stellar contract build --package callora-settlement
  print_command stellar contract build --package callora-revenue-pool

  if [[ -z "$USDC" ]]; then
    print_command stellar contract asset deploy --asset "USDC:${DEPLOYER_ADDRESS}" --network "$NETWORK" --source "$SOURCE"
  fi

  print_command stellar contract deploy --wasm target/wasm32v1-none/release/callora_settlement.wasm --network "$NETWORK" --source "$SOURCE"
  print_command stellar contract deploy --wasm target/wasm32v1-none/release/callora_vault.wasm --network "$NETWORK" --source "$SOURCE"
  print_command stellar contract deploy --wasm target/wasm32v1-none/release/callora_revenue_pool.wasm --network "$NETWORK" --source "$SOURCE"

  print_command stellar contract invoke --id "$SETTLEMENT_ID" --network "$NETWORK" --source "$SOURCE" -- init --admin "$DEPLOYER_ADDRESS" --vault_address "$VAULT_ID"
  print_command stellar contract invoke --id "$POOL_ID" --network "$NETWORK" --source "$SOURCE" -- init --admin "$DEPLOYER_ADDRESS" --usdc_token "$USDC_ID"
  print_command stellar contract invoke --id "$VAULT_ID" --network "$NETWORK" --source "$SOURCE" -- init \
    --owner "$DEPLOYER_ADDRESS" --usdc_token "$USDC_ID" --initial_balance 0 \
    --authorized_caller "\"$DEPLOYER_ADDRESS\"" --min_deposit 1 \
    --revenue_pool "\"$POOL_ID\"" --max_deduct 1000000000 --settlement "\"$SETTLEMENT_ID\""

  print_command stellar contract invoke --id "$VAULT_ID" --network "$NETWORK" --source "$SOURCE" -- get_settlement
  print_command stellar contract invoke --id "$VAULT_ID" --network "$NETWORK" --source "$SOURCE" -- get_owner
  print_command stellar contract invoke --id "$VAULT_ID" --network "$NETWORK" --source "$SOURCE" -- get_usdc_token
  print_command stellar contract invoke --id "$VAULT_ID" --network "$NETWORK" --source "$SOURCE" -- get_revenue_pool
  print_command stellar contract invoke --id "$SETTLEMENT_ID" --network "$NETWORK" --source "$SOURCE" -- get_admin
  print_command stellar contract invoke --id "$POOL_ID" --network "$NETWORK" --source "$SOURCE" -- get_admin
  exit 0
fi

if ! DEPLOYER_ADDRESS=$(stellar keys address "$SOURCE" 2>/dev/null); then
  err "could not resolve key '$SOURCE'. Generate it first or pass --source with an existing key."
  exit 2
fi
DEPLOYER_ADDRESS=$(printf '%s' "$DEPLOYER_ADDRESS" | tr -d '\r[:space:]')
if [[ -z "$DEPLOYER_ADDRESS" ]]; then
  err "resolved an empty Stellar address for key '$SOURCE'."
  exit 2
fi

log "Building WASMs..."
run_command stellar contract build --package callora-vault
run_command stellar contract build --package callora-settlement
run_command stellar contract build --package callora-revenue-pool

if [[ -z "$USDC" ]]; then
  log "Deploying USDC asset contract..."
  USDC_OUTPUT=$(run_command stellar contract asset deploy --asset "USDC:${DEPLOYER_ADDRESS}" --network "$NETWORK" --source "$SOURCE")
  USDC=$(capture_contract_id "$USDC_OUTPUT")
  log "USDC=$USDC"
else
  log "Using existing USDC contract: $USDC"
fi

log "Deploying settlement..."
SETTLEMENT_OUTPUT=$(run_command stellar contract deploy --wasm target/wasm32v1-none/release/callora_settlement.wasm --network "$NETWORK" --source "$SOURCE")
SETTLEMENT_ID=$(capture_contract_id "$SETTLEMENT_OUTPUT")

log "Deploying vault..."
VAULT_OUTPUT=$(run_command stellar contract deploy --wasm target/wasm32v1-none/release/callora_vault.wasm --network "$NETWORK" --source "$SOURCE")
VAULT_ID=$(capture_contract_id "$VAULT_OUTPUT")

log "Deploying revenue pool..."
POOL_OUTPUT=$(run_command stellar contract deploy --wasm target/wasm32v1-none/release/callora_revenue_pool.wasm --network "$NETWORK" --source "$SOURCE")
POOL_ID=$(capture_contract_id "$POOL_OUTPUT")

log "Initialising settlement..."
run_command stellar contract invoke --id "$SETTLEMENT_ID" --network "$NETWORK" --source "$SOURCE" -- init --admin "$DEPLOYER_ADDRESS" --vault_address "$VAULT_ID"

log "Initialising revenue pool..."
run_command stellar contract invoke --id "$POOL_ID" --network "$NETWORK" --source "$SOURCE" -- init --admin "$DEPLOYER_ADDRESS" --usdc_token "$USDC"

log "Initialising vault..."
run_command stellar contract invoke --id "$VAULT_ID" --network "$NETWORK" --source "$SOURCE" -- init \
  --owner "$DEPLOYER_ADDRESS" --usdc_token "$USDC" --initial_balance 0 \
  --authorized_caller "\"$DEPLOYER_ADDRESS\"" --min_deposit 1 \
  --revenue_pool "\"$POOL_ID\"" --max_deduct 1000000000 --settlement "\"$SETTLEMENT_ID\""

log "Verifying wiring..."
VAULT_SETTLEMENT=$(stellar contract invoke --id "$VAULT_ID" --network "$NETWORK" --source "$SOURCE" -- get_settlement)
VAULT_OWNER=$(stellar contract invoke --id "$VAULT_ID" --network "$NETWORK" --source "$SOURCE" -- get_owner)
VAULT_USDC=$(stellar contract invoke --id "$VAULT_ID" --network "$NETWORK" --source "$SOURCE" -- get_usdc_token)
VAULT_POOL=$(stellar contract invoke --id "$VAULT_ID" --network "$NETWORK" --source "$SOURCE" -- get_revenue_pool)
SETTLEMENT_ADMIN=$(stellar contract invoke --id "$SETTLEMENT_ID" --network "$NETWORK" --source "$SOURCE" -- get_admin)
POOL_ADMIN=$(stellar contract invoke --id "$POOL_ID" --network "$NETWORK" --source "$SOURCE" -- get_admin)

normalize_result() {
  printf '%s' "$1" | tr -d '\r\n[:space:]' | tr -d '"'
}

VAULT_SETTLEMENT_NORMALIZED=$(normalize_result "$VAULT_SETTLEMENT")
VAULT_OWNER_NORMALIZED=$(normalize_result "$VAULT_OWNER")
VAULT_USDC_NORMALIZED=$(normalize_result "$VAULT_USDC")
VAULT_POOL_NORMALIZED=$(normalize_result "$VAULT_POOL")
SETTLEMENT_ADMIN_NORMALIZED=$(normalize_result "$SETTLEMENT_ADMIN")
POOL_ADMIN_NORMALIZED=$(normalize_result "$POOL_ADMIN")

if [[ "$VAULT_SETTLEMENT_NORMALIZED" != "$SETTLEMENT_ID" ]]; then
  err "vault get_settlement did not match settlement ID"
  exit 1
fi
if [[ "$VAULT_USDC_NORMALIZED" != "$USDC" ]]; then
  err "vault get_usdc_token did not match USDC contract ID"
  exit 1
fi
if [[ "$VAULT_POOL_NORMALIZED" != "$POOL_ID" ]]; then
  err "vault get_revenue_pool did not match revenue pool ID"
  exit 1
fi
if [[ "$VAULT_OWNER_NORMALIZED" != "$DEPLOYER_ADDRESS" ]]; then
  err "vault get_owner did not match deployer address"
  exit 1
fi
if [[ "$SETTLEMENT_ADMIN_NORMALIZED" != "$DEPLOYER_ADDRESS" ]]; then
  err "settlement get_admin did not match deployer address"
  exit 1
fi
if [[ "$POOL_ADMIN_NORMALIZED" != "$DEPLOYER_ADDRESS" ]]; then
  err "revenue pool get_admin did not match deployer address"
  exit 1
fi

echo ""
echo "Callora testnet deployment"
echo "---------------------------"
printf '%-18s %s\n' "Deployer" "$DEPLOYER_ADDRESS"
printf '%-18s %s\n' "USDC" "$USDC"
printf '%-18s %s\n' "Vault" "$VAULT_ID"
printf '%-18s %s\n' "Settlement" "$SETTLEMENT_ID"
printf '%-18s %s\n' "Revenue pool" "$POOL_ID"

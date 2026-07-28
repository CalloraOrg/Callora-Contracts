#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASELINE_PATH="${REPO_ROOT}/contracts/.gas-baseline.json"
THRESHOLD=5
UPDATE_BASELINE=false
REPORT_PATH="${REPO_ROOT}/target/gas-report.md"
MEASUREMENTS_PATH="${REPO_ROOT}/target/gas-measurements.json"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --update-baseline)  UPDATE_BASELINE=true ;;
    --threshold)        THRESHOLD="$2"; shift ;;
    *) echo "Unknown option: $1" >&2; exit 2 ;;
  esac
  shift
done

for dep in jq cargo python3; do
  if ! command -v "$dep" &>/dev/null; then
    echo "ERROR: '$dep' is required but not installed." >&2
    exit 2
  fi
done

log()  { echo "[gas-regression] $*"; }
err()  { echo "[gas-regression] ERROR: $*" >&2; }

pct_increase() {
  python3 -c "
import math
base, cur = int('$1'), int('$2')
if base == 0:
    print(0)
else:
    pct = (cur - base) / base * 100
    print(math.ceil(pct))
"
}

log "Building workspace..."
cargo build --workspace 2>&1 | tail -5

log "Running gas measurement tests..."
mkdir -p "${REPO_ROOT}/target"

cargo test -p callora-vault -- gas_budget --nocapture 2>/dev/null \
  | grep '^{"contract"' \
  > "${MEASUREMENTS_PATH}" || true

if [[ ! -s "${MEASUREMENTS_PATH}" ]]; then
  err "No gas measurements found."
  exit 2
fi

log "Measurements written to ${MEASUREMENTS_PATH}"

if $UPDATE_BASELINE; then
  python3 - "${MEASUREMENTS_PATH}" "${BASELINE_PATH}" <<'PYEOF'
import json, sys
path_m, path_b = sys.argv[1], sys.argv[2]
with open(path_m) as f:
    lines = [json.loads(l) for l in f if l.strip()]
try:
    with open(path_b) as f:
        baseline = json.load(f)
except FileNotFoundError:
    baseline = {}
baseline.setdefault("_schema_version", 1)
for m in lines:
    baseline.setdefault(m["contract"], {})[m["entrypoint"]] = {"cpu": m["cpu"], "mem": m["mem"]}
with open(path_b, "w") as f:
    json.dump(baseline, f, indent=2)
    f.write("\n")
print(f"Baseline written to {path_b}")
PYEOF
  log "Done. Commit ${BASELINE_PATH}."
  exit 0
fi

if [[ ! -f "${BASELINE_PATH}" ]]; then
  err "Baseline not found. Run: ./scripts/gas-regression.sh --update-baseline"
  exit 2
fi

FAIL=0
REPORT_ROWS=""

while IFS= read -r line; do
  contract=$(echo "$line"  | jq -r '.contract')
  ep=$(echo "$line"        | jq -r '.entrypoint')
  cur_cpu=$(echo "$line"   | jq -r '.cpu')
  cur_mem=$(echo "$line"   | jq -r '.mem')
  base_cpu=$(jq -r --arg c "$contract" --arg e "$ep" '.[$c][$e].cpu // 0' "${BASELINE_PATH}")
  base_mem=$(jq -r --arg c "$contract" --arg e "$ep" '.[$c][$e].mem // 0' "${BASELINE_PATH}")
  cpu_pct=$(pct_increase "$base_cpu" "$cur_cpu")
  mem_pct=$(pct_increase "$base_mem" "$cur_mem")
  cpu_ok="✅"; mem_ok="✅"; row_fail=false
  if [[ "$cpu_pct" -gt "$THRESHOLD" ]]; then cpu_ok="❌"; row_fail=true; fi
  if [[ "$mem_pct" -gt "$THRESHOLD" ]]; then mem_ok="❌"; row_fail=true; fi
  if $row_fail; then FAIL=1; log "FAIL [$contract::$ep] cpu=+${cpu_pct}% mem=+${mem_pct}%"; fi
  REPORT_ROWS+="| \`${contract}\` | \`${ep}\` | ${base_cpu} | ${cur_cpu} | ${cpu_pct}% ${cpu_ok} | ${base_mem} | ${cur_mem} | ${mem_pct}% ${mem_ok} |"$'\n'
done < "${MEASUREMENTS_PATH}"

mkdir -p "$(dirname "${REPORT_PATH}")"
cat > "${REPORT_PATH}" <<MDEOF
## ⛽ Gas Budget Regression Report

> Threshold: **${THRESHOLD}%**

| Contract | Entrypoint | CPU base | CPU now | CPU Δ | Mem base | Mem now | Mem Δ |
|----------|-----------|---------|---------|-------|---------|---------|-------|
${REPORT_ROWS}
MDEOF

cat "${REPORT_PATH}"

if [[ "$FAIL" -ne 0 ]]; then
  err "Regression detected. Run --update-baseline if intentional."
  exit 1
fi

log "All entrypoints within threshold. ✅"
exit 0

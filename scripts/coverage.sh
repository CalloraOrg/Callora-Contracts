#!/usr/bin/env bash
#
# coverage.sh — line-coverage report for the Callora workspace.
#
# Runs the workspace test suite under `cargo-llvm-cov`, writes an LCOV report,
# prints a per-crate summary and optionally enforces a minimum line-coverage
# floor.
#
# Test-only code is excluded from the report with
# `--ignore-filename-regex 'test|tests/|kani_proofs'`: unit-test modules,
# integration-test crates and Kani proof harnesses exercise production code but
# are not part of the shipped contract surface, so counting them would inflate
# the number.
#
# Usage:
#   ./scripts/coverage.sh
#   ./scripts/coverage.sh --html
#   ./scripts/coverage.sh --fail-under-lines 80
#
# Environment:
#   COVERAGE_FAIL_UNDER_LINES  Default minimum line-coverage percentage
#                              (0 disables enforcement).
#
# Requires: cargo and cargo-llvm-cov
#   rustup component add llvm-tools-preview
#   cargo install cargo-llvm-cov
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${REPO_ROOT}"

OUTPUT_PATH="${REPO_ROOT}/target/lcov.info"
SUMMARY_PATH="${REPO_ROOT}/target/coverage-summary.txt"
CRATE_SUMMARY_PATH="${REPO_ROOT}/target/coverage-crates.txt"
HTML_DIR="${REPO_ROOT}/target/coverage/html"
FAIL_UNDER_LINES="${COVERAGE_FAIL_UNDER_LINES:-0}"
GENERATE_HTML=false

# Test-only files are excluded from the production coverage number.
IGNORE_REGEX='test|tests/|kani_proofs'

usage() {
  cat <<'EOF'
Usage: scripts/coverage.sh [--html] [--fail-under-lines <N>]

  --html                 Also write an HTML report to target/coverage/html.
  --fail-under-lines <N> Exit non-zero when total line coverage is below N%.
                         Defaults to $COVERAGE_FAIL_UNDER_LINES, or 0 (disabled).
  -h, --help             Show this help.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --html)             GENERATE_HTML=true ;;
    --fail-under-lines) FAIL_UNDER_LINES="${2:?--fail-under-lines requires a value}"; shift ;;
    -h|--help)          usage; exit 0 ;;
    *) echo "Unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
  shift
done

log() { echo "[coverage] $*"; }
err() { echo "[coverage] ERROR: $*" >&2; }

if [[ ! "${FAIL_UNDER_LINES}" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
  err "--fail-under-lines expects a non-negative number, got '${FAIL_UNDER_LINES}'."
  exit 2
fi

if ! command -v cargo &>/dev/null; then
  err "'cargo' is required but not installed."
  exit 2
fi

if ! cargo llvm-cov --version &>/dev/null; then
  err "'cargo-llvm-cov' is required but not installed."
  err "Install it with: rustup component add llvm-tools-preview && cargo install cargo-llvm-cov"
  exit 2
fi

mkdir -p "$(dirname "${OUTPUT_PATH}")"

log "Running workspace tests under cargo-llvm-cov..."
# `--no-report` keeps the raw profile data so every report below can read it.
cargo llvm-cov --workspace --no-report \
  --ignore-filename-regex "${IGNORE_REGEX}"

log "Writing LCOV report..."
cargo llvm-cov report --workspace --lcov \
  --output-path "${OUTPUT_PATH}" \
  --ignore-filename-regex "${IGNORE_REGEX}"

log "Per-file summary:"
cargo llvm-cov report --workspace \
  --ignore-filename-regex "${IGNORE_REGEX}" | tee "${SUMMARY_PATH}"

log "Per-crate summary:"
{
  printf '%-28s %8s %8s %8s\n' "Crate" "Lines" "Hit" "Cover"
  awk -F: '
    /^SF:/ { file = $2 }
    /^LH:/ { hit[file] += $2 }
    /^LF:/ { total[file] += $2 }
    END {
      for (f in total) {
        crate = f
        sub(/^.*contracts\//, "", crate)
        sub(/\/.*/, "", crate)
        ch[crate] += hit[f]
        ct[crate] += total[f]
      }
      for (c in ct) {
        pct = ct[c] > 0 ? ch[c] * 100 / ct[c] : 0
        printf "%-28s %8d %8d %7.1f%%\n", c, ct[c], ch[c], pct
      }
    }' "${OUTPUT_PATH}" | sort
} | tee "${CRATE_SUMMARY_PATH}"

if $GENERATE_HTML; then
  log "Writing HTML report to ${HTML_DIR}..."
  cargo llvm-cov report --workspace --html \
    --output-dir "${HTML_DIR}" \
    --ignore-filename-regex "${IGNORE_REGEX}"
fi

line_hit=$(awk -F: '/^LH:/{sum+=$2} END{print sum+0}' "${OUTPUT_PATH}")
line_total=$(awk -F: '/^LF:/{sum+=$2} END{print sum+0}' "${OUTPUT_PATH}")
line_pct=$(awk -v hit="${line_hit}" -v total="${line_total}" \
  'BEGIN { printf "%.2f", (total > 0 ? hit * 100 / total : 0) }')

log "Total line coverage: ${line_pct}% (${line_hit}/${line_total} lines)"
log "LCOV report: ${OUTPUT_PATH}"
log "Summary:     ${SUMMARY_PATH}"

if [[ "${FAIL_UNDER_LINES}" != "0" ]]; then
  if awk -v pct="${line_pct}" -v min="${FAIL_UNDER_LINES}" \
      'BEGIN { exit !(pct + 0 < min + 0) }'; then
    err "Line coverage ${line_pct}% is below the required ${FAIL_UNDER_LINES}%."
    exit 1
  fi
  log "Line coverage meets the ${FAIL_UNDER_LINES}% floor. ✅"
fi

log "Done. ✅"

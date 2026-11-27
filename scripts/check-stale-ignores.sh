#!/usr/bin/env bash
# Guard: fail CI when the number of `stale test` #[ignore] attributes grows.
# The current count is compared against the committed baseline in
# scripts/stale-ignore-baseline.txt. The backlog may only shrink.
#
# Usage: bash scripts/check-stale-ignores.sh
# Exit codes: 0 = within baseline, 1 = count exceeded baseline (or bad input).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
BASELINE_FILE="${SCRIPT_DIR}/stale-ignore-baseline.txt"

if [[ ! -f "${BASELINE_FILE}" ]]; then
  echo "error: baseline file not found: ${BASELINE_FILE}" >&2
  exit 1
fi

BASELINE="$(tr -d '[:space:]' < "${BASELINE_FILE}")"
if ! [[ "${BASELINE}" =~ ^[0-9]+$ ]]; then
  echo "error: baseline must be a non-negative integer, got '${BASELINE}' in ${BASELINE_FILE}" >&2
  exit 1
fi

# Count matching lines in Rust sources under contracts/, excluding target/.
# grep exits 1 when there are no matches; treat that as zero.
COUNT="$(cd "${REPO_ROOT}" && (grep -rn 'stale test' contracts --include='*.rs' || true) | wc -l | tr -d '[:space:]')"

if (( COUNT > BASELINE )); then
  echo "FAIL: stale-test ignore count increased." >&2
  echo "  current : ${COUNT}" >&2
  echo "  baseline: ${BASELINE} (${BASELINE_FILE})" >&2
  echo "New '#[ignore = \"stale test: ...\"]' attributes are not allowed." >&2
  echo "Fix the test instead, or see docs/KNOWN_ISSUES.md for the policy." >&2
  exit 1
fi

if (( COUNT < BASELINE )); then
  echo "OK: stale-test ignore count is ${COUNT}, below baseline ${BASELINE}."
  echo "Hint: lower the baseline by setting ${BASELINE_FILE} to ${COUNT}."
  exit 0
fi

echo "OK: stale-test ignore count ${COUNT} matches baseline."

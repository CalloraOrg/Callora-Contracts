#!/usr/bin/env bash
# scripts/coverage.sh
#
# Generate a test coverage report for the Callora Contracts workspace using
# cargo-tarpaulin and enforce a minimum of 95% line coverage.
#
# Usage:
#   ./scripts/coverage.sh
#
# Prerequisites:
#   cargo install cargo-tarpaulin

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
NC='\033[0m'

if ! command -v cargo-tarpaulin &>/dev/null; then
  echo -e "${RED}ERROR:${NC} cargo-tarpaulin is not installed."
  echo "       Install it with:  cargo install cargo-tarpaulin"
  exit 1
fi

TARPAULIN_VERSION=$(cargo tarpaulin --version 2>&1 || true)
echo -e "  ${CYAN}[INFO]${NC}  Using ${TARPAULIN_VERSION}"
echo -e "  ${CYAN}[INFO]${NC}  Running tests with coverage instrumentation..."

cargo tarpaulin

STATUS=$?

# Upload HTML report as a CI artifact when running in GitHub Actions.
if [ -n "${GITHUB_ACTIONS:-}" ] && [ -f "coverage/tarpaulin-report.html" ]; then
  echo "::notice::Coverage report available at coverage/tarpaulin-report.html"
fi

if [ $STATUS -eq 0 ]; then
  echo ""
  echo -e "  ${GREEN}[OK]${NC}  Coverage threshold met."
else
  echo ""
  echo -e "  ${RED}[FAIL]${NC}  Coverage below threshold — see report above."
fi

exit $STATUS

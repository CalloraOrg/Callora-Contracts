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
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
NC='\033[0m'

tarpaulin_missing_hint() {
  local reason="$1"
  echo -e "${RED}ERROR:${NC} ${reason}" >&2
  echo "" >&2
  echo -e "${YELLOW}How to fix:${NC}" >&2
  echo "  1. Install cargo-tarpaulin (Linux/macOS x86_64 + Linux aarch64 supported):" >&2
  echo "       cargo install cargo-tarpaulin --locked" >&2
  echo "" >&2
  echo "  2. Verify the install resolves on your PATH:" >&2
  echo "       command -v cargo-tarpaulin   # should print a path under ~/.cargo/bin" >&2
  echo "       cargo tarpaulin --version    # should print a version banner" >&2
  echo "" >&2
  echo "  3. If 'cargo install' fails on macOS with linker errors, make sure the" >&2
  echo "     Xcode command-line tools are installed:" >&2
  echo "       xcode-select --install" >&2
  echo "" >&2
  echo "  4. On unsupported platforms (e.g. Apple Silicon pre-0.27, Windows)," >&2
  echo "     run coverage in the project's Linux CI instead:" >&2
  echo "       gh workflow run coverage.yml" >&2
  echo "" >&2
  echo -e "${CYAN}Docs:${NC} https://github.com/xd009642/tarpaulin#installation" >&2
  echo -e "${CYAN}Config:${NC} see tarpaulin.toml at the repo root for the coverage profile." >&2
  exit 127
}

if ! command -v cargo-tarpaulin &>/dev/null; then
  tarpaulin_missing_hint "cargo-tarpaulin binary not found on PATH."
fi

# The binary may exist but fail to execute as a cargo subcommand (e.g. built
# against an incompatible rustc). Probe it so the hints trigger there too.
if ! cargo tarpaulin --version &>/dev/null; then
  tarpaulin_missing_hint "cargo-tarpaulin is installed but 'cargo tarpaulin --version' failed."
fi

TARPAULIN_VERSION=$(cargo tarpaulin --version 2>&1 || true)
echo -e "  ${CYAN}[INFO]${NC}  Using ${TARPAULIN_VERSION}"
echo -e "  ${CYAN}[INFO]${NC}  Running tests with coverage instrumentation..."

# Disable errexit around tarpaulin so the threshold-failure branch below is
# reachable; tarpaulin exits non-zero when coverage < fail-under.
set +e
cargo tarpaulin
STATUS=$?
set -e

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

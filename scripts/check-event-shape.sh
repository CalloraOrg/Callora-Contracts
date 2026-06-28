#!/usr/bin/env bash
#
# check-event-shape.sh
#
# CI gate enforcing the structured events index defined in
# docs/EVENTS_INDEX.md. This script greps every contract's lib.rs for
# event publish call sites and fails if any of them bypass the
# centralized `events::event_*()` constructors with a raw inline
# `Symbol::new(&env, "...")` topic literal.
#
# Rationale: every event topic MUST be defined exactly once in the
# contract's `events.rs` module (as an `event_*` function with a
# matching byte-identity snapshot test). Inline literals at call sites
# can drift silently — a typo or rename at one call site would not be
# caught by the events.rs snapshot tests, since those only test the
# centralized functions, not the call sites.
#
# Usage:
#   ./scripts/check-event-shape.sh
#
# Exit code 0  = all publish() call sites use centralized event_* constructors.
# Exit code 1  = one or more violations found; details printed to stderr.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VIOLATIONS=0

echo "Checking event topic shape across contracts/*/src/lib.rs ..."
echo ""

for lib_file in "$REPO_ROOT"/contracts/*/src/lib.rs; do
  contract_dir="$(dirname "$lib_file")"
  contract_name="$(basename "$(dirname "$contract_dir")")"

  # Find any .publish(( call site whose first topic element is a raw
  # Symbol::new(...) literal rather than an events::event_*() call.
  # Pattern: publish(( Symbol::new(...)
  matches=$(grep -nE '\.publish\(\s*\(\s*Symbol::new\(' "$lib_file" || true)

  if [ -n "$matches" ]; then
    echo "FAIL [$contract_name]: raw Symbol::new(...) used as event topic in $lib_file"
    echo "$matches" | sed 's/^/    /'
    echo "    -> Define a constructor in events.rs (event_<name>) and use events::event_<name>(&env) instead."
    echo ""
    VIOLATIONS=$((VIOLATIONS + 1))
  fi
done

if [ "$VIOLATIONS" -eq 0 ]; then
  echo "PASS: every publish() call site uses a centralized events::event_*() constructor."
  exit 0
else
  echo "FAILED: $VIOLATIONS file(s) with raw inline event topic literals."
  echo "See docs/EVENTS_INDEX.md for the required event topic shape and ladder."
  exit 1
fi
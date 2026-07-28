#!/usr/bin/env bash
# check-event-shape.sh
# Verifies that every env.events().publish() call site across all contracts
# uses a centralized events::event_*() constructor (never inline Symbol::new).
# Also verifies that every constructor-exported topic appears in EVENT_TOPICS.md.
#
# Exit 0 = all events are documented. Exit 1 = undocumented event found.
set -euo pipefail

SCHEMA="docs/EVENT_TOPICS.md"
EVENTS_DIR="contracts"

if [[ ! -f "$SCHEMA" ]]; then
  echo "ERROR: $SCHEMA not found (run from repo root)" >&2
  exit 1
fi

FAIL=0

check_contract() {
  local contract_name="$1"
  local lib="contracts/${contract_name}/src/lib.rs"
  local events="contracts/${contract_name}/src/events.rs"

  if [[ ! -f "$lib" ]]; then
    echo "WARN: $lib not found, skipping $contract_name" >&2
    return
  fi

  echo "=== $contract_name Event Shape Check ==="

  # 1. Verify no inline Symbol::new in publish call sites
  if [[ -f "$lib" ]]; then
    local INLINE_COUNT
    INLINE_COUNT=$(grep -cP 'env\.events\(\)\.publish\(\(.*Symbol::new' "$lib" 2>/dev/null || true)
    if [[ "$INLINE_COUNT" -gt 0 ]]; then
      echo "FAIL: $contract_name has $INLINE_COUNT inline Symbol::new in publish() calls"
      grep -nP 'env\.events\(\)\.publish\(\(.*Symbol::new' "$lib" || true
      FAIL=1
    else
      echo "OK: no inline Symbol::new in publish() calls"
    fi
  fi

  # 2. Verify every event constructor in events.rs has a snapshot test
  if [[ -f "$events" ]]; then
    local CTOR_COUNT
    CTOR_COUNT=$(grep -cP 'pub fn event_\w+' "$events" || true)
    # Count unique Symbol literals covered by snapshot tests
    # Some tests cover multiple symbols (e.g. migration events)
    local TESTED_SYMBOLS
    TESTED_SYMBOLS=$(grep -oP 'Symbol::new\(&env,\s*"\K[a-z_]+' "$events" | sort -u | wc -l)
    if [[ "$CTOR_COUNT" -ne "$TESTED_SYMBOLS" ]]; then
      echo "FAIL: $contract_name has $CTOR_COUNT constructors but tests cover $TESTED_SYMBOLS unique symbols"
      FAIL=1
    else
      echo "OK: $CTOR_COUNT constructors match $TESTED_SYMBOLS tested symbols"
    fi
  fi

  # 3. Verify every constructor-exported topic appears in EVENT_TOPICS.md
  if [[ -f "$events" ]]; then
    # Extract actual topic strings from Symbol::new(env, "...") calls
    mapfile -t TOPIC_STRINGS < <(
      grep -oP 'Symbol::new\(env,\s*"\K[a-z_]+' "$events" | sort -u
    )
    mapfile -t SCHEMA_TOPICS < <(
      grep -oP '^\|\s*\d+\s*\|\s*`\K[a-z_]+(?=`)' "$SCHEMA" | sort -u
    )

    local MISSING=()
    for topic in "${TOPIC_STRINGS[@]}"; do
      if ! printf '%s\n' "${SCHEMA_TOPICS[@]}" | grep -qx "$topic"; then
        MISSING+=("$topic")
      fi
    done

    if [[ ${#MISSING[@]} -gt 0 ]]; then
      echo "FAIL: the following $contract_name topics are in events.rs but not in EVENT_TOPICS.md:"
      for m in "${MISSING[@]}"; do
        echo "  - $m"
      done
      FAIL=1
    else
      echo "OK: all ${#TOPIC_STRINGS[@]} topics documented in EVENT_TOPICS.md"
    fi
  fi

  echo ""
}

check_contract "vault"
check_contract "settlement"
check_contract "revenue_pool"

if [[ "$FAIL" -ne 0 ]]; then
  echo "FAILED: some contracts have undocumented events. See above."
  exit 1
fi

echo "OK: all event constructors are centralized, tested, and documented."
exit 0

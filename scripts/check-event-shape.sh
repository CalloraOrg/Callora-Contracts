#!/usr/bin/env bash
# check-event-shape.sh
# Verifies that every env.events().publish() call site across all contracts
# uses a centralized events::event_*() constructor (never inline Symbol::new).
# Also verifies that every constructor-exported topic is snapshot-tested and
# appears in EVENT_TOPICS.md.
#
# Version markers (`event_version_*`, e.g. "callora.v1") describe the event
# schema version rather than an action, are emitted next to topic 0 instead of
# replacing it, and are therefore excluded from the snapshot-test and
# documentation rules. The script still reports them so an invalid marker
# literal (one that is not a valid Soroban symbol) stays visible.
#
# Exit 0 = all events are documented. Exit 1 = undocumented event found.
set -euo pipefail

SCHEMA="docs/EVENT_TOPICS.md"
EVENTS_DIR="contracts"

# Soroban topic strings are lowercase identifiers (see
# tests/event_topic_catalog.rs::all_topic_strings_are_valid_identifiers).
TOPIC_RE='^[a-z_][a-z0-9_]*$'

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

  if [[ -f "$events" ]]; then
    # Constructors that produce a real event topic (version markers excluded).
    local CTOR_COUNT
    CTOR_COUNT=$( { grep -oP 'pub fn \Kevent_(?!version)\w+' "$events" || true; } | wc -l | tr -d ' ')

    # Topic literals produced by the constructors above. The whole literal is
    # captured so a malformed symbol (e.g. one containing ".") cannot silently
    # truncate into a different string.
    local PRODUCED_TOPICS TESTED_TOPICS MARKERS
    PRODUCED_TOPICS=$( { grep -oP 'Symbol::new\(env,\s*"\K[^"]*' "$events" || true; } | grep -v '\.' | sort -u || true)
    TESTED_TOPICS=$( { grep -oP 'Symbol::new\(&env,\s*"\K[^"]*' "$events" || true; } | grep -v '\.' | sort -u || true)
    MARKERS=$( { grep -oP 'Symbol::new\(env,\s*"\K[^"]*' "$events" || true; } | grep '\.' | sort -u || true)

    local PRODUCED_COUNT TESTED_COUNT
    PRODUCED_COUNT=$(echo "$PRODUCED_TOPICS" | sed '/^$/d' | wc -l | tr -d ' ')
    TESTED_COUNT=$(echo "$TESTED_TOPICS" | sed '/^$/d' | wc -l | tr -d ' ')

    # 2a. Every constructor must publish its own distinct topic literal.
    if [[ "$CTOR_COUNT" -ne "$PRODUCED_COUNT" ]]; then
      echo "FAIL: $contract_name has $CTOR_COUNT constructors but only $PRODUCED_COUNT distinct topic literals"
      FAIL=1
    else
      echo "OK: $CTOR_COUNT constructors map to $PRODUCED_COUNT distinct topics"
    fi

    # 2b. Every produced topic must be covered by a snapshot test.
    local UNTESTED=()
    local topic
    while IFS= read -r topic; do
      [[ -z "$topic" ]] && continue
      if ! printf '%s\n' "$TESTED_TOPICS" | grep -qx "$topic"; then
        UNTESTED+=("$topic")
      fi
    done <<< "$PRODUCED_TOPICS"

    if [[ ${#UNTESTED[@]} -gt 0 ]]; then
      echo "FAIL: the following $contract_name topics have no snapshot test in events.rs:"
      for topic in "${UNTESTED[@]}"; do
        echo "  - $topic"
      done
      FAIL=1
    else
      echo "OK: all $PRODUCED_COUNT topics are snapshot-tested ($TESTED_COUNT unique tested)"
    fi

    # 2c. Report version markers, and flag marker literals that are not valid
    # Soroban symbols (they panic at runtime and can never be snapshot-tested).
    local marker
    while IFS= read -r marker; do
      [[ -z "$marker" ]] && continue
      if [[ ! "$marker" =~ $TOPIC_RE ]]; then
        echo "WARN: $contract_name version marker \"$marker\" is not a valid Soroban symbol"
      else
        echo "OK: $contract_name version marker \"$marker\""
      fi
    done <<< "$MARKERS"

    # 3. Verify every constructor-exported topic appears in EVENT_TOPICS.md
    mapfile -t SCHEMA_TOPICS < <(
      grep -oP '^\|\s*\d+\s*\|\s*`\K[^`]+' "$SCHEMA" | sort -u
    )

    local MISSING=()
    while IFS= read -r topic; do
      [[ -z "$topic" ]] && continue
      if ! printf '%s\n' "${SCHEMA_TOPICS[@]}" | grep -qx "$topic"; then
        MISSING+=("$topic")
      fi
    done <<< "$PRODUCED_TOPICS"

    if [[ ${#MISSING[@]} -gt 0 ]]; then
      echo "FAIL: the following $contract_name topics are in events.rs but not in EVENT_TOPICS.md:"
      for m in "${MISSING[@]}"; do
        echo "  - $m"
      done
      FAIL=1
    else
      echo "OK: all $PRODUCED_COUNT topics documented in EVENT_TOPICS.md"
    fi
  fi

  echo ""
}

check_contract "vault"
check_contract "settlement"
check_contract "revenue_pool"
check_contract "distribute"

if [[ "$FAIL" -ne 0 ]]; then
  echo "FAILED: some contracts have undocumented events. See above."
  exit 1
fi

echo "OK: all event constructors are centralized, tested, and documented."
exit 0

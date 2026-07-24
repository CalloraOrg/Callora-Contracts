#!/usr/bin/env bash
# check-event-shape.sh
# Verifies that every env.events().publish() call site in contracts/revenue_pool/src/lib.rs
# has a corresponding entry in EVENT_SCHEMA.md.
#
# Exit 0 = all events are documented. Exit 1 = undocumented event found.
set -euo pipefail

SCHEMA="EVENT_SCHEMA.md"
LIB="contracts/revenue_pool/src/lib.rs"

if [[ ! -f "$SCHEMA" ]]; then
  echo "ERROR: $SCHEMA not found (run from repo root)" >&2
  exit 1
fi
if [[ ! -f "$LIB" ]]; then
  echo "ERROR: $LIB not found (run from repo root)" >&2
  exit 1
fi

# Extract event names from publish call sites:
#   events::event_FOO(&env)  →  FOO
# Matches lines like: (events::event_admin_changed(&env), ...)
mapfile -t CODE_EVENTS < <(
  grep -oP 'events::event_\K[a-z_]+(?=\(&env\))' "$LIB" | sort -u
)

# Extract event names documented under the revenue-pool section of EVENT_SCHEMA.md.
# We look for ### `foo` headings between the revenue-pool header and the next ## header.
IN_POOL=0
mapfile -t SCHEMA_EVENTS < <(
  while IFS= read -r line; do
    if [[ "$line" =~ ^##[[:space:]].*callora-revenue-pool ]]; then
      IN_POOL=1; continue
    fi
    if [[ $IN_POOL -eq 1 && "$line" =~ ^##[[:space:]] ]]; then
      IN_POOL=0; continue
    fi
    if [[ $IN_POOL -eq 1 && "$line" =~ ^###[[:space:]]\`([a-z_]+)\` ]]; then
      echo "${BASH_REMATCH[1]}"
    fi
  done < "$SCHEMA" | sort -u
)

echo "=== Revenue Pool Event Shape Check ==="
echo "Events in lib.rs  : ${CODE_EVENTS[*]}"
echo "Events in schema   : ${SCHEMA_EVENTS[*]}"
echo ""

MISSING=()
for ev in "${CODE_EVENTS[@]}"; do
  if ! printf '%s\n' "${SCHEMA_EVENTS[@]}" | grep -qx "$ev"; then
    MISSING+=("$ev")
  fi
done

if [[ ${#MISSING[@]} -gt 0 ]]; then
  echo "FAIL: The following revenue_pool events are emitted in lib.rs but not documented in EVENT_SCHEMA.md:"
  for m in "${MISSING[@]}"; do
    echo "  - $m"
  done
  echo ""
  echo "Add a '### \`$m\`' section under the callora-revenue-pool heading in EVENT_SCHEMA.md."
  exit 1
fi

echo "OK: all revenue_pool event publish sites are documented in EVENT_SCHEMA.md."
exit 0

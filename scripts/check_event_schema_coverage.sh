#!/usr/bin/env bash
# Verify that every env.events().publish( site in the three contract crates
# has a matching topic entry in EVENT_SCHEMA.md.
#
# Usage:
#   ./scripts/check_event_schema_coverage.sh
#   SCHEMA_FILE=docs/MY_SCHEMA.md ./scripts/check_event_schema_coverage.sh
#
# Exit codes:
#   0  all topics are documented
#   1  one or more topics are missing from EVENT_SCHEMA.md

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCHEMA_FILE="${SCHEMA_FILE:-"${REPO_ROOT}/EVENT_SCHEMA.md"}"
CONTRACTS_DIR="${REPO_ROOT}/contracts"

if [[ ! -t 1 ]]; then
  RED=''
  GREEN=''
  YELLOW=''
  NC=''
fi

if [[ ! -f "${SCHEMA_FILE}" ]]; then
  echo -e "${RED}ERROR:${NC} schema file not found: ${SCHEMA_FILE}" >&2
  exit 1
fi

echo "Checking EVENT_SCHEMA.md coverage"
echo "  Schema : ${SCHEMA_FILE}"
echo "  Scope  : ${CONTRACTS_DIR}/*/src/*.rs (excluding #[cfg(test)] blocks)"
echo ""

# Remove #[cfg(test)] inline blocks and one-line test module imports.
strip_test_blocks() {
  awk '
    BEGIN { inside_test = 0; depth = 0 }

    function handle_test_content(content) {
      n = split(content, chars, "")
      for (i = 1; i <= n; i++) {
        if (chars[i] == "{") depth++
        if (chars[i] == "}") {
          depth--
          if (depth <= 0) { inside_test = 0; return }
        }
      }
    }

    /^[[:space:]]*#\[cfg\(test\)\]/ {
      if ((getline nextline) <= 0) next
      if (nextline ~ /^[[:space:]]*mod[[:space:]]+[A-Za-z_][A-Za-z0-9_]*[[:space:]]*;/) next
      inside_test = 1
      depth = 0
      handle_test_content(nextline)
      next
    }

    inside_test {
      handle_test_content($0)
      next
    }

    { print }
  ' "$1"
}

# Extract the event topic (first Symbol::new string) from each publish site.
# Handles both env.events().publish( and env.events()\n    .publish( forms.
collect_topics() {
  local lib="$1"

  strip_test_blocks "${lib}" \
    | perl -0777 -pe 's/env\.events\(\)\s*\n\s*\.publish\(/env.events().publish(/g' \
    | perl -0777 -ne '
        while (/\.publish\s*\(/g) {
          my $chunk = substr($_, pos(), 500);
          if ($chunk =~ /Symbol::new\(&env,\s*"([^"]+)"/) {
            print "$1\n";
          }
        }
      ' \
    | sort -u
}

declare -A ALL_TOPICS=()
topic_count=0

while IFS= read -r -d '' rs_file; do
  case "$(basename "${rs_file}")" in
    test.rs | test_*.rs) continue ;;
  esac

  crate=$(basename "$(dirname "$(dirname "${rs_file}")")")

  while IFS= read -r topic; do
    [[ -z "${topic}" ]] && continue
    ALL_TOPICS["${topic}"]="${crate}"
    topic_count=$((topic_count + 1))
  done < <(collect_topics "${rs_file}")
done < <(find "${CONTRACTS_DIR}" -path '*/src/*.rs' -print0 | sort -z)

if [[ ${topic_count} -eq 0 ]]; then
  echo -e "${YELLOW}WARN:${NC} no publish topics found under ${CONTRACTS_DIR}" >&2
  exit 0
fi

echo "Found ${#ALL_TOPICS[@]} unique topic(s) across all crates:"
for t in $(printf '%s\n' "${!ALL_TOPICS[@]}" | sort); do
  echo "  [${ALL_TOPICS[$t]}]  ${t}"
done
echo ""

# A topic is considered documented if the schema file contains any of:
#   ### `topic_name`   (section header)
#   `topic_name`       (inline backtick reference)
#   "topic_name"       (double-quoted, e.g. in JSON examples)

missing=()

for topic in $(printf '%s\n' "${!ALL_TOPICS[@]}" | sort); do
  if grep -qE "(###[[:space:]]+\`${topic}\`|\`${topic}\`|\"${topic}\")" "${SCHEMA_FILE}"; then
    echo -e "  ${GREEN}OK${NC}    ${topic}"
  else
    echo -e "  ${RED}MISSING${NC}  ${topic}  (crate: ${ALL_TOPICS[$topic]})"
    missing+=("${topic}")
  fi
done

echo ""

if [[ ${#missing[@]} -gt 0 ]]; then
  echo -e "${RED}FAIL:${NC} ${#missing[@]} topic(s) not documented in EVENT_SCHEMA.md:"
  for t in "${missing[@]}"; do
    echo "  - ${t}  (crate: ${ALL_TOPICS[$t]})"
  done
  echo ""
  echo "  Add a section to EVENT_SCHEMA.md for each missing topic, then re-run."
  exit 1
fi

echo -e "${GREEN}OK:${NC} all ${#ALL_TOPICS[@]} topic(s) are documented in EVENT_SCHEMA.md."

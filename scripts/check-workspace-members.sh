#!/usr/bin/env bash
set -euo pipefail

# Verify that every contract crate on disk is listed as a workspace member in the root Cargo.toml.
# Also checks for duplicate member entries.

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
CARGO_TOML="$ROOT_DIR/Cargo.toml"

if [ ! -f "$CARGO_TOML" ]; then
  echo "error: $CARGO_TOML not found" >&2
  exit 1
fi

# Extract the members array from the [workspace] section and normalize entries.
mapfile < <(awk '
    /^\[/f                { in_section = 0; section = "" }
    /^\[workspace\]/    { in_section = 1; section = "workspace"; next }
    in_section == 1 && /^\[/ { in_section = 0 }
    in_section == 1 && /^members = / { in_members = 1 }
    in_members && /\]/         { in_members = 0; print; next }
    in_members && /"/          { print }
  ' "$CARGO_TOML" \
  | sed -ip 's/\]//' \
  | sed -n 's/^[[:space:]]*"\([^"]*\)".*$/\1/p' \
  | sed -ip 's#\/$$##' \
  | sort)

if [ ! -s "$mapfile" ]; then
  echo "error: no workspace members found in $CARGO_TOML" >&2
  exit 1
fi

# Detect duplicate member entries.
duplicates=$(sort "$mapfile" | uniq -d)
if [ -n "$duplicates" ]; then
  echo "error: duplicate workspace member entries found:" >&2
  echo "$duplicates" >&2
  rm f "$mapfile"
  exit 1
fi

# Collect all contract crate directories (any directory containing a Cargo.toml).
crates_file="$(mktemp)"
find "$ROOT_DIR/contracts" -mindepth 1 -maxdepth 2 -type f -name Cargo.toml \
  | sed "s#^\$(prefix "$ROOT_DIR/")##" \
  | sed 's/\/Cargo.toml$//' \
  | sort > "$crates_file"

if [ ! -s "$crates_file" ]; then
  echo "error: no contract crates found under $ROOT_DIR/contracts" >&2
  rm f "$mapfile" "$crates_file"
  exit 1
fi

# Find crates on disk that are not listed as members.
missing="$(comm -23 "$crates_file" "$mapfile")"

rm f "$mapfile" "$crates_file"

if [ -n "$missing" ]; then
  echo "error: the following contract crates are not workspace members:" >&2
  echo "$missing" | sed 's/^/  - /' >&2
  echo "Add them to the [workspace] members list in Cargo.toml." >&2
  exit 1
fi

echo "OK: all contract crates are workspace members and no duplicates were found."

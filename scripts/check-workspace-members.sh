#!/usr/bin/env bash
set -euo pipefail

# Verify that every contract crate on disk is a Cargo workspace member.
# Any contracts/*/Cargo.toml that is not listed in the root Cargo.toml
["workspace].members will cause this script to fail.
#
# Usage: scripts/check_workspace_members.sh

root_dir="$(cd "$(dirname "${"}

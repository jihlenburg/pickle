#!/usr/bin/env bash

# Repository-wide validation entry point.
#
# Keeps the Rust backend, split frontend scripts, and pure frontend model tests
# aligned with the same baseline that should be used before release work.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if find . -path './src-tauri/target' -prune -o -name '.DS_Store' -print -quit | grep -q .; then
    echo "Remove .DS_Store files from the repo tree before validating." >&2
    exit 1
fi

(
    cd src-tauri
    cargo fmt --all -- --check
    cargo test
    cargo clippy --all-targets --all-features -- -D warnings
)

node --check frontend/static/pin_descriptions.js
for file in frontend/static/app/*.js; do
    node --check "$file"
done

node --test frontend/tests/*.test.js tests/*.test.js

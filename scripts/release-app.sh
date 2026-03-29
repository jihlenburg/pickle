#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tauri_dir="$repo_root/src-tauri"
bundle_src="$tauri_dir/target/release/bundle/macos/pickle.app"
bundle_dst="$repo_root/bin/pickle.app"

echo "Building pickle release bundle..."
(
  cd "$tauri_dir"
  cargo tauri build --bundles app
)

if [[ ! -d "$bundle_src" ]]; then
  echo "Expected bundle not found: $bundle_src" >&2
  exit 1
fi

mkdir -p "$repo_root/bin"
rm -rf "$bundle_dst"
cp -R "$bundle_src" "$bundle_dst"

echo "Copied latest app bundle to:"
echo "  $bundle_dst"

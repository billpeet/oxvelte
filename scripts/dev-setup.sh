#!/usr/bin/env bash
set -euo pipefail

# Install the local development dependencies that the build and test suite
# need but that are not tracked in git.
#
#   - scripts/compiler-runtime/node_modules — the Svelte compiler and friends.
#     src/compiler.rs drives the compiler through Node.js, and the compiler
#     fixture tests under `cargo test --lib` fail without these packages.
#   - scripts/upstream-parity/node_modules — the upstream parity harness
#     (scripts/upstream-parity/import.mjs, npm test there).
#
# Safe to run repeatedly: a directory is only reinstalled when its
# node_modules is missing or its package-lock.json is newer than the last
# install. release.sh runs this script before building and testing.

cd "$(dirname "$0")/.."

for tool in node npm cargo; do
  if ! command -v "$tool" > /dev/null 2>&1; then
    echo "error: '$tool' is required but was not found on PATH." >&2
    exit 1
  fi
done

npm_projects=(
  scripts/compiler-runtime
  scripts/upstream-parity
)

for dir in "${npm_projects[@]}"; do
  marker="${dir}/node_modules/.package-lock.json"
  if [ ! -d "${dir}/node_modules" ]; then
    echo "Installing npm dependencies in ${dir} (node_modules is missing)..."
  elif [ ! -f "$marker" ] || [ "${dir}/package-lock.json" -nt "$marker" ]; then
    echo "Reinstalling npm dependencies in ${dir} (package-lock.json changed)..."
  else
    echo "${dir}: npm dependencies up to date."
    continue
  fi
  (cd "$dir" && npm ci --ignore-scripts --no-audit --no-fund)
done

echo "Development dependencies are ready."

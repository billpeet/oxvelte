#!/usr/bin/env bash
set -euo pipefail

# Create a new release of oxvelte.
#
# Usage:
#   ./scripts/release.sh patch    # 0.1.0 -> 0.1.1
#   ./scripts/release.sh minor    # 0.1.0 -> 0.2.0
#   ./scripts/release.sh major    # 0.1.0 -> 1.0.0
#   ./scripts/release.sh 0.2.0    # explicit version

CURRENT=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
echo "Current version: ${CURRENT}"

IFS='.' read -r major minor patch <<< "$CURRENT"

case "${1:-}" in
  patch) NEW="${major}.${minor}.$((patch + 1))" ;;
  minor) NEW="${major}.$((minor + 1)).0" ;;
  major) NEW="$((major + 1)).0.0" ;;
  "") echo "Usage: $0 <patch|minor|major|VERSION>"; exit 1 ;;
  *) NEW="$1" ;;
esac

echo "New version: ${NEW}"
echo ""

# Update Cargo.toml
sed -i.bak "s/^version = \"${CURRENT}\"/version = \"${NEW}\"/" Cargo.toml
rm -f Cargo.toml.bak

# The npm packages take their version from Cargo.toml when the release
# workflow stages them (scripts/stage-npm.mjs), so there is nothing else to bump.

# Verify build (this also updates the version recorded in Cargo.lock)
echo "Building..."
cargo build --release

echo "Running tests..."
cargo test --lib

# Commit and tag
git add Cargo.toml Cargo.lock
git commit -m "release: v${NEW}"
git tag -a "v${NEW}" -m "v${NEW}"

echo ""
echo "Release v${NEW} created."
echo ""
echo "To publish:"
echo "  git push origin main --tags"
echo ""
echo "This will trigger the GitHub Actions release workflow,"
echo "which builds binaries and publishes to npm."

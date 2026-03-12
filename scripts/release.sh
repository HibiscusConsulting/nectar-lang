#!/bin/sh
# Usage: ./scripts/release.sh 0.2.0

set -e

VERSION="$1"

if [ -z "$VERSION" ]; then
    echo "Usage: ./scripts/release.sh <version>"
    echo "Example: ./scripts/release.sh 0.2.0"
    exit 1
fi

echo "Releasing Nectar v${VERSION}..."

# Update Cargo.toml version
sed -i.bak "s/^version = \".*\"/version = \"${VERSION}\"/" Cargo.toml
rm -f Cargo.toml.bak

# Update lock file
cargo check

# Commit and tag
git add Cargo.toml Cargo.lock
git commit -m "Release v${VERSION}"
git tag -a "v${VERSION}" -m "Release v${VERSION}"

echo ""
echo "Release v${VERSION} prepared."
echo "Run 'git push origin main --tags' to trigger the release build."

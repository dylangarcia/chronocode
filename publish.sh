#!/usr/bin/env bash
set -euo pipefail

# Publish chronocode: bump version, test, commit, tag, push, publish to crates.io.
#
# Usage:
#   ./publish.sh patch    # 0.1.1 -> 0.1.2
#   ./publish.sh minor    # 0.1.1 -> 0.2.0
#   ./publish.sh major    # 0.1.1 -> 1.0.0
#   ./publish.sh 0.3.0    # set exact version

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

die() { echo -e "${RED}Error: $1${NC}" >&2; exit 1; }
info() { echo -e "${GREEN}==> $1${NC}"; }
warn() { echo -e "${YELLOW}    $1${NC}"; }

# ---------------------------------------------------------------------------
# Parse arguments
# ---------------------------------------------------------------------------

BUMP="${1:-}"
[ -z "$BUMP" ] && die "Usage: ./publish.sh <patch|minor|major|X.Y.Z>"

# ---------------------------------------------------------------------------
# Pre-flight checks
# ---------------------------------------------------------------------------

info "Running pre-flight checks..."

command -v cargo >/dev/null || die "cargo not found"
command -v git >/dev/null   || die "git not found"

# Ensure working tree is clean
if [ -n "$(git status --porcelain)" ]; then
    die "Working tree is dirty. Commit or stash your changes first."
fi

# Ensure we're on main
BRANCH=$(git branch --show-current)
if [ "$BRANCH" != "main" ]; then
    warn "Not on main (on '$BRANCH'). Continue anyway? [y/N]"
    read -r REPLY
    [ "$REPLY" = "y" ] || exit 1
fi

# ---------------------------------------------------------------------------
# Compute new version
# ---------------------------------------------------------------------------

CURRENT=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
info "Current version: $CURRENT"

IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT"

case "$BUMP" in
    patch) PATCH=$((PATCH + 1)) ; NEW_VERSION="$MAJOR.$MINOR.$PATCH" ;;
    minor) MINOR=$((MINOR + 1)) ; PATCH=0 ; NEW_VERSION="$MAJOR.$MINOR.$PATCH" ;;
    major) MAJOR=$((MAJOR + 1)) ; MINOR=0 ; PATCH=0 ; NEW_VERSION="$MAJOR.$MINOR.$PATCH" ;;
    [0-9]*.[0-9]*.[0-9]*) NEW_VERSION="$BUMP" ;;
    *) die "Invalid bump: '$BUMP'. Use patch, minor, major, or X.Y.Z" ;;
esac

info "New version: $NEW_VERSION"
echo ""

# ---------------------------------------------------------------------------
# Update version in Cargo.toml
# ---------------------------------------------------------------------------

info "Updating Cargo.toml..."
sed -i '' "s/^version = \"$CURRENT\"/version = \"$NEW_VERSION\"/" Cargo.toml

# Update Cargo.lock
cargo update -p chronocode >/dev/null 2>&1

# ---------------------------------------------------------------------------
# Quality checks
# ---------------------------------------------------------------------------

info "Running fmt..."
cargo fmt --check || die "cargo fmt failed. Run 'cargo fmt' first."

info "Running clippy..."
cargo clippy -- -D warnings || die "clippy found warnings"

info "Running tests..."
cargo test --locked || die "tests failed"

info "Building release..."
cargo build --release || die "release build failed"

# ---------------------------------------------------------------------------
# Package verification
# ---------------------------------------------------------------------------

info "Verifying package..."
cargo package --allow-dirty || die "cargo package failed"

CRATE_SIZE=$(ls -lh target/package/chronocode-"$NEW_VERSION".crate | awk '{print $5}')
BINARY_SIZE=$(ls -lh target/release/chronocode | awk '{print $5}')
echo ""
warn "Crate size:  $CRATE_SIZE"
warn "Binary size: $BINARY_SIZE"
echo ""

# ---------------------------------------------------------------------------
# Confirm
# ---------------------------------------------------------------------------

echo -e "${YELLOW}Ready to publish chronocode $NEW_VERSION${NC}"
echo "  This will:"
echo "    1. Commit version bump"
echo "    2. Tag v$NEW_VERSION"
echo "    3. Push to origin"
echo "    4. Publish to crates.io"
echo ""
echo -n "Proceed? [y/N] "
read -r REPLY
[ "$REPLY" = "y" ] || { git checkout Cargo.toml Cargo.lock; die "Aborted."; }

# ---------------------------------------------------------------------------
# Git commit + tag + push
# ---------------------------------------------------------------------------

info "Committing..."
git add Cargo.toml Cargo.lock
git commit -m "v$NEW_VERSION"

info "Tagging v$NEW_VERSION..."
git tag "v$NEW_VERSION"

info "Pushing..."
git push origin "$BRANCH"
git push origin "v$NEW_VERSION"

# ---------------------------------------------------------------------------
# Publish to crates.io
# ---------------------------------------------------------------------------

info "Publishing to crates.io..."
cargo publish

echo ""
info "Done! chronocode $NEW_VERSION is live."
echo ""
echo "  crates.io:  https://crates.io/crates/chronocode/$NEW_VERSION"
echo "  GitHub:     https://github.com/dylangarcia/chronocode/releases/tag/v$NEW_VERSION"
echo "  Install:    cargo install chronocode"

#!/usr/bin/env bash
#
# Update Homebrew formula with version and SHA256 checksums from release artifacts
#
# Usage: ./scripts/update-formula.sh <release-dir>
# Example: ./scripts/update-formula.sh release-v0.3.1
#
# This script updates the local homebrew-tap/Formula/dotdipper.rb file
# After running, you need to commit and push the changes to the tap repository

set -euo pipefail

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m'

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1" >&2; }
log_step() { echo -e "${BLUE}==>${NC} $1"; }

RELEASE_DIR="${1:-}"

if [ -z "$RELEASE_DIR" ] || [ ! -d "$RELEASE_DIR" ]; then
    echo "Usage: $0 <release-dir>"
    echo "Example: $0 release-v0.3.1"
    echo ""
    echo "Available release directories:"
    ls -d release-v* 2>/dev/null || echo "  (none found)"
    exit 1
fi

# Ensure we're in the repo root
cd "$(git rev-parse --show-toplevel)"

FORMULA_PATH="homebrew-tap/Formula/dotdipper.rb"

if [ ! -f "$FORMULA_PATH" ]; then
    log_error "Formula not found at $FORMULA_PATH"
    log_info "Make sure you have the homebrew-tap submodule or directory"
    exit 1
fi

# Extract version from directory name (e.g., release-v0.3.1 -> 0.3.1)
VERSION=$(echo "$RELEASE_DIR" | sed 's/.*release-v//')
if [ -z "$VERSION" ]; then
    # Fallback: get from Cargo.toml
    VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
fi

log_step "Updating formula to version $VERSION"

log_step "Generating updated formula..."

# The template lives in gen-formula.sh, which the release workflow also uses.
# Keeping a second copy here is how the two drifted: this one emitted Linux
# blocks and the workflow's did not, so whichever ran last decided whether
# Linux users could install at all.
"$(dirname "$0")/gen-formula.sh" --version "$VERSION" --checksums "$RELEASE_DIR" \
    > "$FORMULA_PATH"

log_info "Formula updated successfully!"
echo ""

# A platform with no artifact is left out of the formula rather than given a
# placeholder checksum, so say which ones made it in.
for platform in macos linux; do
    if grep -q "on_${platform} do" "$FORMULA_PATH"; then
        log_info "Formula covers ${platform}"
    else
        log_warn "Formula does NOT cover ${platform} (no checksums in $RELEASE_DIR)"
    fi
done

# Show next steps
log_step "Next steps:"
echo "  1. Review the formula: $FORMULA_PATH"
echo "  2. Commit and push to the tap repository:"
echo "     cd homebrew-tap"
echo "     git add Formula/dotdipper.rb"
echo "     git commit -m \"Update dotdipper to $VERSION\""
echo "     git push"
echo ""
log_info "Done!"

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

# Extract the version from the directory name (release-v0.3.1 -> 0.3.1).
#
# sed echoes its input unchanged when the pattern does not match, so a
# directory named "artifacts" used to yield VERSION=artifacts and a formula
# whose every URL was dead — and the [ -z "$VERSION" ] fallback that was here
# could therefore never fire. Validate instead.
VERSION=$(basename "${RELEASE_DIR%/}" | sed -n 's/^release-v\(.*\)$/\1/p')
if [ -z "$VERSION" ]; then
    log_error "Cannot read a version from '$RELEASE_DIR'."
    log_info  "Expected a directory named release-vX.Y.Z (e.g. release-v0.8.0)."
    exit 1
fi
if ! printf '%s' "$VERSION" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$'; then
    log_error "'$VERSION' is not a semantic version."
    exit 1
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

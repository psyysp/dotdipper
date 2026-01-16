#!/usr/bin/env bash
#
# Update Homebrew formula with SHA256 checksums from release artifacts
#
# Usage: ./scripts/update-formula.sh <release-dir>
# Example: ./scripts/update-formula.sh release-v0.3.0

set -euo pipefail

GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_step() { echo -e "${BLUE}==>${NC} $1"; }

RELEASE_DIR="${1:-}"

if [ -z "$RELEASE_DIR" ] || [ ! -d "$RELEASE_DIR" ]; then
    echo "Usage: $0 <release-dir>"
    echo "Example: $0 release-v0.3.0"
    exit 1
fi

FORMULA_PATH="homebrew-tap/Formula/dotdipper.rb"

if [ ! -f "$FORMULA_PATH" ]; then
    echo "Formula not found at $FORMULA_PATH"
    exit 1
fi

# Extract SHA256 values
get_sha256() {
    local file="$RELEASE_DIR/dotdipper-$1.tar.gz.sha256"
    if [ -f "$file" ]; then
        awk '{print $1}' "$file"
    else
        echo "MISSING"
    fi
}

SHA256_MACOS_ARM=$(get_sha256 "aarch64-apple-darwin")
SHA256_MACOS_X86=$(get_sha256 "x86_64-apple-darwin")
SHA256_LINUX_ARM=$(get_sha256 "aarch64-unknown-linux-gnu")
SHA256_LINUX_X86=$(get_sha256 "x86_64-unknown-linux-gnu")

log_step "SHA256 checksums found:"
echo "  macOS ARM64:  $SHA256_MACOS_ARM"
echo "  macOS x86_64: $SHA256_MACOS_X86"
echo "  Linux ARM64:  $SHA256_LINUX_ARM"
echo "  Linux x86_64: $SHA256_LINUX_X86"

log_step "Updating formula..."

# Update the formula file
sed -i.bak \
    -e "s/PLACEHOLDER_SHA256_ARM64/$SHA256_MACOS_ARM/g" \
    -e "s/PLACEHOLDER_SHA256_X86_64/$SHA256_MACOS_X86/g" \
    -e "s/PLACEHOLDER_SHA256_LINUX_ARM64/$SHA256_LINUX_ARM/g" \
    -e "s/PLACEHOLDER_SHA256_LINUX_X86_64/$SHA256_LINUX_X86/g" \
    "$FORMULA_PATH"

rm -f "${FORMULA_PATH}.bak"

log_info "Formula updated successfully!"
log_info "Review changes in: $FORMULA_PATH"

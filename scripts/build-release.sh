#!/usr/bin/env bash
#
# Build release binaries for all supported platforms
# This script is used to create release artifacts locally
#
# Usage: ./scripts/build-release.sh [version]
# Example: ./scripts/build-release.sh 0.3.0

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1" >&2; }
log_step() { echo -e "${BLUE}==>${NC} $1"; }

# Get version from argument or Cargo.toml
if [ -n "${1:-}" ]; then
    VERSION="$1"
else
    VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
fi

log_info "Building dotdipper v$VERSION"

# Create release directory
RELEASE_DIR="release-v$VERSION"
rm -rf "$RELEASE_DIR"
mkdir -p "$RELEASE_DIR"

# Detect current platform
CURRENT_OS=$(uname -s)
CURRENT_ARCH=$(uname -m)

log_info "Current platform: $CURRENT_OS $CURRENT_ARCH"

# macOS targets
MACOS_TARGETS=("x86_64-apple-darwin" "aarch64-apple-darwin")

# Linux targets (only if on Linux or with cross-compilation)
LINUX_TARGETS=("x86_64-unknown-linux-gnu")

build_target() {
    local target=$1
    local artifact_name="dotdipper-$target"
    
    log_step "Building for $target..."
    
    # Check if we need to add the target
    if ! rustup target list --installed | grep -q "$target"; then
        log_info "Adding target $target..."
        rustup target add "$target"
    fi
    
    # Build
    if cargo build --release --target "$target"; then
        log_info "Build successful for $target"
        
        # Create tarball
        local binary_path="target/$target/release/dotdipper"
        if [ -f "$binary_path" ]; then
            mkdir -p "$RELEASE_DIR/tmp"
            cp "$binary_path" "$RELEASE_DIR/tmp/dotdipper"
            
            cd "$RELEASE_DIR/tmp"
            tar -czvf "../$artifact_name.tar.gz" dotdipper
            cd ../..
            
            rm -rf "$RELEASE_DIR/tmp"
            
            # Generate SHA256
            if command -v shasum &> /dev/null; then
                shasum -a 256 "$RELEASE_DIR/$artifact_name.tar.gz" > "$RELEASE_DIR/$artifact_name.tar.gz.sha256"
            elif command -v sha256sum &> /dev/null; then
                sha256sum "$RELEASE_DIR/$artifact_name.tar.gz" > "$RELEASE_DIR/$artifact_name.tar.gz.sha256"
            fi
            
            log_info "Created $RELEASE_DIR/$artifact_name.tar.gz"
        else
            log_error "Binary not found at $binary_path"
            return 1
        fi
    else
        log_error "Build failed for $target"
        return 1
    fi
}

# Build macOS targets (if on macOS)
if [ "$CURRENT_OS" = "Darwin" ]; then
    for target in "${MACOS_TARGETS[@]}"; do
        build_target "$target" || true
    done
fi

# Build Linux targets (if on Linux)
if [ "$CURRENT_OS" = "Linux" ]; then
    for target in "${LINUX_TARGETS[@]}"; do
        build_target "$target" || true
    done
fi

# Summary
log_step "Release artifacts created in $RELEASE_DIR/"
ls -la "$RELEASE_DIR/"

# Generate Homebrew formula update snippet
log_step "Homebrew formula SHA256 values:"
echo ""
for file in "$RELEASE_DIR"/*.sha256; do
    if [ -f "$file" ]; then
        cat "$file"
    fi
done

echo ""
log_info "To update the Homebrew formula, replace the sha256 values in:"
log_info "  homebrew-tap/Formula/dotdipper.rb"
echo ""
log_info "Done! Release artifacts are in $RELEASE_DIR/"

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

# Extract SHA256 values
get_sha256() {
    local file="$RELEASE_DIR/dotdipper-$1.tar.gz.sha256"
    if [ -f "$file" ]; then
        awk '{print $1}' "$file"
    else
        echo ""
    fi
}

SHA256_MACOS_ARM=$(get_sha256 "aarch64-apple-darwin")
SHA256_MACOS_X86=$(get_sha256 "x86_64-apple-darwin")
SHA256_LINUX_ARM=$(get_sha256 "aarch64-unknown-linux-gnu")
SHA256_LINUX_X86=$(get_sha256 "x86_64-unknown-linux-gnu")

log_step "SHA256 checksums found:"
[ -n "$SHA256_MACOS_ARM" ] && echo "  macOS ARM64:  $SHA256_MACOS_ARM" || echo "  macOS ARM64:  (not found)"
[ -n "$SHA256_MACOS_X86" ] && echo "  macOS x86_64: $SHA256_MACOS_X86" || echo "  macOS x86_64: (not found)"
[ -n "$SHA256_LINUX_ARM" ] && echo "  Linux ARM64:  $SHA256_LINUX_ARM" || echo "  Linux ARM64:  (not found)"
[ -n "$SHA256_LINUX_X86" ] && echo "  Linux x86_64: $SHA256_LINUX_X86" || echo "  Linux x86_64: (not found)"

# Check if we have at least macOS checksums (required)
if [ -z "$SHA256_MACOS_ARM" ] && [ -z "$SHA256_MACOS_X86" ]; then
    log_error "No macOS checksums found. Cannot update formula."
    exit 1
fi

log_step "Generating updated formula..."

# Generate the new formula
cat > "$FORMULA_PATH" << FORMULA_EOF
# Homebrew Formula for Dotdipper
# This formula installs pre-built binaries for macOS and Linux
#
# To use this tap:
#   brew tap psyysp/dotdipper
#   brew install dotdipper
#
# Or install directly:
#   brew install psyysp/dotdipper/dotdipper

class Dotdipper < Formula
  desc "A safe, deterministic, and feature-rich dotfiles manager built in Rust"
  homepage "https://github.com/psyysp/dotdipper"
  version "$VERSION"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/psyysp/dotdipper/releases/download/v#{version}/dotdipper-aarch64-apple-darwin.tar.gz"
      sha256 "${SHA256_MACOS_ARM:-PLACEHOLDER_NEEDS_UPDATE}"
    else
      url "https://github.com/psyysp/dotdipper/releases/download/v#{version}/dotdipper-x86_64-apple-darwin.tar.gz"
      sha256 "${SHA256_MACOS_X86:-PLACEHOLDER_NEEDS_UPDATE}"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/psyysp/dotdipper/releases/download/v#{version}/dotdipper-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "${SHA256_LINUX_ARM:-PLACEHOLDER_NEEDS_UPDATE}"
    else
      url "https://github.com/psyysp/dotdipper/releases/download/v#{version}/dotdipper-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "${SHA256_LINUX_X86:-PLACEHOLDER_NEEDS_UPDATE}"
    end
  end

  # Age is required for secrets encryption feature
  depends_on "age"

  def install
    bin.install "dotdipper"
  end

  def caveats
    <<~EOS
      Dotdipper has been installed!

      To get started:
        dotdipper init

      For help:
        dotdipper --help

      For secrets encryption, 'age' has been installed as a dependency.
      To set up secrets encryption:
        dotdipper secrets init
    EOS
  end

  test do
    # Test that the binary runs and shows version
    assert_match "dotdipper", shell_output("#{bin}/dotdipper --version")
    
    # Test that help works
    assert_match "dotfiles manager", shell_output("#{bin}/dotdipper --help")
  end
end
FORMULA_EOF

log_info "Formula updated successfully!"
echo ""

# Check for placeholder values
if grep -q "PLACEHOLDER_NEEDS_UPDATE" "$FORMULA_PATH"; then
    log_warn "Some SHA256 values are missing (marked as PLACEHOLDER_NEEDS_UPDATE)"
    log_warn "These will need to be updated before the formula works"
fi

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

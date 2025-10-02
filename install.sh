#!/usr/bin/env bash
#
# Dotdipper Installation Script
# This script installs Rust (if needed) and builds/installs dotdipper
#

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Helper functions
log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1" >&2
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_step() {
    echo -e "${BLUE}==>${NC} $1"
}

# Check if command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Detect OS
detect_os() {
    case "$(uname -s)" in
        Linux*)     echo "Linux";;
        Darwin*)    echo "Mac";;
        CYGWIN*)    echo "Cygwin";;
        MINGW*)     echo "MinGw";;
        *)          echo "UNKNOWN";;
    esac
}

OS=$(detect_os)
log_info "Detected OS: $OS"

# Check for Rust installation
log_step "Checking for Rust installation..."

if ! command_exists rustc || ! command_exists cargo; then
    log_warn "Rust is not installed"
    
    read -p "Would you like to install Rust now? (y/n) " -n 1 -r
    echo
    
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        log_info "Installing Rust via rustup..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        
        # Source cargo env
        source "$HOME/.cargo/env"
        
        log_info "Rust installed successfully"
    else
        log_error "Rust is required to build dotdipper. Please install Rust and try again."
        log_info "Visit https://rustup.rs for installation instructions"
        exit 1
    fi
else
    RUST_VERSION=$(rustc --version | cut -d' ' -f2)
    log_info "Rust is installed (version $RUST_VERSION)"
fi

# Build dotdipper
log_step "Building dotdipper..."

if ! cargo build --release; then
    log_error "Failed to build dotdipper"
    exit 1
fi

log_info "Build successful"

# Install dotdipper
log_step "Installing dotdipper..."

INSTALL_PATH="$HOME/.cargo/bin"
mkdir -p "$INSTALL_PATH"

cp target/release/dotdipper "$INSTALL_PATH/"
chmod +x "$INSTALL_PATH/dotdipper"

log_info "Dotdipper installed to $INSTALL_PATH/dotdipper"

# Check if cargo bin is in PATH
if ! echo "$PATH" | grep -q "$HOME/.cargo/bin"; then
    log_warn "$HOME/.cargo/bin is not in your PATH"
    log_info "Add the following to your shell configuration file:"
    echo "    export PATH=\"\$HOME/.cargo/bin:\$PATH\""
    
    # Try to add to shell config
    SHELL_CONFIG=""
    if [[ -f "$HOME/.zshrc" ]]; then
        SHELL_CONFIG="$HOME/.zshrc"
    elif [[ -f "$HOME/.bashrc" ]]; then
        SHELL_CONFIG="$HOME/.bashrc"
    elif [[ -f "$HOME/.profile" ]]; then
        SHELL_CONFIG="$HOME/.profile"
    fi
    
    if [[ -n "$SHELL_CONFIG" ]]; then
        read -p "Would you like to add this to $SHELL_CONFIG automatically? (y/n) " -n 1 -r
        echo
        
        if [[ $REPLY =~ ^[Yy]$ ]]; then
            echo "" >> "$SHELL_CONFIG"
            echo "# Added by dotdipper installer" >> "$SHELL_CONFIG"
            echo "export PATH=\"\$HOME/.cargo/bin:\$PATH\"" >> "$SHELL_CONFIG"
            log_info "Added to $SHELL_CONFIG. Please restart your terminal or run:"
            echo "    source $SHELL_CONFIG"
        fi
    fi
fi

# Verify installation
if command_exists dotdipper || [[ -x "$INSTALL_PATH/dotdipper" ]]; then
    log_info "✨ Dotdipper installed successfully!"
    echo
    echo "To get started, run:"
    echo "    dotdipper init"
    echo
    echo "For help, run:"
    echo "    dotdipper --help"
else
    log_error "Installation verification failed"
    exit 1
fi

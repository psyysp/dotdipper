#!/usr/bin/env bash
#
# Bring a fresh Mac up on a dotdipper repository: Homebrew, dotdipper, the
# dotfiles, the tools they expect, and the macOS preferences.
#
#   ./bootstrap.sh --user <gh-user> --repo <repo> [--https]
#
# --https clones over anonymous HTTPS, for a machine with no GitHub sign-in
# and no SSH key. Without it the repo is reached over SSH, which is what a
# private repository requires.
#
# Every step is idempotent: re-running after a failure picks up where it
# stopped rather than starting over.
set -euo pipefail

USER_NAME=""
REPO_NAME=""
HTTPS=0
RUN_APPS=1
RUN_MACOS=1

log()  { printf '\033[0;32m==>\033[0m %s\n' "$1"; }
warn() { printf '\033[1;33m==>\033[0m %s\n' "$1" >&2; }
die()  { printf '\033[0;31m==>\033[0m %s\n' "$1" >&2; exit 1; }

usage() {
    cat <<'USAGE'
Usage: bootstrap.sh --user <gh-user> --repo <repo> [options]

  --user <gh-user>   GitHub account that owns the dotfiles repository
  --repo <repo>      Repository name
  --https            Clone over anonymous HTTPS (no account, no SSH key).
                     Use this for a public mirror. Omit it for a private repo.
  --no-apps          Skip installing Homebrew packages and App Store titles
  --no-macos         Skip applying macOS preferences
  -h, --help         Show this message
USAGE
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --user)     USER_NAME="${2:-}"; shift 2 ;;
        --repo)     REPO_NAME="${2:-}"; shift 2 ;;
        --https)    HTTPS=1; shift ;;
        --no-apps)  RUN_APPS=0; shift ;;
        --no-macos) RUN_MACOS=0; shift ;;
        -h|--help)  usage; exit 0 ;;
        *)          usage >&2; die "Unknown argument: $1" ;;
    esac
done

[[ -n "$USER_NAME" ]] || { usage >&2; die "--user is required."; }
[[ -n "$REPO_NAME" ]] || { usage >&2; die "--repo is required."; }
[[ "$(uname -s)" == "Darwin" ]] || die "This script targets macOS."

# ---------------------------------------------------------------------------
# Homebrew. Its installer needs sudo to create and chown the prefix; that is
# the one unavoidable password prompt. brew itself must never run under sudo.
# ---------------------------------------------------------------------------
if ! command -v brew >/dev/null 2>&1; then
    for prefix in /opt/homebrew /usr/local; do
        if [[ -x "$prefix/bin/brew" ]]; then
            eval "$("$prefix/bin/brew" shellenv)"
            break
        fi
    done
fi

if ! command -v brew >/dev/null 2>&1; then
    log "Installing Homebrew (this will ask for your password)"
    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
    for prefix in /opt/homebrew /usr/local; do
        [[ -x "$prefix/bin/brew" ]] && eval "$("$prefix/bin/brew" shellenv)" && break
    done
fi
command -v brew >/dev/null 2>&1 || die "Homebrew installed but not on PATH."
log "Homebrew: $(brew --version | head -1)"

# ---------------------------------------------------------------------------
# dotdipper
# ---------------------------------------------------------------------------
if command -v dotdipper >/dev/null 2>&1; then
    log "dotdipper already installed: $(dotdipper --version)"
else
    log "Installing dotdipper"
    brew install psyysp/dotdipper/dotdipper
fi

# A stale copy earlier on PATH silently shadows the one just installed.
BREW_DOTDIPPER="$(brew --prefix)/bin/dotdipper"
if [[ -x "$BREW_DOTDIPPER" && "$(command -v dotdipper)" != "$BREW_DOTDIPPER" ]]; then
    warn "$(command -v dotdipper) shadows $BREW_DOTDIPPER on your PATH."
    warn "Using the Homebrew copy for this run."
fi
DOTDIPPER="${BREW_DOTDIPPER:-dotdipper}"
[[ -x "$DOTDIPPER" ]] || DOTDIPPER="$(command -v dotdipper)"

# ---------------------------------------------------------------------------
# Configure and pull
# ---------------------------------------------------------------------------
CONFIG="${DOTDIPPER_HOME:-$HOME/.config/dotdipper}/config.toml"
if [[ -f "$CONFIG" ]]; then
    log "Config already present; leaving it in place"
else
    log "Initializing dotdipper"
    "$DOTDIPPER" init
fi

"$DOTDIPPER" config --set "github.username=$USER_NAME" >/dev/null
"$DOTDIPPER" config --set "github.repo_name=$REPO_NAME" >/dev/null
log "Configured for $USER_NAME/$REPO_NAME"

if [[ "$HTTPS" -eq 1 ]]; then
    log "Pulling over anonymous HTTPS and applying"
    "$DOTDIPPER" pull --https --apply
else
    log "Pulling over SSH and applying"
    "$DOTDIPPER" pull --apply
fi

# ---------------------------------------------------------------------------
# Tools. Cask installers that ship a .pkg each prompt for a password, so take
# one authorization up front and refresh it while the run is in flight.
# ---------------------------------------------------------------------------
if [[ "$RUN_APPS" -eq 1 ]]; then
    # A public mirror publishes a ready-made install script; a private repo
    # publishes the Brewfile it is derived from. Prefer regenerating from the
    # Brewfile when one was pulled, so a stale script from an earlier run on
    # this machine never wins over the inventory that just arrived.
    APPS_SCRIPT="$HOME/install-apps.sh"
    STORE="${DOTDIPPER_HOME:-$HOME/.config/dotdipper}/profiles/default/compiled"
    if [[ -f "$STORE/Brewfile" ]]; then
        log "Generating the install script from the pulled Brewfile"
        "$DOTDIPPER" install apps-script -o "$APPS_SCRIPT"
    elif [[ ! -f "$APPS_SCRIPT" ]]; then
        die "No install script and no Brewfile in the repository; re-run with --no-apps."
    else
        log "Using the install script published in the repository"
    fi

    log "Authorizing sudo for the cask installers"
    sudo -v
    while true; do
        sudo -n true
        sleep 60
        kill -0 "$$" 2>/dev/null || exit
    done &
    SUDO_KEEPALIVE=$!
    trap 'kill "$SUDO_KEEPALIVE" 2>/dev/null || true' EXIT

    log "Installing tools (failures are collected, not fatal)"
    bash "$APPS_SCRIPT" || warn "Some tools did not install; see the list above."

    kill "$SUDO_KEEPALIVE" 2>/dev/null || true
    trap - EXIT
fi

# ---------------------------------------------------------------------------
# macOS preferences. No sudo here: the domains that refuse writes are guarded
# by privacy controls, which sudo does not satisfy.
# ---------------------------------------------------------------------------
if [[ "$RUN_MACOS" -eq 1 ]]; then
    DEFAULTS="$HOME/.config/macos/defaults.sh"
    if [[ -f "$DEFAULTS" ]]; then
        log "Applying macOS preferences"
        bash "$DEFAULTS" || true
    else
        warn "No $DEFAULTS in this repository; skipping preferences."
    fi
fi

log "Done. Open a new terminal so the shell configuration takes effect."

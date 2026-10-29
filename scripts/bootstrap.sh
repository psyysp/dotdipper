#!/usr/bin/env bash
#
# Bring a fresh Mac up on a dotdipper repository: Homebrew, dotdipper, the
# dotfiles, the tools they expect, and the macOS preferences.
#
#   ./bootstrap.sh <source>
#   ./bootstrap.sh --user <gh-user> --repo <repo> [--https]
#
# <source> is anything `dotdipper init` accepts: owner/repo, or an https://
# or git@ URL on any host.
#
# --https clones over anonymous HTTPS, for a machine with no GitHub sign-in
# and no SSH key. Without it the repo is reached over SSH, which is what a
# private repository requires.
#
# Every step is idempotent: re-running after a failure picks up where it
# stopped rather than starting over.
set -euo pipefail

SOURCE=""
INIT_EXTRA=()
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
Usage: bootstrap.sh <source> [options]
       bootstrap.sh --user <gh-user> --repo <repo> [options]

  <source>           owner/repo, or an https:// or git@ URL on any host
  --user <gh-user>   GitHub account that owns the dotfiles repository
  --repo <repo>      Repository name
  --https            Clone over anonymous HTTPS (no account, no SSH key).
                     Use this for a public mirror. Omit it for a private repo.
  --no-apps          Skip installing Homebrew packages and App Store titles
                     (run 'dotdipper install apps' later to choose them)
  --no-macos         Skip applying macOS preferences
  -i, --interactive  Choose the files to apply from a checklist
  --adopt-config     Also apply the repository's own dotdipper settings and
                     hooks. Use it for your own repository.
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
        --adopt-config) INIT_EXTRA+=(--adopt-config); shift ;;
        -i|--interactive) INIT_EXTRA+=(--interactive); shift ;;
        -h|--help)  usage; exit 0 ;;
        -*)         usage >&2; die "Unknown argument: $1" ;;
        *)          [[ -z "$SOURCE" ]] || die "Only one source may be given."
                    SOURCE="$1"; shift ;;
    esac
done

if [[ -z "$SOURCE" ]]; then
    [[ -n "$USER_NAME" ]] || { usage >&2; die "A source, or --user and --repo, is required."; }
    [[ -n "$REPO_NAME" ]] || { usage >&2; die "--repo is required."; }
    if [[ "$HTTPS" -eq 1 ]]; then
        SOURCE="$USER_NAME/$REPO_NAME"
    else
        SOURCE="git@github.com:$USER_NAME/$REPO_NAME.git"
    fi
fi
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
# A first run clones through `init`, which previews what the repository
# would write and asks before writing it. A re-run finds the store already
# cloned and pulls instead, so the script stays idempotent.
STORE_GIT="${DOTDIPPER_HOME:-$HOME/.config/dotdipper}/profiles/default/compiled/.git"
if [[ -d "$STORE_GIT" ]]; then
    log "Store already cloned; pulling and applying"
    "$DOTDIPPER" config --set "github.url=$SOURCE" >/dev/null 2>&1 || true
    "$DOTDIPPER" pull --apply
else
    log "Setting up from $SOURCE"
    "$DOTDIPPER" init "$SOURCE" --apply ${INIT_EXTRA[@]+"${INIT_EXTRA[@]}"}
fi

# ---------------------------------------------------------------------------
# Tools. `dotdipper install apps` looks every entry up, shows one checklist,
# asks for confirmation and at most one password up front, and then installs
# unattended. It holds the sudo authorization itself, only when a selected
# item needs it, and never runs brew under sudo. It reads the Brewfile as
# text; nothing in the repository is evaluated.
# ---------------------------------------------------------------------------
if [[ "$RUN_APPS" -eq 1 ]]; then
    STORE="${DOTDIPPER_HOME:-$HOME/.config/dotdipper}/profiles/default/compiled"
    if [[ -f "$STORE/Brewfile" || -f "$STORE/apps_manifest.toml" ]]; then
        log "Installing tools"
        "$DOTDIPPER" install apps || warn "Some tools did not install; see the report above."
    elif [[ -f "$HOME/install-apps.sh" ]]; then
        # A public mirror ships a generated script instead of its inventory.
        # It came from the repository, so it is not run on your behalf.
        warn "The repository provides $HOME/install-apps.sh but no Brewfile."
        warn "Read it, then run it yourself: bash ~/install-apps.sh"
    else
        warn "No Brewfile or apps manifest in this repository; skipping tools."
    fi
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

#!/usr/bin/env bash
#
# Rebuild dotdipper from this checkout and make it the `dotdipper` on PATH,
# so a code change can be tried immediately.
#
#   scripts/dev-install.sh            # build (release profile) and install once
#   scripts/dev-install.sh --check    # run scripts/check.sh first; install only if it passes
#   scripts/dev-install.sh --watch    # rebuild and reinstall whenever src/ changes
#   scripts/dev-install.sh --debug    # faster builds, slower binary
#   scripts/dev-install.sh --restore  # remove the dev build and go back to Homebrew's
#
# The dev build is installed to ~/.cargo/bin. When Homebrew's copy is earlier
# on PATH it would keep winning, so the script unlinks the Homebrew formula
# (the Cellar copy is kept) and --restore links it back. Builds reuse this
# checkout's target/ directory, so a rebuild after a small change is
# incremental rather than the full rebuild `cargo install` does.
set -euo pipefail

cd "$(dirname "$0")/.."

PROFILE=release
CHECK=0
WATCH=0
RESTORE=0
DEST_DIR="${CARGO_HOME:-$HOME/.cargo}/bin"
DEST="$DEST_DIR/dotdipper"

log()  { printf '\033[0;32m==>\033[0m %s\n' "$1"; }
warn() { printf '\033[1;33m==>\033[0m %s\n' "$1" >&2; }
die()  { printf '\033[0;31m==>\033[0m %s\n' "$1" >&2; exit 1; }

while [[ $# -gt 0 ]]; do
    case "$1" in
        --check)   CHECK=1; shift ;;
        --watch)   WATCH=1; shift ;;
        --debug)   PROFILE=debug; shift ;;
        --restore) RESTORE=1; shift ;;
        -h|--help) sed -n '3,16p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *)         die "Unknown argument: $1 (see --help)" ;;
    esac
done

brew_has_formula() {
    command -v brew >/dev/null 2>&1 && brew list --formula dotdipper >/dev/null 2>&1
}

if [[ "$RESTORE" -eq 1 ]]; then
    if [[ -f "$DEST" ]]; then
        rm -f "$DEST"
        log "Removed the dev build at $DEST"
    fi
    if brew_has_formula; then
        brew link --overwrite dotdipper >/dev/null
        log "Relinked Homebrew's dotdipper"
    fi
    hash -r 2>/dev/null || true
    log "Now using: $(command -v dotdipper 2>/dev/null || echo 'none') ($(dotdipper --version 2>/dev/null || echo 'not found'))"
    exit 0
fi

build_and_install() {
    local flags=(--locked)
    [[ "$PROFILE" == release ]] && flags+=(--release)
    if ! cargo build "${flags[@]}"; then
        warn "Build failed; the installed binary was left unchanged."
        return 1
    fi
    mkdir -p "$DEST_DIR"
    # Copy beside the target, then rename: a shell running the old binary
    # never sees a half-written file.
    cp "target/$PROFILE/dotdipper" "$DEST_DIR/.dotdipper.new"
    mv -f "$DEST_DIR/.dotdipper.new" "$DEST"
    # macOS kills an unsigned binary whose pages changed under an existing
    # code signature; an ad-hoc signature after the copy avoids that.
    if [[ "$(uname -s)" == Darwin ]]; then
        codesign --force --sign - "$DEST" >/dev/null 2>&1 || true
    fi
    log "Installed $("$DEST" --version) ($PROFILE, $(git rev-parse --short HEAD)$(git diff --quiet HEAD -- || echo '+dirty')) to $DEST"
}

ensure_on_path() {
    local resolved
    resolved="$(command -v dotdipper 2>/dev/null || true)"
    if [[ "$resolved" == "$DEST" ]]; then
        return
    fi
    if brew_has_formula && [[ "$resolved" == "$(brew --prefix)/bin/dotdipper" ]]; then
        brew unlink dotdipper >/dev/null
        log "Unlinked Homebrew's dotdipper so the dev build wins (undo: $0 --restore)"
        hash -r 2>/dev/null || true
        resolved="$(command -v dotdipper 2>/dev/null || true)"
    fi
    if [[ "$resolved" != "$DEST" ]]; then
        warn "'dotdipper' on PATH is ${resolved:-missing}, not the dev build."
        warn "Put $DEST_DIR earlier on PATH, or run $DEST directly."
    fi
}

if [[ "$CHECK" -eq 1 ]]; then
    log "Running scripts/check.sh"
    ./scripts/check.sh
fi

build_and_install
ensure_on_path

if [[ "$WATCH" -eq 1 ]]; then
    log "Watching src/, Cargo.toml and Cargo.lock; Ctrl-C to stop"
    stamp="$(mktemp)"
    trap 'rm -f "$stamp"' EXIT
    while true; do
        sleep 1
        if [[ -n "$(find src Cargo.toml Cargo.lock -newer "$stamp" -type f -print -quit 2>/dev/null)" ]]; then
            touch "$stamp"
            log "Change detected; rebuilding"
            build_and_install || true
        fi
    done
fi

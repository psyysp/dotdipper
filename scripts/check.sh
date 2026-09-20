#!/usr/bin/env bash
# Everything CI enforces, in CI's order, before you push.
#
# CI fails on formatting first, and a formatting failure cancels the rest of
# the matrix — so a red build tells you nothing about your actual change. Run
# this instead of remembering four commands.
set -euo pipefail

cd "$(dirname "$0")/.."

run() {
    printf '\n\033[1m== %s\033[0m\n' "$1"
    shift
    "$@"
}

run "formatting"     cargo fmt -- --check
run "clippy"         cargo clippy --all-targets --locked -- -D warnings
run "clippy (no default features)" \
                     cargo clippy --no-default-features --locked -- -D warnings
run "tests"          cargo test --locked

printf '\n\033[32m✓ all checks passed\033[0m\n'

#!/usr/bin/env bash
#
# In-container Linux e2e suite for dotdipper.
# Invoked by scripts/container-test.sh; not meant to be run on the host.
#
set -euo pipefail

SRC="${SRC:-/src}"
BUILD="${CARGO_TARGET_DIR:-/build}"
BIN="${BUILD}/debug/dotdipper"

MACHINE_A_HOME="/tmp/machine-a"
MACHINE_B_HOME="/tmp/machine-b"
REMOTE="/srv/dotfiles.git"
EXPECTED="/tmp/e2e-expected"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

TESTS_RUN=0

pass() {
  echo -e "${GREEN}PASS${NC}: $*"
}

fail() {
  echo -e "${RED}FAIL${NC}: $*" >&2
  exit 1
}

assert_eq() {
  local actual=$1
  local expected=$2
  local msg=${3:-"values differ"}
  if [[ "$actual" != "$expected" ]]; then
    echo -e "${RED}FAIL${NC}: ${msg}" >&2
    echo "  expected: ${expected}" >&2
    echo "  actual:   ${actual}" >&2
    exit 1
  fi
}

assert_file_exists() {
  local path=$1
  local msg=${2:-"missing file ${path}"}
  if [[ ! -e "$path" ]]; then
    fail "$msg"
  fi
}

assert_file_matches() {
  local actual=$1
  local expected=$2
  local msg=${3:-"${actual} vs ${expected}"}
  if [[ ! -e "$actual" ]]; then
    fail "${msg} (missing actual file ${actual})"
  fi
  if [[ ! -e "$expected" ]]; then
    fail "${msg} (missing expected file ${expected})"
  fi
  if ! cmp -s "$actual" "$expected"; then
    echo -e "${RED}FAIL${NC}: ${msg} (byte mismatch)" >&2
    echo "  --- ${expected}" >&2
    echo "  +++ ${actual}" >&2
    diff -u "$expected" "$actual" >&2 || true
    exit 1
  fi
}

# Run a command that must fail. Needle must appear in combined stdout+stderr.
# Usage: assert_cmd_fails <needle> <cmd> [args...]
assert_cmd_fails() {
  local needle=$1
  shift
  local out ec
  set +e
  out=$("$@" 2>&1)
  ec=$?
  set -e
  if [[ $ec -eq 0 ]]; then
    echo -e "${RED}FAIL${NC}: expected nonzero exit: $*" >&2
    echo "$out" >&2
    exit 1
  fi
  if [[ "$out" != *"$needle"* ]]; then
    echo -e "${RED}FAIL${NC}: output missing '${needle}' from: $*" >&2
    echo "$out" >&2
    exit 1
  fi
}

begin_test() {
  TESTS_RUN=$((TESTS_RUN + 1))
  echo
  echo -e "${YELLOW}==>${NC} ${TESTS_RUN}. $*"
}

use_machine() {
  local home=$1
  export HOME="$home"
  export DOTDIPPER_HOME="${home}/.config/dotdipper"
  export DOTDIPPER_TEST_REMOTE="$REMOTE"
  unset XDG_CONFIG_HOME || true
  unset DOTDIPPER_PROFILE || true
}

write_fixtures() {
  local home=$1
  mkdir -p \
    "${home}/.config/kitty" \
    "${home}/.config/nvim/lua/config"

  cat > "${home}/.zshrc" <<'EOF'
# E2E_ZSHRC_MARKER=alpha-7f3c9e2b
export EDITOR=nvim
alias ll='ls -la'
EOF

  cat > "${home}/.gitconfig" <<'EOF'
# E2E_GITCONFIG_MARKER=dotdipper-container-e2e-gitconfig-9f2a
[user]
	name = e2e-tester
	email = e2e@dotdipper.test
[color]
	ui = auto
EOF

  cat > "${home}/.config/kitty/kitty.conf" <<'EOF'
# E2E_KITTY_MARKER=kitty-font-size-13-e2e
font_family JetBrains Mono
font_size 13.0
background_opacity 0.95
EOF

  cat > "${home}/.config/nvim/init.lua" <<'EOF'
-- E2E_NVIM_INIT_MARKER=init-lua-e2e-c4d8
vim.g.mapleader = " "
require("config.options")
EOF

  cat > "${home}/.config/nvim/lua/config/options.lua" <<'EOF'
-- E2E_NVIM_OPTIONS_MARKER=nested-lua-path-e2e-aa11
vim.opt.number = true
vim.opt.relativenumber = true
vim.opt.expandtab = true
EOF
}

snapshot_expected() {
  local home=$1
  mkdir -p \
    "${EXPECTED}/.config/kitty" \
    "${EXPECTED}/.config/nvim/lua/config"
  cp -a "${home}/.zshrc" "${EXPECTED}/.zshrc"
  cp -a "${home}/.gitconfig" "${EXPECTED}/.gitconfig"
  cp -a "${home}/.config/kitty/kitty.conf" "${EXPECTED}/.config/kitty/kitty.conf"
  cp -a "${home}/.config/nvim/init.lua" "${EXPECTED}/.config/nvim/init.lua"
  cp -a "${home}/.config/nvim/lua/config/options.lua" "${EXPECTED}/.config/nvim/lua/config/options.lua"
}

assert_tracked_contains() {
  local overlay=$1
  shift
  local path
  for path in "$@"; do
    if ! grep -F -q "$path" "$overlay"; then
      echo -e "${RED}FAIL${NC}: overlay ${overlay} missing tracked path ${path}" >&2
      echo "----- overlay -----" >&2
      cat "$overlay" >&2
      exit 1
    fi
  done
}

assert_home_matches_expected() {
  local home=$1
  assert_file_matches "${home}/.zshrc" "${EXPECTED}/.zshrc" "${home}/.zshrc"
  assert_file_matches "${home}/.gitconfig" "${EXPECTED}/.gitconfig" "${home}/.gitconfig"
  assert_file_matches \
    "${home}/.config/kitty/kitty.conf" \
    "${EXPECTED}/.config/kitty/kitty.conf" \
    "${home}/.config/kitty/kitty.conf"
  assert_file_matches \
    "${home}/.config/nvim/init.lua" \
    "${EXPECTED}/.config/nvim/init.lua" \
    "${home}/.config/nvim/init.lua"
  assert_file_matches \
    "${home}/.config/nvim/lua/config/options.lua" \
    "${EXPECTED}/.config/nvim/lua/config/options.lua" \
    "nested nvim lua path ${home}/.config/nvim/lua/config/options.lua"
}

# ---------------------------------------------------------------------------
# 0. Packages
# ---------------------------------------------------------------------------
echo "==> Preparing container packages"
if ! command -v git >/dev/null 2>&1; then
  echo "git missing; installing..."
  apt-get update
  apt-get install -y git
fi
command -v git >/dev/null 2>&1 || fail "git is not available after install"
git --version

# openssl-sys / pkg-config are needed for default features (reqwest, rust-s3)
if ! command -v pkg-config >/dev/null 2>&1 || [[ ! -f /usr/include/openssl/ssl.h ]]; then
  apt-get update
  apt-get install -y pkg-config libssl-dev
fi

# ---------------------------------------------------------------------------
# 1. Build on Linux
# ---------------------------------------------------------------------------
begin_test "Build succeeds on Linux (debug)"
cd "$SRC"
export CARGO_TARGET_DIR="$BUILD"
cargo build
assert_file_exists "$BIN" "debug binary missing at ${BIN}"
"$BIN" --version
pass "1. cargo build (Linux debug)"

# ---------------------------------------------------------------------------
# 2. apps subcommand is compile-time gated out
# ---------------------------------------------------------------------------
begin_test "dotdipper apps is absent on Linux"
assert_cmd_fails "unrecognized subcommand" "$BIN" apps capture

HELP_OUT=$("$BIN" --help 2>&1)
if echo "$HELP_OUT" | grep -Eiq '^[[:space:]]*apps[[:space:]]'; then
  echo "$HELP_OUT" >&2
  fail "2. --help listed an 'apps' subcommand on Linux"
fi
if echo "$HELP_OUT" | grep -Fq 'Capture and restore installed macOS applications'; then
  echo "$HELP_OUT" >&2
  fail "2. --help still describes the macOS apps command"
fi
pass "2. apps subcommand absent (clap error + --help)"

# ---------------------------------------------------------------------------
# 3. Machine A: discover + push to local bare remote
# ---------------------------------------------------------------------------
begin_test "Machine A: init, discover, push to bare remote"

rm -rf "$MACHINE_A_HOME" "$EXPECTED"
mkdir -p "$MACHINE_A_HOME"
use_machine "$MACHINE_A_HOME"
write_fixtures "$MACHINE_A_HOME"

git config --global user.name "e2e-tester"
git config --global user.email "e2e@dotdipper.test"

grep -Fq "E2E_GITCONFIG_MARKER=dotdipper-container-e2e-gitconfig-9f2a" \
  "${MACHINE_A_HOME}/.gitconfig" \
  || fail "git config --global dropped the unique .gitconfig marker"

snapshot_expected "$MACHINE_A_HOME"

rm -rf "$REMOTE"
mkdir -p "$(dirname "$REMOTE")"
git init --bare --initial-branch=main "$REMOTE"

"$BIN" init
"$BIN" discover --write

OVERLAY="${DOTDIPPER_HOME}/profiles/default/config.toml"
assert_file_exists "$OVERLAY" "profile overlay missing at ${OVERLAY}"
assert_tracked_contains "$OVERLAY" \
  "${MACHINE_A_HOME}/.zshrc" \
  "${MACHINE_A_HOME}/.gitconfig" \
  "${MACHINE_A_HOME}/.config/kitty/kitty.conf" \
  "${MACHINE_A_HOME}/.config/nvim/init.lua" \
  "${MACHINE_A_HOME}/.config/nvim/lua/config/options.lua"

"$BIN" config --set github.username=e2e-user
"$BIN" config --set github.repo_name=e2e-dotfiles

export DOTDIPPER_TEST_REMOTE="$REMOTE"
"$BIN" push -m "e2e"
assert_eq "$?" "0" "dotdipper push -m e2e exit code"

TREE=$(git --git-dir="$REMOTE" ls-tree -r --name-only main)
echo "$TREE"
for rel in \
  .zshrc \
  .gitconfig \
  .config/kitty/kitty.conf \
  .config/nvim/init.lua \
  .config/nvim/lua/config/options.lua \
  manifest.lock
do
  if ! echo "$TREE" | grep -Fxq "$rel"; then
    echo "$TREE" >&2
    fail "bare repo main branch missing ${rel}"
  fi
done
pass "3. Machine A discover + push (ls-tree includes fixtures + manifest.lock)"

# ---------------------------------------------------------------------------
# 4. Machine B: fresh machine pull --apply --force
# ---------------------------------------------------------------------------
begin_test "Machine B: fresh HOME pull --apply --force"

rm -rf "$MACHINE_B_HOME"
mkdir -p "$MACHINE_B_HOME"
use_machine "$MACHINE_B_HOME"
git config --global user.name "e2e-tester"
git config --global user.email "e2e@dotdipper.test"

"$BIN" pull --help >/tmp/e2e-pull-help.txt
if ! grep -q -- '--apply' /tmp/e2e-pull-help.txt; then
  cat /tmp/e2e-pull-help.txt >&2
  fail "pull --help does not mention --apply"
fi
if ! grep -q -- '--force' /tmp/e2e-pull-help.txt; then
  cat /tmp/e2e-pull-help.txt >&2
  fail "pull --help does not mention --force"
fi

"$BIN" init
"$BIN" config --set github.username=e2e-user
"$BIN" config --set github.repo_name=e2e-dotfiles
export DOTDIPPER_TEST_REMOTE="$REMOTE"

set +e
PULL_OUT=$("$BIN" pull --apply --force 2>&1)
PULL_EC=$?
set -e
echo "$PULL_OUT"
assert_eq "$PULL_EC" "0" "dotdipper pull --apply --force exit code"

assert_home_matches_expected "$MACHINE_B_HOME"
assert_file_exists "${MACHINE_B_HOME}/.config/nvim/lua/config/options.lua" \
  "nested nvim/lua path was not preserved on machine B"
pass "4. Machine B pull --apply --force (byte-identical fixtures, nested path)"

# ---------------------------------------------------------------------------
# 5. status on machine B is clean
# ---------------------------------------------------------------------------
begin_test "Machine B: status is clean"
use_machine "$MACHINE_B_HOME"
export DOTDIPPER_TEST_REMOTE="$REMOTE"

set +e
STATUS_OUT=$("$BIN" status 2>&1)
STATUS_EC=$?
set -e
echo "$STATUS_OUT"
assert_eq "$STATUS_EC" "0" "dotdipper status exit code"
if [[ "$STATUS_OUT" != *"No changes detected"* ]]; then
  fail "status did not report a clean tree (expected 'No changes detected')"
fi
if echo "$STATUS_OUT" | grep -Eq 'Changes detected'; then
  fail "status reported changes on a freshly applied machine B"
fi
pass "5. Machine B status is clean"

# ---------------------------------------------------------------------------
# 6. Round-trip update: A modifies, pushes; B pulls
# ---------------------------------------------------------------------------
begin_test "Round-trip: modify on A, push, pull on B"

use_machine "$MACHINE_A_HOME"
export DOTDIPPER_TEST_REMOTE="$REMOTE"

cat > "${MACHINE_A_HOME}/.zshrc" <<'EOF'
# E2E_ZSHRC_MARKER=beta-updated-41d2
export EDITOR=nvim
alias ll='ls -la'
export E2E_ROUNDTRIP=1
EOF
cp -a "${MACHINE_A_HOME}/.zshrc" "${EXPECTED}/.zshrc"

"$BIN" push -m "e2e-roundtrip"
assert_eq "$?" "0" "machine A second push exit code"

use_machine "$MACHINE_B_HOME"
export DOTDIPPER_TEST_REMOTE="$REMOTE"
"$BIN" pull --apply --force
assert_eq "$?" "0" "machine B second pull exit code"

assert_file_matches \
  "${MACHINE_B_HOME}/.zshrc" \
  "${EXPECTED}/.zshrc" \
  "machine B .zshrc after round-trip pull"
if ! grep -Fq "E2E_ZSHRC_MARKER=beta-updated-41d2" "${MACHINE_B_HOME}/.zshrc"; then
  fail "machine B .zshrc missing updated marker after pull"
fi
pass "6. Round-trip update (B .zshrc byte-matches A's new content)"

# ---------------------------------------------------------------------------
# 7. Linux install script generation
# ---------------------------------------------------------------------------
begin_test "Linux install --dry-run scripts"
use_machine "$MACHINE_B_HOME"
export DOTDIPPER_TEST_REMOTE="$REMOTE"

"$BIN" install --dry-run
assert_eq "$?" "0" "dotdipper install --dry-run exit code"

INSTALL_DIR="${DOTDIPPER_HOME}/install"
assert_file_exists "${INSTALL_DIR}/install.sh"
assert_file_exists "${INSTALL_DIR}/setup_dotfiles.sh"

mapfile -t PKG_SCRIPTS < <(find "$INSTALL_DIR" -maxdepth 1 -type f -name 'install_*.sh' ! -name 'install.sh' | sort)
if [[ ${#PKG_SCRIPTS[@]} -eq 0 ]]; then
  ls -la "$INSTALL_DIR" >&2
  fail "no install_<os>.sh generated under ${INSTALL_DIR}"
fi

bash -n "${INSTALL_DIR}/install.sh" || fail "bash -n failed on install.sh"
bash -n "${INSTALL_DIR}/setup_dotfiles.sh" || fail "bash -n failed on setup_dotfiles.sh"
for pkg in "${PKG_SCRIPTS[@]}"; do
  bash -n "$pkg" || fail "bash -n failed on ${pkg}"
done

# The OS package script must not contain macOS Homebrew / MAS restore logic.
for pkg in "${PKG_SCRIPTS[@]}"; do
  if grep -Eiq 'Brewfile|brew bundle|[[:space:]]mas[[:space:]]|xcode-select' "$pkg"; then
    echo "----- ${pkg} -----" >&2
    cat "$pkg" >&2
    fail "Linux package script ${pkg} mentions Brewfile/brew bundle/mas/xcode-select"
  fi
done

pass "7. Linux install --dry-run (syntax ok, no Brewfile/mas/Homebrew logic)"

echo
echo "========================================"
echo -e "${GREEN}All ${TESTS_RUN} tests passed.${NC}"
echo "========================================"

#!/usr/bin/env bash
#
# Host-side runner for the Linux container e2e suite.
#
# Usage (from repo root):
#   ./scripts/container-test.sh
#
# Requires Docker. Uses linux/arm64 rust:1-bookworm, a unique container name,
# and a trap that `docker rm -f`s the container on any exit.
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

IMAGE="${DOTDIPPER_E2E_IMAGE:-rust:1-bookworm}"
PLATFORM="${DOTDIPPER_E2E_PLATFORM:-linux/arm64}"
CONTAINER="dotdipper-e2e-$$"
VOLUME_BUILD="${DOTDIPPER_E2E_BUILD_VOLUME:-dotdipper-e2e-build}"
VOLUME_CARGO="${DOTDIPPER_E2E_CARGO_VOLUME:-dotdipper-e2e-cargo}"
DOCKER_WAIT_SECS="${DOTDIPPER_E2E_DOCKER_WAIT:-90}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info() { echo -e "${GREEN}[INFO]${NC} $*"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }

cleanup() {
  # Always tear down the named container so a failed/interrupted run cannot leak it.
  docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

wait_for_docker() {
  local deadline=$((SECONDS + DOCKER_WAIT_SECS))
  local waited=0
  log_info "Waiting for Docker daemon (up to ${DOCKER_WAIT_SECS}s)..."
  while (( SECONDS < deadline )); do
    if docker info >/dev/null 2>&1; then
      if (( waited > 0 )); then
        log_info "Docker daemon is ready (waited ${waited}s)."
      else
        log_info "Docker daemon is ready."
      fi
      return 0
    fi
    sleep 2
    waited=$((waited + 2))
  done
  log_error "Docker daemon did not become available within ${DOCKER_WAIT_SECS}s."
  log_error "Start Docker Desktop (or the docker daemon) and re-run ./scripts/container-test.sh"
  return 1
}

wait_for_docker

log_info "Ensuring image ${IMAGE} (${PLATFORM}) is present..."
docker pull --platform "$PLATFORM" "$IMAGE"

docker volume create "$VOLUME_BUILD" >/dev/null
docker volume create "$VOLUME_CARGO" >/dev/null
log_info "Using cache volumes: ${VOLUME_BUILD} (/build), ${VOLUME_CARGO} (cargo registry)"

log_info "Starting container ${CONTAINER}..."
set +e
docker run \
  --name "$CONTAINER" \
  --platform "$PLATFORM" \
  --init \
  -v "$ROOT":/src:ro \
  -v "$VOLUME_BUILD":/build \
  -v "$VOLUME_CARGO":/usr/local/cargo/registry \
  -e CARGO_TARGET_DIR=/build \
  -e CARGO_TERM_COLOR=always \
  -w /src \
  "$IMAGE" \
  bash /src/scripts/container-e2e.sh
ec=$?
set -e

echo
echo "========================================"
if [[ $ec -eq 0 ]]; then
  echo -e "${GREEN}PASS${NC}: container e2e suite (container ${CONTAINER} will be removed)"
else
  echo -e "${RED}FAIL${NC}: container e2e suite exited ${ec} (container ${CONTAINER} will be removed)"
fi
echo "========================================"
exit "$ec"

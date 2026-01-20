#!/usr/bin/env bash
#
# Local release script that mirrors the auto-release workflow logic
# Analyzes conventional commits to determine version bump
#
# Usage:
#   ./scripts/release.sh              # Dry run - show what would happen
#   ./scripts/release.sh --execute    # Actually perform the release
#   ./scripts/release.sh --version 1.0.0  # Override version
#
# Examples:
#   ./scripts/release.sh              # Preview version bump
#   ./scripts/release.sh --execute    # Create tag and bump version
#   ./scripts/release.sh --version 2.0.0 --execute  # Force specific version

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1" >&2; }
log_step() { echo -e "${BLUE}==>${NC} $1"; }
log_detail() { echo -e "    ${CYAN}→${NC} $1"; }

# Parse arguments
DRY_RUN=true
VERSION_OVERRIDE=""

while [[ $# -gt 0 ]]; do
  case $1 in
    --execute|-e)
      DRY_RUN=false
      shift
      ;;
    --version|-v)
      VERSION_OVERRIDE="$2"
      shift 2
      ;;
    --help|-h)
      echo "Usage: $0 [OPTIONS]"
      echo ""
      echo "Options:"
      echo "  --execute, -e       Actually perform the release (default: dry run)"
      echo "  --version, -v VER   Override version (e.g., 1.0.0)"
      echo "  --help, -h          Show this help message"
      echo ""
      echo "Conventional Commit Rules:"
      echo "  feat:               Minor version bump (0.1.0 -> 0.2.0)"
      echo "  fix:                Patch version bump (0.1.0 -> 0.1.1)"
      echo "  perf:               Patch version bump (0.1.0 -> 0.1.1)"
      echo "  BREAKING CHANGE:    Major version bump (0.1.0 -> 1.0.0)"
      echo "  docs/chore/ci:      No release"
      exit 0
      ;;
    *)
      log_error "Unknown option: $1"
      exit 1
      ;;
  esac
done

# Ensure we're in the repo root
cd "$(git rev-parse --show-toplevel)"

log_step "Analyzing repository for release..."

# Get current version from Cargo.toml
CURRENT_VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
log_info "Current version: $CURRENT_VERSION"

# Check for uncommitted changes
if ! git diff --quiet || ! git diff --staged --quiet; then
  log_warn "You have uncommitted changes. Please commit or stash them first."
  if [ "$DRY_RUN" = false ]; then
    exit 1
  fi
fi

# Get the last release tag
LAST_TAG=$(git describe --tags --abbrev=0 2>/dev/null || echo "")

if [ -z "$LAST_TAG" ]; then
  log_info "No previous tags found, analyzing all commits"
  COMMIT_RANGE="HEAD"
else
  log_info "Last tag: $LAST_TAG"
  COMMIT_RANGE="$LAST_TAG..HEAD"
fi

# Check if version override is provided
if [ -n "$VERSION_OVERRIDE" ]; then
  NEW_VERSION="$VERSION_OVERRIDE"
  BUMP_TYPE="override"
  log_info "Version override provided: $NEW_VERSION"
else
  # Get commits since last tag
  if [ -z "$LAST_TAG" ]; then
    COMMITS=$(git log --pretty=format:"%s")
  else
    COMMITS=$(git log "$COMMIT_RANGE" --pretty=format:"%s" 2>/dev/null || echo "")
  fi

  if [ -z "$COMMITS" ]; then
    log_info "No commits to analyze since last release"
    exit 0
  fi

  log_step "Analyzing commits..."
  echo ""

  # Analyze commit messages for conventional commits
  MAJOR_BUMP=false
  MINOR_BUMP=false
  PATCH_BUMP=false
  
  BREAKING_COMMITS=""
  FEATURE_COMMITS=""
  FIX_COMMITS=""
  PERF_COMMITS=""
  OTHER_COMMITS=""

  while IFS= read -r commit; do
    # Skip empty lines
    [ -z "$commit" ] && continue

    # Check for breaking changes (MAJOR bump)
    if echo "$commit" | grep -qiE "^.*!:|BREAKING CHANGE"; then
      MAJOR_BUMP=true
      BREAKING_COMMITS="${BREAKING_COMMITS}\n    💥 $commit"
    # Check for features (MINOR bump)
    elif echo "$commit" | grep -qE "^feat(\([^)]+\))?:"; then
      MINOR_BUMP=true
      FEATURE_COMMITS="${FEATURE_COMMITS}\n    ✨ $commit"
    # Check for fixes (PATCH bump)
    elif echo "$commit" | grep -qE "^fix(\([^)]+\))?:"; then
      PATCH_BUMP=true
      FIX_COMMITS="${FIX_COMMITS}\n    🐛 $commit"
    # Check for performance improvements (PATCH bump)
    elif echo "$commit" | grep -qE "^perf(\([^)]+\))?:"; then
      PATCH_BUMP=true
      PERF_COMMITS="${PERF_COMMITS}\n    ⚡ $commit"
    # Non-release commits
    elif echo "$commit" | grep -qE "^(docs|chore|ci|style|refactor|test)(\([^)]+\))?:"; then
      OTHER_COMMITS="${OTHER_COMMITS}\n    ⏭️  $commit"
    else
      OTHER_COMMITS="${OTHER_COMMITS}\n    ❓ $commit"
    fi
  done <<< "$COMMITS"

  # Print categorized commits
  if [ -n "$BREAKING_COMMITS" ]; then
    echo -e "${RED}Breaking Changes:${NC}$BREAKING_COMMITS"
    echo ""
  fi
  if [ -n "$FEATURE_COMMITS" ]; then
    echo -e "${GREEN}Features:${NC}$FEATURE_COMMITS"
    echo ""
  fi
  if [ -n "$FIX_COMMITS" ]; then
    echo -e "${YELLOW}Bug Fixes:${NC}$FIX_COMMITS"
    echo ""
  fi
  if [ -n "$PERF_COMMITS" ]; then
    echo -e "${CYAN}Performance:${NC}$PERF_COMMITS"
    echo ""
  fi
  if [ -n "$OTHER_COMMITS" ]; then
    echo -e "${BLUE}Other (no release):${NC}$OTHER_COMMITS"
    echo ""
  fi

  # Determine bump type (major > minor > patch)
  if [ "$MAJOR_BUMP" = true ]; then
    BUMP_TYPE="major"
  elif [ "$MINOR_BUMP" = true ]; then
    BUMP_TYPE="minor"
  elif [ "$PATCH_BUMP" = true ]; then
    BUMP_TYPE="patch"
  else
    log_info "No releasable commits found (only docs/chore/ci/style/refactor/test commits)"
    exit 0
  fi

  # Calculate new version
  IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT_VERSION"
  
  case $BUMP_TYPE in
    major)
      NEW_VERSION="$((MAJOR + 1)).0.0"
      ;;
    minor)
      NEW_VERSION="${MAJOR}.$((MINOR + 1)).0"
      ;;
    patch)
      NEW_VERSION="${MAJOR}.${MINOR}.$((PATCH + 1))"
      ;;
  esac
fi

log_step "Release Summary"
log_detail "Current version: $CURRENT_VERSION"
log_detail "Bump type: $BUMP_TYPE"
log_detail "New version: $NEW_VERSION"
log_detail "Tag: v$NEW_VERSION"

# Check if tag already exists
if git rev-parse "v$NEW_VERSION" >/dev/null 2>&1; then
  log_error "Tag v$NEW_VERSION already exists!"
  exit 1
fi

if [ "$DRY_RUN" = true ]; then
  echo ""
  log_warn "This is a dry run. Use --execute to actually perform the release."
  echo ""
  echo "Commands that would be executed:"
  echo "  1. sed -i \"s/^version = \\\".*\\\"/version = \\\"$NEW_VERSION\\\"/\" Cargo.toml"
  echo "  2. cargo update --workspace"
  echo "  3. git add Cargo.toml Cargo.lock"
  echo "  4. git commit -m \"chore(release): bump version to $NEW_VERSION\""
  echo "  5. git tag -a \"v$NEW_VERSION\" -m \"Release v$NEW_VERSION\""
  echo "  6. git push origin main"
  echo "  7. git push origin \"v$NEW_VERSION\""
  exit 0
fi

# Confirm before proceeding
echo ""
read -p "Proceed with release v$NEW_VERSION? [y/N] " -n 1 -r
echo ""
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
  log_info "Release cancelled."
  exit 0
fi

log_step "Performing release..."

# Update Cargo.toml
log_detail "Updating Cargo.toml..."
if [[ "$OSTYPE" == "darwin"* ]]; then
  # macOS sed requires empty string for -i
  sed -i '' "s/^version = \".*\"/version = \"$NEW_VERSION\"/" Cargo.toml
else
  sed -i "s/^version = \".*\"/version = \"$NEW_VERSION\"/" Cargo.toml
fi

# Update Cargo.lock
log_detail "Updating Cargo.lock..."
cargo update --workspace --quiet

# Commit changes
log_detail "Committing version bump..."
git add Cargo.toml Cargo.lock
git commit -m "chore(release): bump version to $NEW_VERSION"

# Create tag
log_detail "Creating tag v$NEW_VERSION..."
git tag -a "v$NEW_VERSION" -m "Release v$NEW_VERSION"

# Push
log_detail "Pushing to remote..."
git push origin main
git push origin "v$NEW_VERSION"

echo ""
log_info "🎉 Release v$NEW_VERSION complete!"
log_info "The release workflow should now be triggered automatically."
log_info "Check: https://github.com/psyysp/dotdipper/actions"

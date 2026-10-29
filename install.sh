#!/bin/sh
#
# Install dotdipper, and optionally set up a home directory from a dotfiles
# repository in the same command:
#
#   curl -fsSL https://raw.githubusercontent.com/psyysp/dotdipper/main/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/psyysp/dotdipper/main/install.sh | sh -s -- owner/repo --apply
#
# Downloads the prebuilt release binary for this platform, verifies its
# SHA-256 checksum, and installs it to ~/.local/bin (no sudo). With a source,
# it then runs `dotdipper init <source>`, which previews what the repository
# would write into $HOME and asks before writing anything.
#
# POSIX sh, because `| sh` is dash on Debian and Ubuntu. The whole script is
# a function called on the last line, so a download cut short runs nothing.

set -eu

REPO="psyysp/dotdipper"

# The newest release built before build provenance was attested. Releases up
# to and including it cannot be verified, so the check is skipped for them;
# every later release must verify whenever gh can check it. A cutoff, rather
# than "warn when no attestation exists", because an attacker who replaces a
# release asset produces exactly that: a file with no attestation.
LAST_UNATTESTED="v0.12.0"

log()  { printf '\033[0;32m==>\033[0m %s\n' "$1"; }
warn() { printf '\033[1;33m==>\033[0m %s\n' "$1" >&2; }
die()  { printf '\033[0;31m==>\033[0m %s\n' "$1" >&2; exit 1; }

usage() {
    cat <<'USAGE'
Usage: install.sh [options] [source] [init options]

  source               Dotfiles repository to set up from after installing:
                       owner, owner/repo, or an https:// or git@ URL

Installer options:
  --version <tag>      Release to install (default: latest), e.g. v0.12.0
  --bin-dir <dir>      Where to install the binary (default: ~/.local/bin)
  --from-source        Build with cargo from the current checkout instead
  --require-attestation
                       Fail unless the build provenance is verified with the
                       GitHub CLI (gh, authenticated). Without this flag the
                       check runs when gh is available and is skipped otherwise.
  -h, --help           Show this message

Options passed to `dotdipper init` (with a source):
  --apply              Apply the dotfiles after the preview
  -i, --interactive    Choose the files to apply from a checklist
  --yes                Do not ask before applying (unattended runs)
  --ref <commit>       Apply this exact, reviewed commit
  --branch <branch>    Follow this branch instead of the default
  --install            Then install the repository's apps (macOS)
  --adopt-config       Also apply the repository's dotdipper settings and
                       hooks; only for a repository you control
USAGE
}

need() {
    command -v "$1" >/dev/null 2>&1 || die "$1 is required but not installed."
}

detect_target() {
    os=$(uname -s)
    arch=$(uname -m)
    case "$os" in
        Darwin) os_part="apple-darwin" ;;
        Linux)  os_part="unknown-linux-gnu" ;;
        *)      die "Unsupported operating system: $os. Install with cargo instead." ;;
    esac
    case "$arch" in
        arm64|aarch64) arch_part="aarch64" ;;
        x86_64|amd64)  arch_part="x86_64" ;;
        *)             die "Unsupported architecture: $arch. Install with cargo instead." ;;
    esac
    printf '%s-%s' "$arch_part" "$os_part"
}

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    else
        die "Neither sha256sum nor shasum is available to verify the download."
    fi
}

# Verify the signed build attestation for a tarball. The .sha256 file comes
# from the same release as the tarball, so it cannot detect a tampered release;
# the attestation can. require=1 turns "cannot check" into a failure.
# Succeeds when tag $1 is at or below tag $2 (vMAJOR.MINOR.PATCH).
version_le() {
    printf '%s\n%s\n' "${1#v}" "${2#v}" | awk -F. '
        NR == 1 { a1 = $1; a2 = $2; a3 = $3 }
        NR == 2 { b1 = $1; b2 = $2; b3 = $3 }
        END {
            if (a1 + 0 != b1 + 0) exit !(a1 + 0 < b1 + 0)
            if (a2 + 0 != b2 + 0) exit !(a2 + 0 < b2 + 0)
            exit !(a3 + 0 <= b3 + 0)
        }'
}

# Resolves "latest" to its tag, so the attestation cutoff applies to it too.
resolve_latest() {
    url=$(curl --proto '=https' --tlsv1.2 -fsSLI -o /dev/null -w '%{url_effective}' \
        "https://github.com/$REPO/releases/latest") \
        || die "Could not determine the latest release."
    tag=${url##*/}
    case "$tag" in
        v[0-9]*) printf '%s' "$tag" ;;
        *) die "Could not determine the latest release (got '$tag')." ;;
    esac
}

verify_provenance() {
    file=$1
    require=$2
    tag=$3
    if version_le "$tag" "$LAST_UNATTESTED"; then
        [ "$require" -eq 0 ] || die "$tag predates attested releases; --require-attestation cannot be satisfied."
        warn "Provenance was not checked: $tag predates attested releases (checksum only)."
        return 0
    fi
    if ! command -v gh >/dev/null 2>&1; then
        [ "$require" -eq 0 ] || die "--require-attestation needs the GitHub CLI (gh), which is not installed."
        warn "Provenance was not checked: gh is not installed (checksum only)."
        return 0
    fi
    if ! gh auth status >/dev/null 2>&1; then
        [ "$require" -eq 0 ] || die "--require-attestation needs gh to be authenticated. Run: gh auth login"
        warn "Provenance was not checked: gh is not authenticated (checksum only)."
        return 0
    fi
    log "Verifying build provenance"
    gh attestation verify "$file" --repo "$REPO" </dev/null >/dev/null 2>&1 \
        || die "Build provenance verification failed for $(basename "$file"). Not installing."
    log "Build provenance verified"
}

install_release() {
    version=$1
    bin_dir=$2
    require_attestation=$3
    need curl
    need tar
    target=$(detect_target)
    asset="dotdipper-$target.tar.gz"
    if [ "$version" = "latest" ]; then
        version=$(resolve_latest)
    fi
    base="https://github.com/$REPO/releases/download/$version"

    tmp=$(mktemp -d 2>/dev/null || mktemp -d -t dotdipper)
    # shellcheck disable=SC2064
    trap "rm -rf '$tmp'" EXIT INT TERM

    log "Downloading $asset ($version)"
    # --proto/--tlsv1.2: never follow a redirect onto plain HTTP or old TLS.
    curl --proto '=https' --tlsv1.2 -fsSL -o "$tmp/$asset" "$base/$asset" \
        || die "Download failed. Is there a release for $target?"
    curl --proto '=https' --tlsv1.2 -fsSL -o "$tmp/$asset.sha256" "$base/$asset.sha256" \
        || die "Could not download the checksum; refusing to install an unverified binary."

    expected=$(awk '{print $1}' "$tmp/$asset.sha256")
    actual=$(sha256_of "$tmp/$asset")
    [ -n "$expected" ] && [ "$expected" = "$actual" ] \
        || die "Checksum mismatch for $asset (expected $expected, got $actual). Not installing."
    log "Checksum verified"
    verify_provenance "$tmp/$asset" "$require_attestation" "$version"

    tar -xzf "$tmp/$asset" -C "$tmp" dotdipper \
        || die "The archive does not contain a dotdipper binary."
    mkdir -p "$bin_dir"
    # Write beside the target, then rename: a running copy is never left
    # half-written, and the replacement is atomic.
    cp "$tmp/dotdipper" "$bin_dir/.dotdipper.new"
    chmod 0755 "$bin_dir/.dotdipper.new"
    mv -f "$bin_dir/.dotdipper.new" "$bin_dir/dotdipper"
}

install_from_source() {
    bin_dir=$1
    [ -f Cargo.toml ] && grep -q '^name = "dotdipper"' Cargo.toml \
        || die "--from-source must be run from a dotdipper checkout."
    need cargo
    log "Building from source"
    cargo build --release --locked
    mkdir -p "$bin_dir"
    cp target/release/dotdipper "$bin_dir/.dotdipper.new"
    chmod 0755 "$bin_dir/.dotdipper.new"
    mv -f "$bin_dir/.dotdipper.new" "$bin_dir/dotdipper"
}

main() {
    set -f  # no globbing: init options are expanded unquoted below
    version="${DOTDIPPER_VERSION:-latest}"
    bin_dir="${DOTDIPPER_BIN_DIR:-$HOME/.local/bin}"
    from_source=0
    require_attestation=0
    source=""
    init_args=""

    while [ $# -gt 0 ]; do
        case "$1" in
            --version)     [ $# -ge 2 ] || die "--version needs a tag"; version=$2; shift 2 ;;
            --bin-dir)     [ $# -ge 2 ] || die "--bin-dir needs a directory"; bin_dir=$2; shift 2 ;;
            --from-source) from_source=1; shift ;;
            --require-attestation) require_attestation=1; shift ;;
            --apply|--yes|-y|--install|--adopt-config|--interactive|-i)
                init_args="$init_args $1"; shift ;;
            --ref|--branch)
                [ $# -ge 2 ] || die "$1 needs a value"
                case "$2" in -*) die "$1 needs a value" ;; esac
                init_args="$init_args $1 $2"; shift 2 ;;
            -h|--help)     usage; exit 0 ;;
            -*)            usage >&2; die "Unknown option: $1" ;;
            *)
                [ -z "$source" ] || die "Only one source may be given."
                source=$1; shift ;;
        esac
    done
    case "$version" in
        latest|v[0-9]*) ;;
        *) die "--version must look like v1.2.3" ;;
    esac

    if [ "$from_source" -eq 1 ] && [ "$require_attestation" -eq 1 ]; then
        die "--require-attestation applies to release downloads, not --from-source."
    fi

    if [ "$from_source" -eq 1 ]; then
        install_from_source "$bin_dir"
    else
        install_release "$version" "$bin_dir" "$require_attestation"
    fi
    bin="$bin_dir/dotdipper"
    log "Installed $("$bin" --version) to $bin"

    case ":$PATH:" in
        *":$bin_dir:"*) ;;
        *)
            warn "$bin_dir is not on your PATH. Add this to your shell configuration:"
            warn "  export PATH=\"$bin_dir:\$PATH\""
            ;;
    esac
    command -v age >/dev/null 2>&1 \
        || warn "age is not installed; it is only needed for encrypted secrets."

    if [ -z "$source" ]; then
        log "Done. Set up from a repository with: dotdipper init <owner/repo> --apply"
        return 0
    fi

    command -v git >/dev/null 2>&1 \
        || die "git is required to clone $source. On macOS, run: xcode-select --install"

    # Under `curl | sh`, stdin is the script itself. Read answers from the
    # terminal instead, so init can show its preview and ask before applying.
    log "Setting up from $source"
    # shellcheck disable=SC2086
    if (exec </dev/tty) 2>/dev/null; then
        "$bin" init "$source" $init_args </dev/tty
    else
        "$bin" init "$source" $init_args
    fi
}

main "$@"

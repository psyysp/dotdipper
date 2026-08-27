pub mod analyzers;
pub mod discover;
pub mod package_map;
pub mod validators;

use anyhow::{Context, Result};
use os_info::Type as OsType;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cfg::{Config, PackagesConfig, RestoreMode};
use crate::ui;

// Re-export commonly used types
pub use discover::{DiscoveryConfig, DiscoveryResult};
pub use package_map::PackageMapper;
pub use validators::ValidationResult;

#[derive(Debug, Clone)]
pub struct InstallScript {
    pub name: String,
    pub content: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DotfileInstallAction {
    rel_path: PathBuf,
    mode: RestoreMode,
}

pub fn detect_os() -> String {
    let info = os_info::get();
    match info.os_type() {
        OsType::Macos => "macos".to_string(),
        OsType::Ubuntu | OsType::Debian => "ubuntu".to_string(),
        OsType::Arch | OsType::Manjaro | OsType::EndeavourOS => "arch".to_string(),
        OsType::Fedora | OsType::Redhat | OsType::CentOS => "fedora".to_string(),
        _ => "linux".to_string(),
    }
}

pub fn generate_scripts(config: &Config, target_os: &str) -> Result<Vec<InstallScript>> {
    let mut scripts = Vec::new();

    // Generate main install script
    let main_script = generate_main_script(config, target_os)?;
    scripts.push(main_script);

    // Generate OS-specific package install script
    let package_script = generate_package_script(&config.packages, target_os)?;
    scripts.push(package_script);

    // Generate dotfiles setup script
    let dotfiles_script = generate_dotfiles_script(config)?;
    scripts.push(dotfiles_script);

    // Save scripts to disk
    let script_dir = crate::paths::install_dir()?;

    fs::create_dir_all(&script_dir)?;

    for script in &mut scripts {
        script.path = script_dir.join(&script.name);
        fs::write(&script.path, &script.content)?;

        // Make script executable on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&script.path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script.path, perms)?;
        }
    }

    Ok(scripts)
}

fn generate_main_script(_config: &Config, target_os: &str) -> Result<InstallScript> {
    let content = format!(
        r#"#!/usr/bin/env bash
#
# Dotdipper Installation Script
# Generated: {timestamp}
# Target OS: {target_os}
#

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Helper functions
log_info() {{
    echo -e "${{GREEN}}[INFO]${{NC}} $1"
}}

log_error() {{
    echo -e "${{RED}}[ERROR]${{NC}} $1" >&2
}}

log_warn() {{
    echo -e "${{YELLOW}}[WARN]${{NC}} $1"
}}

# Check if running as root
if [[ $EUID -eq 0 ]]; then
   log_error "This script should not be run as root"
   exit 1
fi

log_info "Starting Dotdipper installation for {target_os}"

# Set up directories
DOTDIPPER_DIR="${{DOTDIPPER_HOME:-${{XDG_CONFIG_HOME:-$HOME/.config}}/dotdipper}}"
COMPILED_DIR="$DOTDIPPER_DIR/compiled"
INSTALL_DIR="$DOTDIPPER_DIR/install"

mkdir -p "$DOTDIPPER_DIR"
mkdir -p "$COMPILED_DIR"
mkdir -p "$INSTALL_DIR"

# Check for required tools
command -v git >/dev/null 2>&1 || {{
    log_error "Git is not installed. Please install git first."
    exit 1
}}

# Run OS-specific package installation
log_info "Installing packages..."
if [[ -f "$INSTALL_DIR/install_{target_os}.sh" ]]; then
    bash "$INSTALL_DIR/install_{target_os}.sh"
else
    log_warn "Package installation script not found"
fi

# Set up dotfiles
# Prefer the Rust apply path when available so excludes/backups are honored.
log_info "Setting up dotfiles..."
if command -v dotdipper >/dev/null 2>&1; then
    log_info "Using 'dotdipper apply' for safe dotfile placement"
    dotdipper apply || log_warn "dotdipper apply reported issues"
elif [[ -f "$INSTALL_DIR/setup_dotfiles.sh" ]]; then
    log_warn "dotdipper binary not on PATH; falling back to setup_dotfiles.sh"
    bash "$INSTALL_DIR/setup_dotfiles.sh"
else
    log_warn "Dotfiles setup script not found"
fi

log_info "Installation complete!"
log_info "Run 'dotdipper status' to check your dotfiles"
"#,
        timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"),
        target_os = target_os
    );

    Ok(InstallScript {
        name: "install.sh".to_string(),
        content,
        path: PathBuf::new(),
    })
}

fn generate_package_script(packages: &PackagesConfig, target_os: &str) -> Result<InstallScript> {
    // macOS app restore (Brewfile / mas / unmanaged manifest) is gated here so
    // Linux package scripts stay on the legacy apt/pacman/dnf paths.
    if target_os == "macos" {
        return generate_macos_package_script(packages, target_os);
    }

    let (package_manager, install_cmd, update_cmd) = match target_os {
        "macos" => ("brew", "brew install", "brew update"),
        "ubuntu" | "debian" => ("apt", "sudo apt install -y", "sudo apt update"),
        "arch" | "manjaro" => ("pacman", "sudo pacman -S --noconfirm", "sudo pacman -Sy"),
        "fedora" | "redhat" => ("dnf", "sudo dnf install -y", "sudo dnf check-update"),
        _ => ("apt", "sudo apt install -y", "sudo apt update"),
    };

    let mut all_packages = packages.common.clone();

    match target_os {
        "macos" => all_packages.extend(packages.macos.clone()),
        "ubuntu" | "debian" => {
            all_packages.extend(packages.linux.clone());
            all_packages.extend(packages.ubuntu.clone());
        }
        "arch" | "manjaro" => {
            all_packages.extend(packages.linux.clone());
            all_packages.extend(packages.arch.clone());
        }
        _ => all_packages.extend(packages.linux.clone()),
    }

    // Remove duplicates
    all_packages.sort();
    all_packages.dedup();

    let content = format!(
        r#"#!/usr/bin/env bash
#
# Package Installation Script for {}
# Package Manager: {}
#

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info() {{
    echo -e "${{GREEN}}[INFO]${{NC}} $1"
}}

log_error() {{
    echo -e "${{RED}}[ERROR]${{NC}} $1" >&2
}}

# Check if package manager exists
if ! command -v {} &> /dev/null; then
    log_error "Package manager '{}' not found"
    exit 1
fi

# Update package lists
log_info "Updating package lists..."
{} || true

# Packages to install
packages=(
{}
)

# Install packages
for package in "${{packages[@]}}"; do
    if {} "$package"; then
        log_info "Installed $package"
    else
        log_error "Failed to install $package"
    fi
done

log_info "Package installation complete"
"#,
        target_os,
        package_manager,
        package_manager
            .split_whitespace()
            .next()
            .unwrap_or(package_manager),
        package_manager,
        update_cmd,
        all_packages
            .iter()
            .map(|p| format!("    \"{}\"", p))
            .collect::<Vec<_>>()
            .join("\n"),
        install_cmd
    );

    Ok(InstallScript {
        name: format!("install_{}.sh", target_os),
        content,
        path: PathBuf::new(),
    })
}

fn generate_macos_package_script(
    packages: &PackagesConfig,
    target_os: &str,
) -> Result<InstallScript> {
    let mut all_packages = packages.common.clone();
    all_packages.extend(packages.macos.clone());
    all_packages.sort();
    all_packages.dedup();

    let package_lines = all_packages
        .iter()
        .map(|p| format!("    \"{}\"", p))
        .collect::<Vec<_>>()
        .join("\n");

    let content = format!(
        r#"#!/usr/bin/env bash
#
# Package Installation Script for {target_os}
# Package Manager: brew
#

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info() {{
    echo -e "${{GREEN}}[INFO]${{NC}} $1"
}}

log_error() {{
    echo -e "${{RED}}[ERROR]${{NC}} $1" >&2
}}

log_warn() {{
    echo -e "${{YELLOW}}[WARN]${{NC}} $1"
}}

COMPILED_DIR="${{DOTDIPPER_HOME:-${{XDG_CONFIG_HOME:-$HOME/.config}}/dotdipper}}/compiled"

# Xcode Command Line Tools
if ! xcode-select -p >/dev/null 2>&1; then
    log_info "Xcode Command Line Tools not found. Starting installer..."
    xcode-select --install
    log_info "Please complete the Xcode Command Line Tools installation, then re-run this script."
    exit 0
fi

# Homebrew
if ! command -v brew >/dev/null 2>&1; then
    log_info "Homebrew not found. Installing Homebrew..."
    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
fi

# Eval shellenv for Apple Silicon and Intel Homebrew prefixes
if [[ -x /opt/homebrew/bin/brew ]]; then
    eval "$(/opt/homebrew/bin/brew shellenv)"
fi
if [[ -x /usr/local/bin/brew ]]; then
    eval "$(/usr/local/bin/brew shellenv)"
fi

if ! command -v brew >/dev/null 2>&1; then
    log_error "Homebrew is not available. Please install it manually and re-run this script."
    exit 1
fi

# Prefer Brewfile from the compiled repo; fall back to config package lists
if [[ -f "$COMPILED_DIR/Brewfile" ]]; then
    log_info "Found Brewfile at $COMPILED_DIR/Brewfile"

    if grep -q '^mas ' "$COMPILED_DIR/Brewfile"; then
        if ! command -v mas >/dev/null 2>&1; then
            log_info "Installing mas (Mac App Store CLI)..."
            brew install mas || log_warn "Failed to install mas"
        fi
        log_warn "Brewfile contains Mac App Store apps. Sign in to the App Store before continuing."
    fi

    log_info "Installing packages from Brewfile..."
    brew bundle --file="$COMPILED_DIR/Brewfile" || log_warn "brew bundle reported errors; continuing with remaining setup"
else
    log_info "No Brewfile found; falling back to configured package lists"
    log_info "Updating package lists..."
    brew update || true

    packages=(
{package_lines}
    )

    for package in "${{packages[@]}}"; do
        if brew install "$package"; then
            log_info "Installed $package"
        else
            log_error "Failed to install $package"
        fi
    done
fi

# Apps that cannot be installed via brew/mas
if [[ -f "$COMPILED_DIR/apps_manifest.toml" ]]; then
    log_info "Checking for apps that must be installed manually..."
    unmanaged_list="$(awk '
        function flush() {{
            if (name != "") {{
                printf "  - %s\n    %s\n", name, path
                if (homepage != "") {{
                    printf "    %s\n", homepage
                }}
            }}
            name = ""
            path = ""
            homepage = ""
        }}
        /^\[\[unmanaged\]\]/ {{ flush(); in_u = 1; next }}
        /^\[/ {{ if (in_u) flush(); in_u = 0; next }}
        in_u && /^name[[:space:]]*=/ {{
            sub(/^[^=]*=[[:space:]]*"/, "")
            sub(/"[[:space:]]*$/, "")
            name = $0
            next
        }}
        in_u && /^path[[:space:]]*=/ {{
            sub(/^[^=]*=[[:space:]]*"/, "")
            sub(/"[[:space:]]*$/, "")
            path = $0
            next
        }}
        in_u && /^homepage[[:space:]]*=/ {{
            sub(/^[^=]*=[[:space:]]*"/, "")
            sub(/"[[:space:]]*$/, "")
            homepage = $0
            next
        }}
        END {{
            if (in_u) flush()
        }}
    ' "$COMPILED_DIR/apps_manifest.toml")"

    if [[ -n "$unmanaged_list" ]]; then
        echo
        echo "============================================================"
        echo "  Apps you must install manually"
        echo "============================================================"
        echo "$unmanaged_list"
        echo "============================================================"
        echo
    else
        log_info "All apps are covered by Homebrew/mas; nothing to install manually."
    fi
fi

log_info "Package installation complete"
"#,
        target_os = target_os,
        package_lines = package_lines
    );

    Ok(InstallScript {
        name: format!("install_{}.sh", target_os),
        content,
        path: PathBuf::new(),
    })
}

/// Build the portable `setup_dotfiles.sh` content from the manifest or tracked list.
pub fn generate_dotfiles_script(config: &Config) -> Result<InstallScript> {
    let actions = build_dotfiles_install_actions(config)?;
    let helpers = generate_dotfile_helpers();
    let body = if actions.is_empty() {
        generate_find_fallback_body(default_restore_mode(config))
    } else {
        generate_explicit_dotfile_body(&actions)
    };

    let content = format!(
        r#"#!/usr/bin/env bash
#
# Dotfiles Setup Script
# Entries: {entries}
# NOTE: Prefer `dotdipper apply` — this script is a fallback for bootstrap without the binary.
# Encrypted secrets (.age / .sops.*) are skipped here; use `dotdipper apply` to decrypt them.
#

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info() {{
    echo -e "${{GREEN}}[INFO]${{NC}} $1"
}}

log_error() {{
    echo -e "${{RED}}[ERROR]${{NC}} $1" >&2
}}

log_warn() {{
    echo -e "${{YELLOW}}[WARN]${{NC}} $1"
}}

COMPILED_DIR="${{DOTDIPPER_HOME:-${{XDG_CONFIG_HOME:-$HOME/.config}}/dotdipper}}/compiled"
HOME_DIR="$HOME"

# Check if compiled directory exists
if [[ ! -d "$COMPILED_DIR" ]]; then
    log_error "Compiled directory not found at $COMPILED_DIR"
    log_info "Run 'dotdipper pull' to download your dotfiles first"
    exit 1
fi

{helpers}

{body}

log_info "Dotfiles setup complete"
"#,
        entries = describe_install_actions(&actions),
        helpers = helpers,
        body = body,
    );

    Ok(InstallScript {
        name: "setup_dotfiles.sh".to_string(),
        content,
        path: PathBuf::new(),
    })
}

fn default_restore_mode(config: &Config) -> RestoreMode {
    if config
        .dotfiles
        .as_ref()
        .map(|d| d.use_symlinks)
        .unwrap_or(false)
    {
        RestoreMode::Symlink
    } else {
        config.general.default_mode
    }
}

fn override_key(rel: &Path) -> String {
    format!("~/{}", rel.display())
}

fn should_skip_rel(config: &Config, rel: &Path) -> bool {
    if crate::repo::is_store_metadata(rel) {
        return true;
    }
    if crate::secrets::is_encrypted_secret_path(rel) {
        return true;
    }
    config
        .files
        .get(&override_key(rel))
        .is_some_and(|entry| entry.exclude || entry.local_only)
}

fn restore_mode_for(config: &Config, rel: &Path) -> RestoreMode {
    config
        .files
        .get(&override_key(rel))
        .and_then(|entry| entry.mode)
        .unwrap_or_else(|| default_restore_mode(config))
}

fn tracked_files_as_rel(config: &Config) -> Result<Vec<PathBuf>> {
    let home = dirs::home_dir().context("Failed to find home directory")?;
    let mut rels = Vec::new();
    for tracked in &config.general.tracked_files {
        let Ok(rel) = tracked.strip_prefix(&home) else {
            continue;
        };
        if rel.as_os_str().is_empty() {
            continue;
        }
        rels.push(rel.to_path_buf());
    }
    Ok(rels)
}

fn build_dotfiles_install_actions(config: &Config) -> Result<Vec<DotfileInstallAction>> {
    let mut rels: Vec<PathBuf> = Vec::new();
    let mut have_inventory = false;

    if let Ok(manifest) = crate::repo::load_manifest() {
        have_inventory = true;
        rels.extend(manifest.files.keys().cloned());
    }

    if !have_inventory {
        rels = tracked_files_as_rel(config)?;
    }

    let mut actions = Vec::new();
    for rel in rels {
        if should_skip_rel(config, &rel) {
            continue;
        }
        let mode = restore_mode_for(config, &rel);
        actions.push(DotfileInstallAction {
            rel_path: rel,
            mode,
        });
    }

    actions.sort_by(|left, right| left.rel_path.cmp(&right.rel_path));
    actions.dedup_by(|left, right| left.rel_path == right.rel_path);
    Ok(actions)
}

fn describe_install_actions(actions: &[DotfileInstallAction]) -> String {
    let symlink_count = actions
        .iter()
        .filter(|action| action.mode == RestoreMode::Symlink)
        .count();
    let copy_count = actions.len().saturating_sub(symlink_count);

    match (symlink_count, copy_count) {
        (0, 0) => "none (runtime find fallback if compiled/ is populated)".to_string(),
        (symlinks, 0) => format!("{symlinks} symlink entries"),
        (0, copies) => format!("{copies} copy entries"),
        (symlinks, copies) => format!("{symlinks} symlink entries, {copies} copy entries"),
    }
}

fn generate_dotfile_helpers() -> String {
    r#"# Function to create backup
backup_file() {
    local file="$1"
    if [[ -e "$file" ]] && [[ ! -L "$file" ]]; then
        local backup="${file}.backup.$(date +%Y%m%d_%H%M%S)"
        mv "$file" "$backup"
        log_info "Backed up $file to $backup"
    fi
}

# Function to ensure parent directory exists
ensure_parent_dir() {
    local file="$1"
    local parent=$(dirname "$file")
    if [[ ! -d "$parent" ]]; then
        mkdir -p "$parent"
        log_info "Created directory $parent"
    fi
}

remove_target() {
    local path="$1"
    if [[ -d "$path" ]] && [[ ! -L "$path" ]]; then
        rm -rf "$path"
    else
        rm -f "$path"
    fi
}

apply_symlink() {
    local rel_path="$1"
    local source_file="$COMPILED_DIR/$rel_path"
    local target_file="$HOME_DIR/$rel_path"

    if [[ ! -e "$source_file" ]]; then
        log_warn "Skipping $rel_path (source not found in compiled dir)"
        return 0
    fi

    if [[ -L "$target_file" ]] && [[ "$(readlink "$target_file")" == "$source_file" ]]; then
        log_info "Already linked $rel_path"
        return 0
    fi

    ensure_parent_dir "$target_file"

    if [[ -e "$target_file" ]] || [[ -L "$target_file" ]]; then
        backup_file "$target_file"
        remove_target "$target_file"
    fi

    ln -s "$source_file" "$target_file"
    log_info "Linked $rel_path"
}

apply_copy() {
    local rel_path="$1"
    local source_file="$COMPILED_DIR/$rel_path"
    local target_file="$HOME_DIR/$rel_path"

    if [[ ! -e "$source_file" ]]; then
        log_warn "Skipping $rel_path (source not found in compiled dir)"
        return 0
    fi

    if [[ -f "$source_file" ]] && [[ -f "$target_file" ]] && cmp -s "$source_file" "$target_file"; then
        log_info "Already copied $rel_path"
        return 0
    fi

    ensure_parent_dir "$target_file"

    if [[ -e "$target_file" ]] || [[ -L "$target_file" ]]; then
        backup_file "$target_file"
        remove_target "$target_file"
    fi

    if [[ -d "$source_file" ]]; then
        cp -Rp "$source_file" "$target_file"
    else
        cp -p "$source_file" "$target_file"
    fi

    log_info "Copied $rel_path"
}"#
    .to_string()
}

fn generate_explicit_dotfile_body(actions: &[DotfileInstallAction]) -> String {
    let steps = actions
        .iter()
        .map(|action| {
            let rel_path = shell_quote(&action.rel_path.to_string_lossy());
            match action.mode {
                RestoreMode::Symlink => format!("apply_symlink {}", rel_path),
                RestoreMode::Copy => format!("apply_copy {}", rel_path),
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"DOTFILE_COUNT={}

if [[ "$DOTFILE_COUNT" -eq 0 ]]; then
    log_warn "No portable dotfiles are configured for installation"
    exit 0
fi

log_info "Installing $DOTFILE_COUNT tracked dotfiles"

{}"#,
        actions.len(),
        steps
    )
}

fn generate_find_fallback_body(mode: RestoreMode) -> String {
    let apply_fn = match mode {
        RestoreMode::Symlink => "apply_symlink",
        RestoreMode::Copy => "apply_copy",
    };
    format!(
        r#"# No manifest or tracked files at generation time — walk compiled/ but skip metadata.
# Portable across BSD (macOS) and GNU find; skip store metadata and app manifests.
DOTFILES=()
while IFS= read -r f; do
    rel="${{f#"$COMPILED_DIR"/}}"
    case "$rel" in
        .git/*|.git|manifest.lock|.gitignore|Brewfile|apps_manifest.toml|.dotdipper/*) continue ;;
    esac
    case "$rel" in
        *.age|*.sops|*.sops.*|*.enc.yaml|*.enc.yml|*.enc.json|*.enc.env) continue ;;
    esac
    DOTFILES+=("$rel")
done < <(find "$COMPILED_DIR" -type f ! -path '*/.git/*' | sort)

if [[ ${{#DOTFILES[@]}} -eq 0 ]]; then
    log_warn "No portable dotfiles found in $COMPILED_DIR"
    exit 0
fi

log_info "Installing ${{#DOTFILES[@]}} compiled dotfiles"
for rel_path in "${{DOTFILES[@]}}"; do
    {apply_fn} "$rel_path"
done"#
    )
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r#"'"'"'"#))
}

pub fn run_scripts(scripts: &[InstallScript]) -> Result<()> {
    for script in scripts {
        ui::info(&format!("Running {}...", script.name));

        let output = Command::new("bash")
            .arg(&script.path)
            .output()
            .with_context(|| format!("Failed to run script: {}", script.name))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Script {} failed: {}", script.name, stderr);
        }

        ui::success(&format!("{} completed", script.name));
    }

    Ok(())
}

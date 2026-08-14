pub mod analyzers;
pub mod discover;
pub mod package_map;
pub mod validators;

use anyhow::{Context, Result};
use os_info::Type as OsType;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use walkdir::WalkDir;

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

const PACKAGE_SCRIPT_OSES: &[&str] = &["macos", "ubuntu", "arch", "fedora", "linux"];

#[derive(Default)]
pub struct ScriptRunOpts {
    pub skip_packages: bool,
    pub force: bool,
    pub target_os: Option<String>,
}

pub fn generate_scripts(config: &Config, target_os: &str) -> Result<Vec<InstallScript>> {
    generate_scripts_with_export(config, target_os, true)
}

/// `export_to_compiled` copies scripts into `compiled/install/` (snapshot/push).
/// `dotdipper install` passes false so a fresh machine cannot overwrite pulled scripts.
pub fn generate_scripts_with_export(
    config: &Config,
    target_os: &str,
    export_to_compiled: bool,
) -> Result<Vec<InstallScript>> {
    let config = config_for_scripts(config);
    let mut scripts = Vec::new();

    scripts.push(generate_main_script(&config, target_os)?);
    scripts.push(generate_dotfiles_script(&config)?);

    for os in PACKAGE_SCRIPT_OSES {
        scripts.push(generate_package_script(&config, os)?);
    }

    let script_dir = crate::paths::install_dir()?;
    fs::create_dir_all(&script_dir)?;

    let compiled_export = if export_to_compiled {
        crate::paths::compiled_dir()
            .ok()
            .filter(|p| p.exists())
            .map(|p| p.join("install"))
    } else {
        None
    };
    if let Some(dir) = &compiled_export {
        fs::create_dir_all(dir)?;
    }

    for script in &mut scripts {
        script.path = script_dir.join(&script.name);
        write_executable(&script.path, &script.content)?;
        if let Some(dir) = &compiled_export {
            write_executable(&dir.join(&script.name), &script.content)?;
        }
    }

    Ok(scripts)
}

fn is_fresh_init(config: &Config) -> bool {
    config.general.tracked_files.is_empty()
        && config.files.is_empty()
        && config.packages == PackagesConfig::default()
}

fn load_bootstrap_config() -> Result<Option<Config>> {
    let compiled = match crate::paths::compiled_dir() {
        Ok(p) => p,
        Err(_) => return Ok(None),
    };
    let path = compiled.join(".dotdipper").join("bootstrap.toml");
    if !path.exists() {
        return Ok(None);
    }
    let contents =
        fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
    let boot: Config =
        toml::from_str(&contents).with_context(|| format!("Failed to parse {}", path.display()))?;
    Ok(Some(boot))
}

/// On a freshly inited machine, use the bootstrap.toml that traveled with the compiled repo.
fn config_for_scripts(local: &Config) -> Config {
    match load_bootstrap_config() {
        Ok(Some(boot)) if is_fresh_init(local) => {
            let mut merged = local.clone();
            merged.general.default_mode = boot.general.default_mode;
            merged.general.backup = boot.general.backup;
            merged.packages = boot.packages;
            merged.files = boot.files;
            merged.secrets = boot.secrets;
            merged
        }
        Ok(_) => local.clone(),
        Err(e) => {
            ui::warn(&format!("Could not load bootstrap.toml: {:#}", e));
            local.clone()
        }
    }
}

fn write_executable(path: &Path, content: &str) -> Result<()> {
    fs::write(path, content).with_context(|| format!("Failed to write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms)?;
    }
    Ok(())
}

/// After `git pull` of the compiled repo, copy bundled manifest + install scripts
/// into the local dotdipper directories.
pub fn restore_artifacts_from_compiled() -> Result<()> {
    let compiled = crate::paths::compiled_dir()?;
    if !compiled.exists() {
        return Ok(());
    }

    let bundled = compiled.join(".dotdipper").join("manifest.lock");
    if bundled.exists() {
        let dest = crate::paths::manifest_file()?;
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&bundled, &dest)
            .with_context(|| format!("Failed to restore manifest from {}", bundled.display()))?;
    }

    let src_install = compiled.join("install");
    if src_install.is_dir() {
        let dest_install = crate::paths::install_dir()?;
        fs::create_dir_all(&dest_install)?;
        for entry in fs::read_dir(&src_install)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                let dest = dest_install.join(entry.file_name());
                fs::copy(&path, &dest)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mut perms = fs::metadata(&dest)?.permissions();
                    perms.set_mode(0o755);
                    fs::set_permissions(&dest, perms)?;
                }
            }
        }
    }

    Ok(())
}

/// After pull, reconstruct local config from the bundled bootstrap + manifest
/// so machine B is a full peer (status/snapshot/push/install all work).
pub fn hydrate_from_compiled(config_path: &Path) -> Result<bool> {
    restore_artifacts_from_compiled()?;
    let mut config = crate::cfg::load(config_path)?;
    let mut changed = false;
    let had_no_tracked = config.general.tracked_files.is_empty();

    if let Some(boot) = load_bootstrap_config()? {
        if config.general.tracked_files.is_empty() && !boot.general.tracked_files.is_empty() {
            config.general.tracked_files = boot.general.tracked_files.clone();
            changed = true;
        }
        if config.files.is_empty() && !boot.files.is_empty() {
            config.files = boot.files.clone();
            changed = true;
        }
        if config.packages == PackagesConfig::default()
            && boot.packages != PackagesConfig::default()
        {
            config.packages = boot.packages.clone();
            changed = true;
        }
        if config.packages.requirements.is_empty() && !boot.packages.requirements.is_empty() {
            config.packages.requirements = boot.packages.requirements.clone();
            changed = true;
        }
        if config.secrets.is_none() && boot.secrets.is_some() {
            config.secrets = boot.secrets.clone();
            changed = true;
        }
        if config.github.username.is_none() && boot.github.username.is_some() {
            config.github.username = boot.github.username.clone();
            changed = true;
        }
        if config.github.repo_name.is_none() && boot.github.repo_name.is_some() {
            config.github.repo_name = boot.github.repo_name.clone();
            changed = true;
        }
        if had_no_tracked {
            config.general.default_mode = boot.general.default_mode;
            config.general.backup = boot.general.backup;
            changed = true;
        }
    }

    if config.general.tracked_files.is_empty() {
        let candidates = [
            crate::paths::manifest_file().ok(),
            crate::paths::compiled_bundled_manifest().ok(),
        ];
        for path in candidates.into_iter().flatten() {
            if !path.exists() {
                continue;
            }
            if let Ok(manifest) = crate::hash::Manifest::load(&path) {
                if !manifest.files.is_empty() {
                    let mut files: Vec<PathBuf> = manifest
                        .files
                        .keys()
                        .map(|rel| PathBuf::from(format!("~/{}", rel.display())))
                        .collect();
                    files.sort();
                    config.general.tracked_files = files;
                    changed = true;
                    break;
                }
            }
        }
    }

    if changed {
        crate::cfg::save(config_path, &config)?;
        ui::info("Restored tracked files, packages, and GitHub identity from the pulled repo");
    }
    Ok(changed)
}

/// Resolve tracked paths to files that can be analyzed for package discovery.
/// Missing home copies fall back to the compiled snapshot so Machine B can
/// discover packages before `setup_dotfiles.sh` has run.
pub fn paths_for_package_discovery(config: &Config) -> Vec<PathBuf> {
    let home = dirs::home_dir();
    let compiled = crate::paths::compiled_dir().ok();
    config
        .general
        .tracked_files
        .iter()
        .map(|tracked| {
            let abs = match &home {
                Some(h) => crate::cfg::expand_tracked_path(tracked, h),
                None => tracked.clone(),
            };
            if abs.exists() {
                return abs;
            }
            if let (Some(h), Some(c)) = (&home, &compiled) {
                let rel = crate::cfg::home_relative_path(&abs, h).unwrap_or_else(|| abs.clone());
                let compiled_copy = c.join(rel);
                if compiled_copy.exists() {
                    return compiled_copy;
                }
            }
            abs
        })
        .collect()
}

fn generate_main_script(_config: &Config, target_os: &str) -> Result<InstallScript> {
    let content = format!(
        r#"#!/usr/bin/env bash
#
# Dotdipper Installation Script
# Generated: {}
# Default target OS (overridable): {}
#

set -euo pipefail

detect_os() {{
    case "$(uname -s)" in
        Darwin) echo macos ;;
        Linux)
            if [[ -f /etc/os-release ]]; then
                # shellcheck disable=SC1091
                . /etc/os-release
                case "${{ID:-}}" in
                    ubuntu|debian) echo ubuntu ;;
                    arch|manjaro|endeavouros) echo arch ;;
                    fedora|rhel|centos) echo fedora ;;
                    *) echo linux ;;
                esac
            else
                echo linux
            fi
            ;;
        *) echo linux ;;
    esac
}}

TARGET_OS="${{DOTDIPPER_TARGET_OS:-$(detect_os)}}"

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

log_info "Starting Dotdipper installation for $TARGET_OS"

SCRIPT_DIR="$(cd "$(dirname "${{BASH_SOURCE[0]}}")" && pwd)"
DOTDIPPER_DIR="${{DOTDIPPER_HOME:-${{XDG_CONFIG_HOME:-$HOME/.config}}/dotdipper}}"
if [[ -d "$SCRIPT_DIR/../.dotdipper" || -d "$SCRIPT_DIR/../.git" ]]; then
    COMPILED_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
else
    COMPILED_DIR="${{DOTDIPPER_COMPILED:-$DOTDIPPER_DIR/compiled}}"
fi
COMPILED_DIR="${{DOTDIPPER_COMPILED:-$COMPILED_DIR}}"
INSTALL_DIR="$SCRIPT_DIR"

mkdir -p "$DOTDIPPER_DIR"
mkdir -p "$COMPILED_DIR"
mkdir -p "$INSTALL_DIR"

if ! command -v git >/dev/null 2>&1; then
    log_warn "Git is not installed; package recipes that need it may fail"
fi

pkg_status=0
if [[ "${{DOTDIPPER_SKIP_PACKAGES:-0}}" == "1" ]]; then
    log_info "Skipping package installation"
elif [[ -f "$INSTALL_DIR/install_${{TARGET_OS}}.sh" ]]; then
    log_info "Installing packages..."
    if ! bash "$INSTALL_DIR/install_${{TARGET_OS}}.sh"; then
        log_warn "Package installation failed; continuing with dotfiles"
        pkg_status=1
    fi
else
    log_warn "Package installation script not found for $TARGET_OS"
fi

log_info "Setting up dotfiles..."
if [[ -f "$INSTALL_DIR/setup_dotfiles.sh" ]]; then
    DOTDIPPER_COMPILED="$COMPILED_DIR" bash "$INSTALL_DIR/setup_dotfiles.sh"
else
    log_warn "Dotfiles setup script not found"
fi

if [[ "$pkg_status" -ne 0 ]]; then
    log_error "Dotfiles were applied, but package installation did not fully succeed"
    exit 1
fi

log_info "Installation complete!"
log_info "Run 'dotdipper status' to check your dotfiles"
"#,
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"),
        target_os
    );

    Ok(InstallScript {
        name: "install.sh".to_string(),
        content,
        path: PathBuf::new(),
    })
}

fn generate_package_script(config: &Config, target_os: &str) -> Result<InstallScript> {
    let (package_manager, install_cmd, update_cmd) = match target_os {
        "macos" => ("brew", "brew install", "brew update"),
        "ubuntu" | "debian" => ("apt", "sudo apt install -y", "sudo apt update"),
        "arch" | "manjaro" => ("pacman", "sudo pacman -S --noconfirm", "sudo pacman -Sy"),
        "fedora" | "redhat" => ("dnf", "sudo dnf install -y", "sudo dnf check-update"),
        _ => ("apt", "sudo apt install -y", "sudo apt update"),
    };

    let packages = &config.packages;
    let mut all_packages = Vec::new();
    let mapper = package_map::PackageMapper::new(target_os).ok();
    let mut push_mapped = |name: &str| {
        if let Some(mapper) = &mapper {
            if let Some(pkg) = mapper.map_binary(name) {
                all_packages.push(pkg);
                return;
            }
        }
        all_packages.push(name.to_string());
    };
    for name in &packages.common {
        push_mapped(name);
    }
    for name in &packages.requirements {
        push_mapped(name);
    }

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
        "fedora" | "redhat" => {
            all_packages.extend(packages.linux.clone());
            all_packages.extend(packages.fedora.clone());
        }
        _ => all_packages.extend(packages.linux.clone()),
    }

    if resolve_install_entries(config)
        .map(|e| e.iter().any(|x| x.encrypted))
        .unwrap_or(false)
    {
        all_packages.push("age".to_string());
    }

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

log_warn() {{
    echo -e "${{YELLOW}}[WARN]${{NC}} $1"
}}

is_installed() {{
    local pkg="$1"
    case "{}" in
        brew)
            brew list --formula "$pkg" >/dev/null 2>&1 || brew list --cask "$pkg" >/dev/null 2>&1
            ;;
        apt)
            dpkg -s "$pkg" >/dev/null 2>&1
            ;;
        pacman)
            pacman -Qi "$pkg" >/dev/null 2>&1
            ;;
        dnf)
            rpm -q "$pkg" >/dev/null 2>&1
            ;;
        *)
            command -v "$pkg" >/dev/null 2>&1
            ;;
    esac
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

failed=0
for package in "${{packages[@]}}"; do
    if is_installed "$package"; then
        log_info "Already installed: $package"
        continue
    fi
    if {} "$package"; then
        log_info "Installed $package"
    else
        log_error "Failed to install $package"
        failed=$((failed + 1))
    fi
done

if [[ "$failed" -ne 0 ]]; then
    log_error "$failed package(s) failed to install"
    exit 1
fi

log_info "Package installation complete"
"#,
        target_os,
        package_manager,
        package_manager
            .split_whitespace()
            .next()
            .unwrap_or(package_manager),
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

fn generate_dotfiles_script(config: &Config) -> Result<InstallScript> {
    let entries = resolve_install_entries(config)?;
    let summary = tracked_setup_summary(&entries);
    let tracked_body = generate_dotfiles_tracked_body(&entries);
    let age_key = age_key_default(config);

    let content = format!(
        r#"#!/usr/bin/env bash
#
# Dotfiles Setup Script
# {}
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

COMPILED_DIR="${{DOTDIPPER_COMPILED:-}}"
if [[ -z "$COMPILED_DIR" ]]; then
    SCRIPT_DIR="$(cd "$(dirname "${{BASH_SOURCE[0]}}")" && pwd)"
    if [[ -d "$SCRIPT_DIR/../.dotdipper" || -d "$SCRIPT_DIR/../.git" ]]; then
        COMPILED_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
    else
        COMPILED_DIR="${{DOTDIPPER_HOME:-${{XDG_CONFIG_HOME:-$HOME/.config}}/dotdipper}}/compiled"
    fi
fi
HOME_DIR="$HOME"
BACKUP_ENABLED={}
if [[ -z "${{AGE_KEY:-}}" ]]; then
    AGE_KEY={}
fi

# Check if compiled directory exists
if [[ ! -d "$COMPILED_DIR" ]]; then
    log_error "Compiled directory not found at $COMPILED_DIR"
    log_info "Run 'dotdipper pull' to download your dotfiles first"
    exit 1
fi

# Function to create backup (matches dotdipper apply when backup is enabled)
backup_file() {{
    local file="$1"
    if [[ "$BACKUP_ENABLED" != "1" ]]; then
        return 0
    fi
    if [[ -e "$file" ]] && [[ ! -L "$file" ]]; then
        local backup="${{file}}.backup.$(date +%Y%m%d_%H%M%S)"
        mv "$file" "$backup"
        log_info "Backed up $file to $backup"
    fi
}}

# Function to ensure parent directory exists
ensure_parent_dir() {{
    local file="$1"
    local parent
    parent=$(dirname "$file")
    if [[ ! -d "$parent" ]]; then
        mkdir -p "$parent"
        log_info "Created directory $parent"
    fi
}}

{}

log_info "Dotfiles setup complete"
"#,
        summary,
        if config.general.backup { "1" } else { "0" },
        age_key,
        tracked_body
    );

    Ok(InstallScript {
        name: "setup_dotfiles.sh".to_string(),
        content,
        path: PathBuf::new(),
    })
}

#[derive(Debug, Clone)]
struct DotfileInstallEntry {
    compiled_rel: PathBuf,
    mode: RestoreMode,
    encrypted: bool,
}

fn mode_token(entry: &DotfileInstallEntry) -> &'static str {
    if entry.encrypted {
        "decrypt"
    } else {
        match entry.mode {
            RestoreMode::Symlink => "symlink",
            RestoreMode::Copy => "copy",
        }
    }
}

fn is_encrypted_rel(rel: &Path) -> bool {
    rel.extension().and_then(|e| e.to_str()) == Some("age")
}

fn age_key_default(config: &Config) -> String {
    let raw = config
        .secrets
        .as_ref()
        .and_then(|s| s.key_path.clone())
        .unwrap_or_else(|| "~/.config/age/keys.txt".to_string());
    if let Some(rest) = raw.strip_prefix("~/") {
        format!("\"$HOME/{}\"", rest.replace('"', "\\\""))
    } else {
        format!("'{}'", raw.replace('\'', "'\"'\"'"))
    }
}

/// Inventory for the install script, in order:
/// 1. `manifest.lock` (home-relative keys — same source as `apply`)
/// 2. files under `compiled/`
/// 3. `general.tracked_files` converted to home-relative paths (existence not required)
fn collect_compiled_rel_paths(config: &Config) -> Result<Vec<PathBuf>> {
    let candidates = [
        crate::paths::manifest_file().ok(),
        crate::paths::compiled_bundled_manifest().ok(),
    ];
    for path in candidates.into_iter().flatten() {
        if path.exists() {
            if let Ok(manifest) = crate::hash::Manifest::load(&path) {
                let mut keys: Vec<PathBuf> = manifest.files.keys().cloned().collect();
                keys.sort();
                if !keys.is_empty() {
                    return Ok(keys);
                }
            }
        }
    }

    if let Ok(compiled) = crate::paths::compiled_dir() {
        if compiled.is_dir() {
            let walked = walk_compiled_files(&compiled)?;
            if !walked.is_empty() {
                return Ok(walked);
            }
        }
    }

    tracked_files_as_rel(config)
}

fn is_reserved_compiled_rel(rel: &Path) -> bool {
    matches!(
        rel.components().next(),
        Some(std::path::Component::Normal(c))
            if matches!(c.to_str(), Some(".git" | ".dotdipper" | "install"))
    )
}

fn walk_compiled_files(compiled: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in WalkDir::new(compiled)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let Ok(rel) = p.strip_prefix(compiled) else {
            continue;
        };
        if is_reserved_compiled_rel(rel) {
            continue;
        }
        out.push(rel.to_path_buf());
    }
    out.sort();
    out.dedup();
    Ok(out)
}

fn tracked_files_as_rel(config: &Config) -> Result<Vec<PathBuf>> {
    let home = dirs::home_dir().context("Failed to find home directory")?;
    let mut out = Vec::new();

    for tracked in &config.general.tracked_files {
        let Some(rel) = crate::cfg::home_relative_path(tracked, &home) else {
            continue;
        };
        let abs = if tracked.is_absolute() {
            tracked.clone()
        } else {
            home.join(&rel)
        };
        if abs.is_dir() {
            for entry in WalkDir::new(&abs)
                .follow_links(false)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let p = entry.path();
                if p.is_file() {
                    if let Ok(child) = p.strip_prefix(&home) {
                        out.push(child.to_path_buf());
                    }
                }
            }
        } else {
            out.push(rel);
        }
    }

    out.sort();
    out.dedup();
    Ok(out)
}

fn resolve_install_entries(config: &Config) -> Result<Vec<DotfileInstallEntry>> {
    let rels = collect_compiled_rel_paths(config)?;
    let mut entries = Vec::new();

    for compiled_rel in rels {
        let encrypted = is_encrypted_rel(&compiled_rel);
        let target_rel = if encrypted {
            compiled_rel.with_extension("")
        } else {
            compiled_rel.clone()
        };

        let file_override = crate::cfg::file_override_for(config, &compiled_rel)
            .or_else(|| crate::cfg::file_override_for(config, &target_rel));

        if file_override.is_some_and(|o| o.exclude) {
            continue;
        }

        let mut mode = file_override
            .and_then(|o| o.mode)
            .unwrap_or(config.general.default_mode);
        if encrypted {
            mode = RestoreMode::Copy;
        }

        entries.push(DotfileInstallEntry {
            compiled_rel,
            mode,
            encrypted,
        });
    }

    Ok(entries)
}

fn tracked_setup_summary(entries: &[DotfileInstallEntry]) -> String {
    if entries.is_empty() {
        return "No tracked files in config (nothing to install)".to_string();
    }

    let decrypt = entries.iter().filter(|e| e.encrypted).count();
    let symlink = entries
        .iter()
        .filter(|e| !e.encrypted && e.mode == RestoreMode::Symlink)
        .count();
    let copy = entries.len().saturating_sub(decrypt + symlink);

    format!(
        "From tracked list / manifest ({} paths: {} symlink, {} copy, {} decrypt)",
        entries.len(),
        symlink,
        copy,
        decrypt
    )
}

fn generate_dotfiles_tracked_body(entries: &[DotfileInstallEntry]) -> String {
    if entries.is_empty() {
        return r#"log_warn "No tracked dotfiles — add paths with 'dotdipper discover --write', or run 'dotdipper pull' / snapshot so manifest.lock exists""#
            .to_string();
    }

    let mut lines = Vec::new();
    lines.push(
        r#"linked=0
copied=0
decrypted=0
missing=0
skipped=0
decrypt_failed=0

while IFS=$'\t' read -r rel mode; do
  [[ -z "$rel" ]] && continue
  source_file="$COMPILED_DIR/$rel"

  if [[ "$mode" == "decrypt" ]]; then
    target_rel="${rel%.age}"
  else
    target_rel="$rel"
  fi
  target_file="$HOME_DIR/$target_rel"

  if [[ ! -f "$source_file" ]]; then
    log_warn "Missing in compiled tree: $rel (run dotdipper snapshot or pull)"
    missing=$((missing + 1))
    continue
  fi

  ensure_parent_dir "$target_file"

  case "$mode" in
    symlink)
      if [[ -L "$target_file" ]] && [[ "$(readlink "$target_file")" == "$source_file" ]]; then
        log_info "Already linked $target_rel"
        skipped=$((skipped + 1))
        continue
      fi
      if [[ -e "$target_file" || -L "$target_file" ]]; then
        backup_file "$target_file"
        rm -f "$target_file"
      fi
      ln -s "$source_file" "$target_file"
      log_info "Linked $target_rel"
      linked=$((linked + 1))
      ;;
    copy)
      if [[ -e "$target_file" || -L "$target_file" ]]; then
        backup_file "$target_file"
        rm -f "$target_file"
      fi
      cp -p "$source_file" "$target_file"
      log_info "Copied $target_rel"
      copied=$((copied + 1))
      ;;
    decrypt)
      if ! command -v age >/dev/null 2>&1; then
        log_error "Cannot decrypt $rel: 'age' is not installed"
        decrypt_failed=$((decrypt_failed + 1))
        continue
      fi
      if [[ ! -f "$AGE_KEY" ]]; then
        log_error "Cannot decrypt $rel: copy your original age identity to $AGE_KEY (do not run 'secrets init' — that creates a new key)"
        decrypt_failed=$((decrypt_failed + 1))
        continue
      fi
      if [[ -e "$target_file" || -L "$target_file" ]]; then
        backup_file "$target_file"
        rm -f "$target_file"
      fi
      if ! age -d -i "$AGE_KEY" -o "$target_file" "$source_file"; then
        log_error "Failed to decrypt $rel"
        decrypt_failed=$((decrypt_failed + 1))
        continue
      fi
      log_info "Decrypted $target_rel"
      decrypted=$((decrypted + 1))
      ;;
    *)
      log_warn "Unknown mode '$mode' for $rel"
      skipped=$((skipped + 1))
      ;;
  esac
done <<'DOTDIPPER_MANIFEST_EOF'"#
            .to_string(),
    );

    for entry in entries {
        let rel = entry.compiled_rel.to_string_lossy();
        if rel.contains('\t') || rel.contains('\n') {
            continue;
        }
        lines.push(format!("{}\t{}", rel, mode_token(entry)));
    }
    lines.push("DOTDIPPER_MANIFEST_EOF".to_string());
    lines.push(
        r#"log_info "Summary: linked=$linked copied=$copied decrypted=$decrypted skipped=$skipped missing=$missing decrypt_failed=$decrypt_failed"
if [[ "$decrypt_failed" -ne 0 ]]; then
  log_error "$decrypt_failed encrypted file(s) were not restored. Copy ~/.config/age/keys.txt from your old machine, then re-run install."
  exit 1
fi"#
            .to_string(),
    );
    lines.join("\n")
}

/// Run the generated installer. Only `install.sh` is executed; it invokes the
/// OS package script and `setup_dotfiles.sh` so each step runs once.
pub fn run_scripts(scripts: &[InstallScript], opts: &ScriptRunOpts) -> Result<()> {
    let Some(main) = scripts.iter().find(|s| s.name == "install.sh") else {
        anyhow::bail!("install.sh was not generated");
    };

    ui::info(&format!("Running {}...", main.name));

    let mut cmd = Command::new("bash");
    cmd.arg(&main.path)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    if opts.skip_packages {
        cmd.env("DOTDIPPER_SKIP_PACKAGES", "1");
    }
    if opts.force {
        cmd.env("DOTDIPPER_FORCE", "1");
    }
    if let Some(os) = &opts.target_os {
        cmd.env("DOTDIPPER_TARGET_OS", os);
    }

    let status = cmd
        .status()
        .with_context(|| format!("Failed to run script: {}", main.name))?;

    if !status.success() {
        anyhow::bail!(
            "Script {} failed with status {}",
            main.name,
            status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "signal".to_string())
        );
    }

    ui::success(&format!("{} completed", main.name));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg::{Config, FileOverride};
    use crate::hash::{FileHash, Manifest};
    use chrono::Utc;
    use serial_test::serial;
    use std::fs;
    use tempfile::TempDir;

    struct EnvGuard {
        home: Option<String>,
        dd: Option<String>,
        xdg: Option<String>,
    }

    impl EnvGuard {
        fn isolate(tmp: &Path) -> (Self, PathBuf, PathBuf) {
            let guard = EnvGuard {
                home: std::env::var("HOME").ok(),
                dd: std::env::var("DOTDIPPER_HOME").ok(),
                xdg: std::env::var("XDG_CONFIG_HOME").ok(),
            };
            let home = tmp.join("home");
            let dd = tmp.join("dotdipper");
            fs::create_dir_all(&home).unwrap();
            fs::create_dir_all(&dd).unwrap();
            std::env::set_var("HOME", &home);
            std::env::set_var("DOTDIPPER_HOME", &dd);
            std::env::remove_var("XDG_CONFIG_HOME");
            (guard, home, dd)
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.home {
                Some(h) => std::env::set_var("HOME", h),
                None => std::env::remove_var("HOME"),
            }
            match &self.dd {
                Some(h) => std::env::set_var("DOTDIPPER_HOME", h),
                None => std::env::remove_var("DOTDIPPER_HOME"),
            }
            match &self.xdg {
                Some(h) => std::env::set_var("XDG_CONFIG_HOME", h),
                None => std::env::remove_var("XDG_CONFIG_HOME"),
            }
        }
    }

    fn file_hash(rel: &str) -> FileHash {
        FileHash {
            path: PathBuf::from(rel),
            hash: "abc".to_string(),
            size: 1,
            mode: 0o644,
            modified: Utc::now(),
        }
    }

    #[test]
    #[serial]
    fn dotfiles_setup_script_reflects_tracked_files_and_modes() {
        let tmp = TempDir::new().unwrap();
        let (_guard, home, dd) = EnvGuard::isolate(tmp.path());
        let _ = dd;

        fs::write(home.join(".vimrc"), b"v").unwrap();
        fs::create_dir_all(home.join(".config/foo")).unwrap();
        fs::write(home.join(".config/foo/bar.toml"), b"{}").unwrap();
        fs::write(home.join(".config/secret.age"), b"ENC").unwrap();

        let mut config = Config::default();
        config.general.tracked_files = vec![
            home.join(".vimrc"),
            home.join(".config/foo"),
            home.join(".config/secret.age"),
        ];
        config.general.default_mode = RestoreMode::Copy;
        config.files.insert(
            "~/.vimrc".to_string(),
            FileOverride {
                mode: Some(RestoreMode::Symlink),
                exclude: false,
                local_only: false,
            },
        );

        let script = generate_dotfiles_script(&config).unwrap();

        assert!(
            script.content.contains("ln -s"),
            "expected symlink for ~/.vimrc override"
        );
        assert!(
            script.content.contains("cp -p"),
            "expected copy for default_mode files"
        );
        assert!(script.content.contains(".vimrc\tsymlink"));
        assert!(script.content.contains("foo/bar.toml\tcopy"));
        assert!(script.content.contains(".config/secret.age\tdecrypt"));
        assert!(script.content.contains("age -d"));
        assert!(!script.content.contains(".config/secret.age\tcopy"));
        assert!(!script.content.contains(".config/secret.age\tsymlink"));
    }

    #[test]
    #[serial]
    fn excluded_tracked_file_omitted_from_script() {
        let tmp = TempDir::new().unwrap();
        let (_guard, home, _dd) = EnvGuard::isolate(tmp.path());
        fs::write(home.join(".ssh_config_local"), b"x").unwrap();

        let mut config = Config::default();
        config.general.tracked_files = vec![home.join(".ssh_config_local")];
        config.files.insert(
            "~/.ssh_config_local".to_string(),
            FileOverride {
                mode: None,
                exclude: true,
                local_only: false,
            },
        );

        let script = generate_dotfiles_script(&config).unwrap();
        assert!(!script.content.contains("ssh_config_local"));
    }

    #[test]
    #[serial]
    fn manifest_drives_script_when_home_files_are_missing() {
        let tmp = TempDir::new().unwrap();
        let (_guard, _home, dd) = EnvGuard::isolate(tmp.path());

        let mut manifest = Manifest::new();
        manifest.add_file(file_hash(".zshrc"));
        manifest.add_file(file_hash(".config/git/config"));
        manifest.save(&dd.join("manifest.lock")).unwrap();

        let config = Config::default();
        let script = generate_dotfiles_script(&config).unwrap();

        assert!(script.content.contains(".zshrc\tsymlink"));
        assert!(script.content.contains(".config/git/config\tsymlink"));
        assert!(!script.content.contains("No tracked dotfiles"));
    }

    #[test]
    #[serial]
    fn tilde_tracked_paths_work_without_existing_files() {
        let tmp = TempDir::new().unwrap();
        let (_guard, _home, _dd) = EnvGuard::isolate(tmp.path());

        let mut config = Config::default();
        config.general.tracked_files = vec![PathBuf::from("~/.vimrc")];
        config.general.default_mode = RestoreMode::Copy;

        let script = generate_dotfiles_script(&config).unwrap();
        assert!(script.content.contains(".vimrc\tcopy"));
    }

    #[test]
    #[serial]
    fn directory_exclude_applies_to_children() {
        let tmp = TempDir::new().unwrap();
        let (_guard, _home, dd) = EnvGuard::isolate(tmp.path());

        let mut manifest = Manifest::new();
        manifest.add_file(file_hash(".config/nvim/init.lua"));
        manifest.add_file(file_hash(".zshrc"));
        manifest.save(&dd.join("manifest.lock")).unwrap();

        let mut config = Config::default();
        config.files.insert(
            "~/.config/nvim".to_string(),
            FileOverride {
                mode: None,
                exclude: true,
                local_only: false,
            },
        );

        let script = generate_dotfiles_script(&config).unwrap();
        assert!(!script.content.contains("init.lua"));
        assert!(script.content.contains(".zshrc"));
    }

    #[test]
    #[serial]
    fn fresh_init_merges_bootstrap_packages() {
        let tmp = TempDir::new().unwrap();
        let (_guard, _home, dd) = EnvGuard::isolate(tmp.path());
        fs::create_dir_all(dd.join("compiled/.dotdipper")).unwrap();
        fs::write(
            dd.join("compiled/.dotdipper/bootstrap.toml"),
            r#"
[packages]
common = ["ripgrep-from-bootstrap"]
"#,
        )
        .unwrap();

        let config = Config::default();
        assert!(is_fresh_init(&config));
        let merged = config_for_scripts(&config);
        assert!(merged
            .packages
            .common
            .iter()
            .any(|p| p == "ripgrep-from-bootstrap"));
    }

    #[test]
    fn install_sh_detects_os_at_runtime() {
        let script = generate_main_script(&Config::default(), "ubuntu").unwrap();
        assert!(script.content.contains("detect_os"));
        assert!(script.content.contains("DOTDIPPER_TARGET_OS"));
        assert!(script.content.contains("DOTDIPPER_SKIP_PACKAGES"));
        assert!(!script.content.contains("$target_os"));
    }

    #[test]
    #[serial]
    fn setup_script_installs_from_compiled_on_empty_home() {
        let tmp = TempDir::new().unwrap();
        let (_guard, home, dd) = EnvGuard::isolate(tmp.path());

        let compiled = dd.join("compiled");
        fs::create_dir_all(&compiled).unwrap();
        fs::write(compiled.join(".vimrc"), b"set nocompatible").unwrap();

        let mut manifest = Manifest::new();
        manifest.add_file(file_hash(".vimrc"));
        manifest.save(&dd.join("manifest.lock")).unwrap();

        let config = Config::default();
        let script = generate_dotfiles_script(&config).unwrap();
        let script_path = tmp.path().join("setup_dotfiles.sh");
        fs::write(&script_path, &script.content).unwrap();

        let status = std::process::Command::new("bash")
            .arg(&script_path)
            .env("HOME", &home)
            .env("DOTDIPPER_HOME", &dd)
            .env("DOTDIPPER_COMPILED", &compiled)
            .status()
            .unwrap();
        assert!(status.success(), "setup_dotfiles.sh should succeed");

        let installed = home.join(".vimrc");
        assert!(installed.exists());
        assert!(installed
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(fs::read_to_string(&installed).unwrap(), "set nocompatible");
    }

    #[test]
    #[serial]
    fn restore_artifacts_copies_bundled_manifest_and_scripts() {
        let tmp = TempDir::new().unwrap();
        let (_guard, _home, dd) = EnvGuard::isolate(tmp.path());

        let compiled = dd.join("compiled");
        fs::create_dir_all(compiled.join(".dotdipper")).unwrap();
        fs::create_dir_all(compiled.join("install")).unwrap();
        fs::write(compiled.join(".dotdipper/manifest.lock"), b"{}").unwrap();
        fs::write(compiled.join("install/install.sh"), b"#!/bin/sh\necho hi\n").unwrap();

        restore_artifacts_from_compiled().unwrap();

        assert!(dd.join("manifest.lock").exists());
        assert!(dd.join("install/install.sh").exists());
    }

    fn copy_tree(src: &Path, dst: &Path) {
        fs::create_dir_all(dst).unwrap();
        for entry in fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let to = dst.join(entry.file_name());
            if entry.path().is_dir() {
                copy_tree(&entry.path(), &to);
            } else {
                fs::copy(entry.path(), to).unwrap();
            }
        }
    }

    #[test]
    #[serial]
    fn machine_b_hydrates_from_machine_a_snapshot() {
        let tmp = TempDir::new().unwrap();
        let bundle = tmp.path().join("compiled-bundle");

        {
            let (_guard, home, dd) = EnvGuard::isolate(&tmp.path().join("machine-a"));
            fs::write(home.join(".vimrc"), b"set number").unwrap();
            fs::write(home.join(".zshrc"), b"export FOO=1").unwrap();

            let config_path = dd.join("config.toml");
            let mut config = Config::default();
            config.github.username = Some("alice".into());
            config.github.repo_name = Some("dotfiles".into());
            config.packages.requirements = vec!["ripgrep".into(), "fd".into()];
            config.general.tracked_files = vec![home.join(".vimrc"), home.join(".zshrc")];
            crate::cfg::save(&config_path, &config).unwrap();
            fs::create_dir_all(dd.join("compiled")).unwrap();
            let config = crate::cfg::load(&config_path).unwrap();
            crate::repo::snapshot(&config, true).unwrap();
            copy_tree(&dd.join("compiled"), &bundle);
        }

        {
            let (_guard, home, dd) = EnvGuard::isolate(&tmp.path().join("machine-b"));
            let config_path = dd.join("config.toml");
            crate::cfg::init(config_path.clone(), true).unwrap();
            copy_tree(&bundle, &dd.join("compiled"));

            let changed = hydrate_from_compiled(&config_path).unwrap();
            assert!(
                changed,
                "fresh machine should hydrate from compiled snapshot"
            );

            let config = crate::cfg::load(&config_path).unwrap();
            assert!(
                config
                    .general
                    .tracked_files
                    .iter()
                    .any(|p| p.ends_with(".vimrc")),
                "tracked_files should include .vimrc, got {:?}",
                config.general.tracked_files
            );
            assert_eq!(config.github.username.as_deref(), Some("alice"));
            assert_eq!(config.github.repo_name.as_deref(), Some("dotfiles"));
            assert!(config
                .packages
                .requirements
                .contains(&"ripgrep".to_string()));
            assert!(config.packages.requirements.contains(&"fd".to_string()));

            fs::write(home.join(".vimrc"), b"local only").unwrap();
            let kept = crate::repo::snapshot(&config, true).unwrap();
            assert!(
                kept.file_count >= 2,
                "partial local files must not drop missing tracked paths"
            );
            let manifest = Manifest::load(&dd.join("manifest.lock")).unwrap();
            assert!(
                manifest.files.keys().any(|p| p.ends_with(".zshrc")),
                "missing .zshrc must be carried forward, got {:?}",
                manifest.files.keys().collect::<Vec<_>>()
            );
            assert_eq!(
                fs::read_to_string(dd.join("compiled/.zshrc")).unwrap(),
                "export FOO=1",
                "compiled copy of a missing home file must be left intact"
            );

            let scripts = generate_scripts_with_export(&config, "ubuntu", false).unwrap();
            let ubuntu = scripts
                .iter()
                .find(|s| s.name == "install_ubuntu.sh")
                .unwrap();
            assert!(
                ubuntu.content.contains("fd-find"),
                "ubuntu script should map portable fd -> fd-find"
            );
            assert!(ubuntu.content.contains("ripgrep") || ubuntu.content.contains("rg"));

            let setup = scripts
                .iter()
                .find(|s| s.name == "setup_dotfiles.sh")
                .unwrap();
            let status = std::process::Command::new("bash")
                .arg(&setup.path)
                .env("HOME", &home)
                .env("DOTDIPPER_HOME", &dd)
                .env("DOTDIPPER_COMPILED", dd.join("compiled"))
                .status()
                .unwrap();
            assert!(
                status.success(),
                "setup_dotfiles.sh should succeed on machine B"
            );
            assert!(home.join(".zshrc").exists());
            assert_eq!(
                fs::read_to_string(home.join(".zshrc")).unwrap(),
                "export FOO=1"
            );
        }
    }

    #[test]
    #[serial]
    fn setup_script_fails_closed_without_age_key_but_still_links_plaintext() {
        let tmp = TempDir::new().unwrap();
        let (_guard, home, dd) = EnvGuard::isolate(tmp.path());

        let compiled = dd.join("compiled");
        fs::create_dir_all(compiled.join(".config")).unwrap();
        fs::write(compiled.join(".vimrc"), b"set number").unwrap();
        fs::write(compiled.join(".config/secret.age"), b"ENC").unwrap();

        let mut manifest = Manifest::new();
        manifest.add_file(file_hash(".vimrc"));
        manifest.add_file(file_hash(".config/secret.age"));
        manifest.save(&dd.join("manifest.lock")).unwrap();

        let config = Config::default();
        let script = generate_dotfiles_script(&config).unwrap();
        let script_path = tmp.path().join("setup_dotfiles.sh");
        fs::write(&script_path, &script.content).unwrap();

        let status = std::process::Command::new("bash")
            .arg(&script_path)
            .env("HOME", &home)
            .env("DOTDIPPER_HOME", &dd)
            .env("DOTDIPPER_COMPILED", &compiled)
            .env("PATH", "/usr/bin:/bin")
            .status()
            .unwrap();
        assert!(
            !status.success(),
            "missing age key must fail the setup script"
        );
        assert!(
            home.join(".vimrc").exists(),
            "plaintext files should still be linked"
        );
        assert!(
            script.content.contains("do not run 'secrets init'"),
            "script must warn against generating a new age key"
        );
    }
}

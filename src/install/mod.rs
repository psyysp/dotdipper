pub mod analyzers;
pub mod discover;
pub mod package_map;
pub mod validators;

use anyhow::{Context, Result};
use os_info::Type as OsType;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
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
# Generated: {}
# Target OS: {}
#

set -euo pipefail

TARGET_OS='{}'

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
if [[ -f "$INSTALL_DIR/install_{}.sh" ]]; then
    bash "$INSTALL_DIR/install_{}.sh"
else
    log_warn "Package installation script not found"
fi

# Set up dotfiles
log_info "Setting up dotfiles..."
if [[ -f "$INSTALL_DIR/setup_dotfiles.sh" ]]; then
    bash "$INSTALL_DIR/setup_dotfiles.sh"
else
    log_warn "Dotfiles setup script not found"
fi

log_info "Installation complete!"
log_info "Run 'dotdipper status' to check your dotfiles"
"#,
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"),
        target_os,
        target_os,
        target_os,
        target_os
    );

    Ok(InstallScript {
        name: "install.sh".to_string(),
        content,
        path: PathBuf::new(),
    })
}

fn generate_package_script(packages: &PackagesConfig, target_os: &str) -> Result<InstallScript> {
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

COMPILED_DIR="${{DOTDIPPER_HOME:-${{XDG_CONFIG_HOME:-$HOME/.config}}/dotdipper}}/compiled"
HOME_DIR="$HOME"
BACKUP_ENABLED={}
AGE_KEY="${{AGE_KEY:-{}}}"

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
        format!("$HOME/{}", rest)
    } else {
        raw
    }
}

/// Inventory for the install script, in order:
/// 1. `manifest.lock` (home-relative keys — same source as `apply`)
/// 2. files under `compiled/`
/// 3. `general.tracked_files` converted to home-relative paths (existence not required)
fn collect_compiled_rel_paths(config: &Config) -> Result<Vec<PathBuf>> {
    if let Ok(manifest_path) = crate::paths::manifest_file() {
        if manifest_path.exists() {
            if let Ok(manifest) = crate::hash::Manifest::load(&manifest_path) {
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
        let rel_str = rel.to_string_lossy();
        if rel_str == ".git" || rel_str.starts_with(".git/") {
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
        r#"while IFS=$'\t' read -r rel mode; do
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
    continue
  fi

  ensure_parent_dir "$target_file"

  case "$mode" in
    symlink)
      if [[ -L "$target_file" ]] && [[ "$(readlink "$target_file")" == "$source_file" ]]; then
        log_info "Already linked $target_rel"
        continue
      fi
      if [[ -e "$target_file" || -L "$target_file" ]]; then
        rm -f "$target_file"
      fi
      ln -s "$source_file" "$target_file"
      log_info "Linked $target_rel"
      ;;
    copy)
      if [[ -e "$target_file" || -L "$target_file" ]]; then
        backup_file "$target_file"
        rm -f "$target_file"
      fi
      cp -p "$source_file" "$target_file"
      log_info "Copied $target_rel"
      ;;
    decrypt)
      if ! command -v age >/dev/null 2>&1; then
        log_warn "Skipping encrypted $rel (age not installed)"
        continue
      fi
      if [[ ! -f "$AGE_KEY" ]]; then
        log_warn "Skipping encrypted $rel (no age key at $AGE_KEY)"
        continue
      fi
      if [[ -e "$target_file" || -L "$target_file" ]]; then
        backup_file "$target_file"
        rm -f "$target_file"
      fi
      if ! age -d -i "$AGE_KEY" -o "$target_file" "$source_file"; then
        log_error "Failed to decrypt $rel"
        continue
      fi
      log_info "Decrypted $target_rel"
      ;;
    *)
      log_warn "Unknown mode '$mode' for $rel"
      ;;
  esac
done <<'DOTDIPPER_FILES'"#
            .to_string(),
    );

    for entry in entries {
        let rel = entry.compiled_rel.to_string_lossy();
        if rel.contains('\t') || rel.contains('\n') {
            continue;
        }
        lines.push(format!("{}\t{}", rel, mode_token(entry)));
    }
    lines.push("DOTDIPPER_FILES".to_string());
    lines.join("\n")
}

/// Run the generated installer. Only `install.sh` is executed; it invokes the
/// OS package script and `setup_dotfiles.sh` so each step runs once.
pub fn run_scripts(scripts: &[InstallScript]) -> Result<()> {
    let Some(main) = scripts.iter().find(|s| s.name == "install.sh") else {
        anyhow::bail!("install.sh was not generated");
    };

    ui::info(&format!("Running {}...", main.name));

    let output = Command::new("bash")
        .arg(&main.path)
        .output()
        .with_context(|| format!("Failed to run script: {}", main.name))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Script {} failed: {}", main.name, stderr);
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
    fn install_sh_binds_target_os() {
        let script = generate_main_script(&Config::default(), "ubuntu").unwrap();
        assert!(script.content.contains("TARGET_OS='ubuntu'"));
        assert!(!script.content.contains("$target_os"));
    }
}

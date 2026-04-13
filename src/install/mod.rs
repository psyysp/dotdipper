pub mod analyzers;
pub mod discover;
pub mod package_map;
pub mod validators;

use anyhow::{Context, Result};
use os_info::Type as OsType;
use shell_escape::escape;
use std::borrow::Cow;
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

log_info "Starting Dotdipper installation for $target_os"

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
    let tracked_body = generate_dotfiles_tracked_body(config)?;
    let summary = tracked_setup_summary(config);

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
        tracked_body
    );

    Ok(InstallScript {
        name: "setup_dotfiles.sh".to_string(),
        content,
        path: PathBuf::new(),
    })
}

/// Expand `general.tracked_files` into concrete file paths (directories become their files).
fn expand_tracked_file_paths(config: &Config) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();

    for tracked in &config.general.tracked_files {
        if !tracked.exists() {
            continue;
        }
        if tracked.is_dir() {
            for entry in WalkDir::new(tracked)
                .follow_links(false)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let p = entry.path();
                if p.is_file() {
                    out.push(p.to_path_buf());
                }
            }
        } else if tracked.is_file() {
            out.push(tracked.clone());
        }
    }

    out.sort();
    out.dedup();
    Ok(out)
}

fn sh_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}

fn tracked_setup_summary(config: &Config) -> String {
    let Ok(paths) = expand_tracked_file_paths(config) else {
        return "Per tracked file from config".to_string();
    };

    if paths.is_empty() {
        return "No tracked files in config (nothing to install)".to_string();
    }

    let symlink_count = paths
        .iter()
        .filter(|p| {
            let Some(rel) = rel_path_under_home(p) else {
                return false;
            };
            let key = format!("~/{}", rel.display());
            if config.files.get(&key).is_some_and(|o| o.exclude) {
                return false;
            }
            let mode = config
                .files
                .get(&key)
                .and_then(|o| o.mode)
                .unwrap_or(config.general.default_mode);
            mode == RestoreMode::Symlink
        })
        .count();

    let total = paths.len();

    format!(
        "Per tracked file from config ({} paths: {} symlink, {} copy)",
        total,
        symlink_count,
        total.saturating_sub(symlink_count)
    )
}

fn rel_path_under_home(path: &Path) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    path.strip_prefix(&home).ok().map(|p| p.to_path_buf())
}

fn target_rel_for_compiled(rel: &Path) -> PathBuf {
    if rel.extension().and_then(|e| e.to_str()) == Some("age") {
        rel.with_extension("")
    } else {
        rel.to_path_buf()
    }
}

fn generate_dotfiles_tracked_body(config: &Config) -> Result<String> {
    let paths = expand_tracked_file_paths(config)?;

    if paths.is_empty() {
        return Ok(
            r#"log_warn "No tracked dotfiles in config — add paths with 'dotdipper discover --write' or edit config.toml""#
                .to_string(),
        );
    }

    let mut lines = Vec::new();

    for abs in paths {
        let Some(rel) = rel_path_under_home(&abs) else {
            lines.push(format!(
                r#"log_warn "Skipping {} (outside $HOME)""#,
                sh_single_quote(&abs.display().to_string())
            ));
            continue;
        };

        let path_key = format!("~/{}", rel.display());
        if config
            .files
            .get(&path_key)
            .is_some_and(|o| o.exclude)
        {
            continue;
        }

        let mode = config
            .files
            .get(&path_key)
            .and_then(|o| o.mode)
            .unwrap_or(config.general.default_mode);

        let compiled_rel = rel;
        let target_rel = target_rel_for_compiled(&compiled_rel);

        let compiled_q = sh_single_quote(&compiled_rel.display().to_string());
        let target_q = sh_single_quote(&target_rel.display().to_string());

        let source_var = format!(
            "$COMPILED_DIR/{}",
            escape(Cow::Borrowed(&compiled_rel.display().to_string()))
        );
        let target_var = format!(
            "$HOME_DIR/{}",
            escape(Cow::Borrowed(&target_rel.display().to_string()))
        );

        match mode {
            RestoreMode::Symlink => {
                lines.push(format!(
                    r#"if [[ -f {source} ]]; then
  ensure_parent_dir {target}
  if [[ -e {target} || -L {target} ]]; then rm -f {target}; fi
  ln -s {source} {target}
  log_info "Linked {tr}"
else
  log_warn "Missing in compiled tree: {cr} (run dotdipper snapshot or pull)"
fi"#,
                    source = source_var,
                    target = target_var,
                    tr = target_q,
                    cr = compiled_q,
                ));
            }
            RestoreMode::Copy => {
                lines.push(format!(
                    r#"if [[ -f {source} ]]; then
  ensure_parent_dir {target}
  if [[ -e {target} || -L {target} ]]; then backup_file {target}; rm -f {target}; fi
  cp -p {source} {target}
  log_info "Copied {tr}"
else
  log_warn "Missing in compiled tree: {cr} (run dotdipper snapshot or pull)"
fi"#,
                    source = source_var,
                    target = target_var,
                    tr = target_q,
                    cr = compiled_q,
                ));
            }
        }
    }

    Ok(lines.join("\n\n"))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg::{Config, FileOverride};
    use serial_test::serial;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    #[serial]
    fn dotfiles_setup_script_reflects_tracked_files_and_modes() {
        let tmp = TempDir::new().unwrap();
        let fake_home = tmp.path();
        fs::write(fake_home.join(".vimrc"), b"v").unwrap();
        fs::create_dir_all(fake_home.join(".config/foo")).unwrap();
        fs::write(fake_home.join(".config/foo/bar.toml"), b"{}").unwrap();
        fs::write(fake_home.join(".config/secret.age"), b"ENC").unwrap();

        let old_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", fake_home.as_os_str());

        let mut config = Config::default();
        config.general.tracked_files = vec![
            fake_home.join(".vimrc"),
            fake_home.join(".config/foo"),
            fake_home.join(".config/secret.age"),
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
        assert!(script.content.contains(".vimrc"));
        assert!(script.content.contains("foo/bar.toml"));
        assert!(
            script.content.contains("$HOME_DIR/.config/secret"),
            "encrypted .age should map target without .age suffix"
        );
        assert!(!script.content.contains("$HOME_DIR/.config/secret.age"));

        match old_home {
            Some(h) => std::env::set_var("HOME", h),
            None => std::env::remove_var("HOME"),
        }
    }

    #[test]
    #[serial]
    fn excluded_tracked_file_omitted_from_script() {
        let tmp = TempDir::new().unwrap();
        let fake_home = tmp.path();
        fs::write(fake_home.join(".ssh_config_local"), b"x").unwrap();

        let old_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", fake_home.as_os_str());

        let mut config = Config::default();
        config.general.tracked_files = vec![fake_home.join(".ssh_config_local")];
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

        match old_home {
            Some(h) => std::env::set_var("HOME", h),
            None => std::env::remove_var("HOME"),
        }
    }
}

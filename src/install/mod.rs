pub mod analyzers;
pub mod discover;
pub mod package_map;
pub mod validators;

use anyhow::{Context, Result};
use os_info::Type as OsType;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use crate::cfg::{Config, PackagesConfig};
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
    let script_dir = dirs::home_dir()
        .context("Failed to find home directory")?
        .join(".dotdipper")
        .join("install");
    
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
    let content = format!(r#"#!/usr/bin/env bash
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
DOTDIPPER_DIR="$HOME/.dotdipper"
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
"#, chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"), target_os, target_os, target_os);

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
    
    let content = format!(r#"#!/usr/bin/env bash
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
        package_manager.split_whitespace().next().unwrap_or(package_manager),
        package_manager,
        update_cmd,
        all_packages.iter().map(|p| format!("    \"{}\"", p)).collect::<Vec<_>>().join("\n"),
        install_cmd
    );

    Ok(InstallScript {
        name: format!("install_{}.sh", target_os),
        content,
        path: PathBuf::new(),
    })
}

fn generate_dotfiles_script(config: &Config) -> Result<InstallScript> {
    let use_symlinks = config.dotfiles.as_ref().map(|d| d.use_symlinks).unwrap_or(false);
    
    let content = format!(r#"#!/usr/bin/env bash
#
# Dotfiles Setup Script
# Method: {}
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

COMPILED_DIR="$HOME/.dotdipper/compiled"
HOME_DIR="$HOME"

# Check if compiled directory exists
if [[ ! -d "$COMPILED_DIR" ]]; then
    log_error "Compiled directory not found at $COMPILED_DIR"
    log_info "Run 'dotdipper pull' to download your dotfiles first"
    exit 1
fi

# Function to create backup
backup_file() {{
    local file="$1"
    if [[ -e "$file" ]] && [[ ! -L "$file" ]]; then
        local backup="${{file}}.backup.$(date +%Y%m%d_%H%M%S)"
        mv "$file" "$backup"
        log_info "Backed up $file to $backup"
    fi
}}

# Function to ensure parent directory exists
ensure_parent_dir() {{
    local file="$1"
    local parent=$(dirname "$file")
    if [[ ! -d "$parent" ]]; then
        mkdir -p "$parent"
        log_info "Created directory $parent"
    fi
}}

{}

log_info "Dotfiles setup complete"
"#,
        if use_symlinks { "symlinks" } else { "copies" },
        if use_symlinks {
            generate_symlink_setup()
        } else {
            generate_copy_setup()
        }
    );

    Ok(InstallScript {
        name: "setup_dotfiles.sh".to_string(),
        content,
        path: PathBuf::new(),
    })
}

fn generate_symlink_setup() -> String {
    r#"# Find all files in compiled directory and create symlinks
find "$COMPILED_DIR" -type f | while read -r source_file; do
    # Get relative path from compiled directory
    rel_path="${source_file#$COMPILED_DIR/}"
    
    # Skip git files
    if [[ "$rel_path" == .git/* ]]; then
        continue
    fi
    
    # Target file in home
    target_file="$HOME_DIR/$rel_path"
    
    # Ensure parent directory exists
    ensure_parent_dir "$target_file"
    
    # Backup existing file if needed
    if [[ -e "$target_file" ]] && [[ ! -L "$target_file" ]]; then
        backup_file "$target_file"
    fi
    
    # Remove existing symlink if it exists
    if [[ -L "$target_file" ]]; then
        rm "$target_file"
    fi
    
    # Create symlink
    ln -s "$source_file" "$target_file"
    log_info "Linked $rel_path"
done"#.to_string()
}

fn generate_copy_setup() -> String {
    r#"# Find all files in compiled directory and copy them
find "$COMPILED_DIR" -type f | while read -r source_file; do
    # Get relative path from compiled directory
    rel_path="${source_file#$COMPILED_DIR/}"
    
    # Skip git files
    if [[ "$rel_path" == .git/* ]]; then
        continue
    fi
    
    # Target file in home
    target_file="$HOME_DIR/$rel_path"
    
    # Ensure parent directory exists
    ensure_parent_dir "$target_file"
    
    # Backup existing file if needed
    backup_file "$target_file"
    
    # Copy file with permissions
    cp -p "$source_file" "$target_file"
    log_info "Copied $rel_path"
done"#.to_string()
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

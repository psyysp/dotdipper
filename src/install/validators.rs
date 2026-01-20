//! Validation and suggestions for discovered packages.
//!
//! This module provides functionality to validate discovered packages,
//! check if they're already installed, and provide suggestions for
//! alternative packages.

use anyhow::Result;
use std::collections::HashSet;
use std::process::Command;

use crate::install::discover::DiscoveryResult;

/// Result of package validation
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Packages that are already installed
    pub installed: HashSet<String>,

    /// Packages that need to be installed
    pub missing: HashSet<String>,

    /// Packages that couldn't be validated
    pub unknown: HashSet<String>,
}

impl ValidationResult {
    /// Create a new empty validation result
    pub fn new() -> Self {
        Self {
            installed: HashSet::new(),
            missing: HashSet::new(),
            unknown: HashSet::new(),
        }
    }

    /// Check if all packages are installed
    pub fn all_installed(&self) -> bool {
        self.missing.is_empty() && self.unknown.is_empty()
    }

    /// Get the total count of packages that need attention
    pub fn needs_attention_count(&self) -> usize {
        self.missing.len() + self.unknown.len()
    }
}

impl Default for ValidationResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Validate discovered packages by checking if they're installed
pub fn validate_packages(discovery_result: &DiscoveryResult) -> Result<ValidationResult> {
    let mut result = ValidationResult::new();

    // Get unique binary names from the discovery result
    let binaries: Vec<&String> = discovery_result.packages.keys().collect();

    for binary in binaries {
        match is_binary_installed(binary) {
            Ok(true) => {
                result.installed.insert(binary.clone());
            }
            Ok(false) => {
                result.missing.insert(binary.clone());
            }
            Err(_) => {
                result.unknown.insert(binary.clone());
            }
        }
    }

    Ok(result)
}

/// Check if a binary is installed on the system
pub fn is_binary_installed(binary: &str) -> Result<bool> {
    // Use the 'which' crate functionality or command
    let output = Command::new("which").arg(binary).output()?;

    Ok(output.status.success())
}

/// Suggest alternative packages for missing binaries
pub fn suggest_alternatives(binary: &str) -> Vec<(&'static str, &'static str)> {
    // Return tuples of (alternative_binary, description)
    match binary {
        "rg" | "ripgrep" => vec![
            ("grep", "Standard grep (slower but universally available)"),
            ("ag", "The Silver Searcher (another fast search tool)"),
        ],
        "fd" | "fd-find" => vec![
            ("find", "Standard find (slower but universally available)"),
        ],
        "bat" => vec![
            ("cat", "Standard cat (no syntax highlighting)"),
            ("less", "Pager with some highlighting support"),
        ],
        "exa" | "eza" | "lsd" => vec![
            ("ls", "Standard ls (no icons or colors)"),
        ],
        "nvim" | "neovim" => vec![
            ("vim", "Original Vim editor"),
            ("vi", "Basic vi editor"),
        ],
        "htop" | "btop" | "bottom" => vec![
            ("top", "Standard top (less features but universally available)"),
        ],
        "delta" | "diff-so-fancy" => vec![
            ("diff", "Standard diff (no syntax highlighting)"),
        ],
        "fzf" => vec![
            ("select", "Basic shell selection (if available)"),
        ],
        "zoxide" => vec![
            ("cd", "Standard cd (no frecency-based jumping)"),
        ],
        "starship" => vec![
            ("PS1", "Custom PS1 prompt configuration"),
        ],
        "jq" => vec![
            ("python", "Use python -m json.tool for basic JSON formatting"),
        ],
        _ => vec![],
    }
}

/// Get installation instructions for a package on the given OS
pub fn get_install_instructions(package: &str, target_os: &str) -> String {
    match target_os {
        "macos" => format!("brew install {}", package),
        "ubuntu" | "debian" => format!("sudo apt install {}", package),
        "arch" | "manjaro" | "endeavouros" => format!("sudo pacman -S {}", package),
        "fedora" | "redhat" | "centos" => format!("sudo dnf install {}", package),
        _ => format!("Install {} using your package manager", package),
    }
}

/// Generate a validation report
pub fn format_validation_report(
    validation_result: &ValidationResult,
    discovery_result: &DiscoveryResult,
    target_os: &str,
) -> String {
    let mut report = String::new();

    // Installed packages
    if !validation_result.installed.is_empty() {
        report.push_str("Already installed:\n");
        for binary in &validation_result.installed {
            report.push_str(&format!("  ✓ {}\n", binary));
        }
        report.push('\n');
    }

    // Missing packages
    if !validation_result.missing.is_empty() {
        report.push_str("Needs installation:\n");
        for binary in &validation_result.missing {
            if let Some(package) = discovery_result.packages.get(binary) {
                let instruction = get_install_instructions(package, target_os);
                report.push_str(&format!("  ✗ {} ({})\n    {}\n", binary, package, instruction));
            }
        }
        report.push('\n');
    }

    // Unknown packages
    if !validation_result.unknown.is_empty() {
        report.push_str("Could not validate:\n");
        for binary in &validation_result.unknown {
            report.push_str(&format!("  ? {}\n", binary));
        }
        report.push('\n');
    }

    // Summary
    report.push_str(&format!(
        "Summary: {} installed, {} missing, {} unknown\n",
        validation_result.installed.len(),
        validation_result.missing.len(),
        validation_result.unknown.len()
    ));

    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_result_new() {
        let result = ValidationResult::new();
        assert!(result.installed.is_empty());
        assert!(result.missing.is_empty());
        assert!(result.unknown.is_empty());
    }

    #[test]
    fn test_all_installed() {
        let mut result = ValidationResult::new();
        result.installed.insert("git".to_string());
        assert!(result.all_installed());

        result.missing.insert("fzf".to_string());
        assert!(!result.all_installed());
    }

    #[test]
    fn test_needs_attention_count() {
        let mut result = ValidationResult::new();
        result.installed.insert("git".to_string());
        result.missing.insert("fzf".to_string());
        result.unknown.insert("custom-tool".to_string());

        assert_eq!(result.needs_attention_count(), 2);
    }

    #[test]
    fn test_get_install_instructions() {
        assert_eq!(get_install_instructions("fzf", "macos"), "brew install fzf");
        assert_eq!(
            get_install_instructions("ripgrep", "ubuntu"),
            "sudo apt install ripgrep"
        );
        assert_eq!(
            get_install_instructions("bat", "arch"),
            "sudo pacman -S bat"
        );
    }

    #[test]
    fn test_suggest_alternatives() {
        let alts = suggest_alternatives("rg");
        assert!(!alts.is_empty());
        assert!(alts.iter().any(|(name, _)| *name == "grep"));

        let no_alts = suggest_alternatives("unknown-tool");
        assert!(no_alts.is_empty());
    }

    #[test]
    fn test_is_binary_installed() {
        // 'ls' should be installed on any Unix system
        assert!(is_binary_installed("ls").unwrap_or(false));

        // Random binary should not be installed
        assert!(!is_binary_installed("definitely-not-installed-binary-xyz").unwrap_or(true));
    }
}

//! Package discovery from dotfiles.
//!
//! This module provides functionality to automatically discover required packages
//! by analyzing dotfiles for binary/tool references and mapping them to OS-specific
//! package names.

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::cfg::Config;
use crate::install::analyzers;
use crate::install::package_map::PackageMapper;

/// Confidence level for detected packages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConfidenceLevel {
    /// Explicit command check: `command -v fzf`, `which binary`
    High,
    /// Binary in PATH or alias definition
    Medium,
    /// String match in comments/config
    Low,
}

impl std::fmt::Display for ConfidenceLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfidenceLevel::High => write!(f, "high"),
            ConfidenceLevel::Medium => write!(f, "medium"),
            ConfidenceLevel::Low => write!(f, "low"),
        }
    }
}

/// Result of package discovery
#[derive(Debug, Clone)]
pub struct DiscoveryResult {
    /// Packages discovered (binary name -> package name)
    pub packages: HashMap<String, String>,

    /// Binaries detected but not mapped to packages
    pub unmapped_binaries: Vec<String>,

    /// Files analyzed
    pub analyzed_files: Vec<PathBuf>,

    /// Confidence level for each binary
    pub confidence: HashMap<String, ConfidenceLevel>,

    /// Errors encountered during analysis (file path -> error message)
    pub errors: HashMap<PathBuf, String>,
}

impl DiscoveryResult {
    /// Create a new empty discovery result
    pub fn new() -> Self {
        Self {
            packages: HashMap::new(),
            unmapped_binaries: Vec::new(),
            analyzed_files: Vec::new(),
            confidence: HashMap::new(),
            errors: HashMap::new(),
        }
    }

    /// Get the total number of packages discovered
    pub fn package_count(&self) -> usize {
        self.packages.len()
    }

    /// Get unique package names (deduplicated)
    pub fn unique_packages(&self) -> Vec<String> {
        let mut packages: Vec<String> = self.packages.values().cloned().collect();
        packages.sort();
        packages.dedup();
        packages
    }

    /// Check if any packages were discovered
    pub fn has_packages(&self) -> bool {
        !self.packages.is_empty()
    }

    /// Check if there were any errors during discovery
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

impl Default for DiscoveryResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration for package discovery
#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    /// Target OS for package mapping
    pub target_os: String,

    /// Whether to include low-confidence matches
    pub include_low_confidence: bool,

    /// Custom binary -> package mappings
    pub custom_mappings: HashMap<String, String>,

    /// Files/patterns to exclude from analysis
    pub exclude_patterns: Vec<String>,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            target_os: crate::install::detect_os(),
            include_low_confidence: false,
            custom_mappings: HashMap::new(),
            exclude_patterns: Vec::new(),
        }
    }
}

/// Discover packages from dotfiles
pub fn discover_packages(config: &Config, discovery_config: &DiscoveryConfig) -> Result<DiscoveryResult> {
    let mut result = DiscoveryResult::new();
    let mut all_binaries = HashSet::new();

    // Get tracked files from config
    let tracked_files = &config.general.tracked_files;

    // Initialize package mapper
    let mut mapper = PackageMapper::new(&discovery_config.target_os)?;

    // Add custom mappings
    for (binary, package) in &discovery_config.custom_mappings {
        mapper.add_custom_mapping(binary.clone(), package.clone());
    }

    // Analyze each tracked file
    for file_path in tracked_files {
        // Skip files matching exclude patterns
        if should_skip_file(file_path, &discovery_config.exclude_patterns) {
            continue;
        }

        // Skip non-existent files
        if !file_path.exists() {
            continue;
        }

        // Try to analyze the file
        match analyzers::analyze_file(file_path) {
            Ok(binaries) => {
                all_binaries.extend(binaries);
                result.analyzed_files.push(file_path.clone());
            }
            Err(e) => {
                // Record the error but continue processing other files
                result.errors.insert(file_path.clone(), e.to_string());
            }
        }
    }

    // Map binaries to packages
    for binary in all_binaries {
        // Set confidence level (default to High for now, could be enhanced)
        result.confidence.insert(binary.clone(), ConfidenceLevel::High);

        match mapper.map_binary(&binary) {
            Some(package_name) => {
                result.packages.insert(binary.clone(), package_name);
            }
            None => {
                result.unmapped_binaries.push(binary);
            }
        }
    }

    // Sort unmapped binaries for consistent output
    result.unmapped_binaries.sort();

    Ok(result)
}

/// Check if a file should be skipped based on exclude patterns
fn should_skip_file(file_path: &PathBuf, exclude_patterns: &[String]) -> bool {
    let path_str = file_path.to_string_lossy();

    for pattern in exclude_patterns {
        // Simple glob-style matching
        if pattern.contains('*') {
            // Convert glob to regex-like pattern
            let regex_pattern = pattern
                .replace('.', "\\.")
                .replace('*', ".*")
                .replace('?', ".");

            if let Ok(re) = regex::Regex::new(&regex_pattern) {
                if re.is_match(&path_str) {
                    return true;
                }
            }
        } else {
            // Exact match or contains
            if path_str.contains(pattern) {
                return true;
            }
        }
    }

    false
}

/// Update configuration with discovered packages
pub fn update_config_with_packages(config_path: &PathBuf, result: &DiscoveryResult) -> Result<()> {
    let mut config = crate::cfg::load(config_path)?;

    // Merge discovered packages with existing common packages
    let mut packages = config.packages.common.clone();

    for package in result.packages.values() {
        if !packages.contains(package) {
            packages.push(package.clone());
        }
    }

    // Sort and deduplicate
    packages.sort();
    packages.dedup();

    config.packages.common = packages;

    crate::cfg::save(config_path, &config)?;

    Ok(())
}

/// Generate a summary of the discovery result for display
pub fn format_discovery_summary(result: &DiscoveryResult) -> String {
    let mut summary = String::new();

    summary.push_str(&format!(
        "Analyzed {} files, found {} packages\n",
        result.analyzed_files.len(),
        result.package_count()
    ));

    if !result.unmapped_binaries.is_empty() {
        summary.push_str(&format!(
            "  {} binaries could not be mapped to packages\n",
            result.unmapped_binaries.len()
        ));
    }

    if !result.errors.is_empty() {
        summary.push_str(&format!(
            "  {} files had errors during analysis\n",
            result.errors.len()
        ));
    }

    summary
}

/// Get a list of packages grouped by binary for display
pub fn get_package_display_list(result: &DiscoveryResult) -> Vec<(String, String, String)> {
    let mut list: Vec<(String, String, String)> = result
        .packages
        .iter()
        .map(|(binary, package)| {
            let confidence = result
                .confidence
                .get(binary)
                .map(|c| c.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            (binary.clone(), package.clone(), confidence)
        })
        .collect();

    // Sort by binary name
    list.sort_by(|a, b| a.0.cmp(&b.0));

    list
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_discovery_result_new() {
        let result = DiscoveryResult::new();
        assert!(result.packages.is_empty());
        assert!(result.unmapped_binaries.is_empty());
        assert!(result.analyzed_files.is_empty());
    }

    #[test]
    fn test_discovery_result_unique_packages() {
        let mut result = DiscoveryResult::new();
        result.packages.insert("rg".to_string(), "ripgrep".to_string());
        result.packages.insert("ripgrep".to_string(), "ripgrep".to_string());
        result.packages.insert("fzf".to_string(), "fzf".to_string());

        let unique = result.unique_packages();
        assert_eq!(unique.len(), 2);
        assert!(unique.contains(&"fzf".to_string()));
        assert!(unique.contains(&"ripgrep".to_string()));
    }

    #[test]
    fn test_should_skip_file() {
        let patterns = vec!["*.bak".to_string(), "/tmp/".to_string()];

        assert!(should_skip_file(&PathBuf::from("/home/user/file.bak"), &patterns));
        assert!(should_skip_file(&PathBuf::from("/tmp/test.txt"), &patterns));
        assert!(!should_skip_file(&PathBuf::from("/home/user/.zshrc"), &patterns));
    }

    #[test]
    fn test_discover_from_zshrc() {
        // Create a temporary zshrc file
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "if command -v fzf > /dev/null; then").unwrap();
        writeln!(temp_file, "    export FZF_DEFAULT_COMMAND='rg --files'").unwrap();
        writeln!(temp_file, "fi").unwrap();
        writeln!(temp_file, "eval \"$(starship init zsh)\"").unwrap();

        let config = Config {
            general: crate::cfg::GeneralConfig {
                tracked_files: vec![temp_file.path().to_path_buf()],
                ..Default::default()
            },
            ..Default::default()
        };

        let discovery_config = DiscoveryConfig {
            target_os: "macos".to_string(),
            ..Default::default()
        };

        let result = discover_packages(&config, &discovery_config).unwrap();

        assert!(result.packages.contains_key("fzf"));
        assert!(result.packages.contains_key("starship"));
    }

    #[test]
    fn test_discovery_config_default() {
        let config = DiscoveryConfig::default();
        assert!(!config.target_os.is_empty());
        assert!(!config.include_low_confidence);
        assert!(config.custom_mappings.is_empty());
    }
}

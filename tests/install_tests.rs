//! Integration tests for the install module

use dotdipper::cfg::Config;
use dotdipper::install::{self, DiscoveryConfig, DiscoveryResult};
use std::io::Write;
use std::path::PathBuf;
use tempfile::{NamedTempFile, TempDir};

#[cfg(test)]
mod os_detection_tests {
    use super::*;

    #[test]
    fn test_detect_os_returns_valid_string() {
        let os = install::detect_os();

        // Should return one of the known OS types
        let valid_os = ["macos", "ubuntu", "arch", "fedora", "linux"];
        assert!(
            valid_os.contains(&os.as_str()),
            "Detected OS '{}' is not in valid list",
            os
        );
    }

    #[test]
    fn test_detect_os_not_empty() {
        let os = install::detect_os();
        assert!(!os.is_empty());
    }
}

#[cfg(test)]
mod discovery_config_tests {
    use super::*;

    #[test]
    fn test_discovery_config_default() {
        let config = DiscoveryConfig::default();

        assert!(!config.target_os.is_empty());
        assert!(!config.include_low_confidence);
        assert!(config.custom_mappings.is_empty());
        assert!(config.exclude_patterns.is_empty());
    }

    #[test]
    fn test_discovery_config_custom() {
        let mut custom_mappings = std::collections::HashMap::new();
        custom_mappings.insert("nvim".to_string(), "neovim".to_string());

        let config = DiscoveryConfig {
            target_os: "macos".to_string(),
            include_low_confidence: true,
            custom_mappings,
            exclude_patterns: vec!["*.bak".to_string()],
        };

        assert_eq!(config.target_os, "macos");
        assert!(config.include_low_confidence);
        assert!(config.custom_mappings.contains_key("nvim"));
        assert_eq!(config.exclude_patterns.len(), 1);
    }
}

#[cfg(test)]
mod discovery_result_tests {
    use super::*;

    #[test]
    fn test_discovery_result_new() {
        let result = DiscoveryResult::new();

        assert!(result.packages.is_empty());
        assert!(result.unmapped_binaries.is_empty());
        assert!(result.analyzed_files.is_empty());
        assert!(result.confidence.is_empty());
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_discovery_result_has_packages() {
        let mut result = DiscoveryResult::new();
        assert!(!result.has_packages());

        result.packages.insert("fzf".to_string(), "fzf".to_string());
        assert!(result.has_packages());
    }

    #[test]
    fn test_discovery_result_has_errors() {
        let mut result = DiscoveryResult::new();
        assert!(!result.has_errors());

        result
            .errors
            .insert(PathBuf::from("/path/to/file"), "error message".to_string());
        assert!(result.has_errors());
    }

    #[test]
    fn test_discovery_result_unique_packages() {
        let mut result = DiscoveryResult::new();

        result
            .packages
            .insert("rg".to_string(), "ripgrep".to_string());
        result
            .packages
            .insert("ripgrep".to_string(), "ripgrep".to_string());
        result.packages.insert("fzf".to_string(), "fzf".to_string());

        let unique = result.unique_packages();

        assert_eq!(unique.len(), 2);
        assert!(unique.contains(&"ripgrep".to_string()));
        assert!(unique.contains(&"fzf".to_string()));
    }

    #[test]
    fn test_discovery_result_package_count() {
        let mut result = DiscoveryResult::new();

        assert_eq!(result.package_count(), 0);

        result.packages.insert("git".to_string(), "git".to_string());
        result.packages.insert("vim".to_string(), "vim".to_string());

        assert_eq!(result.package_count(), 2);
    }
}

#[cfg(test)]
mod discover_from_file_tests {
    use super::*;

    #[test]
    fn test_discover_from_zshrc() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "if command -v fzf > /dev/null; then").unwrap();
        writeln!(temp_file, "    export FZF_DEFAULT_COMMAND='rg --files'").unwrap();
        writeln!(temp_file, "fi").unwrap();
        writeln!(temp_file, "eval \"$(starship init zsh)\"").unwrap();

        let config = Config {
            general: dotdipper::cfg::GeneralConfig {
                tracked_files: vec![temp_file.path().to_path_buf()],
                ..Default::default()
            },
            ..Default::default()
        };

        let discovery_config = DiscoveryConfig {
            target_os: "macos".to_string(),
            ..Default::default()
        };

        let result = install::discover::discover_packages(&config, &discovery_config).unwrap();

        // Should find fzf and starship
        assert!(result.packages.contains_key("fzf"));
        assert!(result.packages.contains_key("starship"));
    }

    #[test]
    fn test_discover_from_bashrc() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "# Bash configuration").unwrap();
        writeln!(temp_file, "alias ll='ls -la'").unwrap();
        writeln!(temp_file, "export EDITOR=nvim").unwrap();
        writeln!(temp_file, "if which bat > /dev/null; then").unwrap();
        writeln!(temp_file, "    alias cat='bat'").unwrap();
        writeln!(temp_file, "fi").unwrap();

        let config = Config {
            general: dotdipper::cfg::GeneralConfig {
                tracked_files: vec![temp_file.path().to_path_buf()],
                ..Default::default()
            },
            ..Default::default()
        };

        let discovery_config = DiscoveryConfig {
            target_os: "ubuntu".to_string(),
            ..Default::default()
        };

        let result = install::discover::discover_packages(&config, &discovery_config).unwrap();

        // Should find nvim and bat
        assert!(result.packages.contains_key("nvim") || result.packages.contains_key("bat"));
    }

    #[test]
    fn test_discover_nonexistent_file() {
        let config = Config {
            general: dotdipper::cfg::GeneralConfig {
                tracked_files: vec![PathBuf::from("/nonexistent/file")],
                ..Default::default()
            },
            ..Default::default()
        };

        let discovery_config = DiscoveryConfig::default();

        // Should not error, just skip nonexistent files
        let result = install::discover::discover_packages(&config, &discovery_config).unwrap();
        assert!(result.analyzed_files.is_empty());
    }
}

#[cfg(test)]
mod confidence_level_tests {
    use dotdipper::install::discover::ConfidenceLevel;

    #[test]
    fn test_confidence_level_display() {
        assert_eq!(format!("{}", ConfidenceLevel::High), "high");
        assert_eq!(format!("{}", ConfidenceLevel::Medium), "medium");
        assert_eq!(format!("{}", ConfidenceLevel::Low), "low");
    }

    #[test]
    fn test_confidence_level_equality() {
        assert_eq!(ConfidenceLevel::High, ConfidenceLevel::High);
        assert_ne!(ConfidenceLevel::High, ConfidenceLevel::Low);
    }
}

#[cfg(test)]
mod install_script_tests {
    use super::*;

    #[test]
    fn test_generate_scripts_creates_files() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        // Create a minimal config
        dotdipper::cfg::init(config_path.clone(), false).unwrap();
        let config = dotdipper::cfg::load(&config_path).unwrap();

        let scripts = install::generate_scripts(&config, "macos").unwrap();

        assert!(!scripts.is_empty());

        // Verify scripts have content
        for script in &scripts {
            assert!(!script.name.is_empty());
            assert!(!script.content.is_empty());
        }
    }

    #[test]
    fn test_script_names() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        dotdipper::cfg::init(config_path.clone(), false).unwrap();
        let config = dotdipper::cfg::load(&config_path).unwrap();

        let scripts = install::generate_scripts(&config, "ubuntu").unwrap();

        let names: Vec<&str> = scripts.iter().map(|s| s.name.as_str()).collect();

        assert!(names.contains(&"install.sh"));
        assert!(names.iter().any(|n| n.starts_with("install_")));
        assert!(names.contains(&"setup_dotfiles.sh"));
    }

    #[test]
    fn test_macos_generated_scripts_contain_app_restore() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        dotdipper::cfg::init(config_path.clone(), false).unwrap();
        let config = dotdipper::cfg::load(&config_path).unwrap();

        let scripts = install::generate_scripts(&config, "macos").unwrap();

        let install_sh = scripts
            .iter()
            .find(|s| s.name == "install.sh")
            .expect("install.sh");
        assert!(install_sh
            .content
            .contains("Starting Dotdipper installation for macos"));
        assert!(
            !install_sh.content.contains("$target_os"),
            "install.sh must interpolate target_os at generation time, not emit an unbound $target_os"
        );

        let macos_pkg = scripts
            .iter()
            .find(|s| s.name == "install_macos.sh")
            .expect("install_macos.sh");

        assert!(
            macos_pkg
                .content
                .contains("https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh"),
            "must contain the official Homebrew installer"
        );
        assert!(
            macos_pkg.content.contains("command -v brew"),
            "must guard Homebrew installation with command -v brew"
        );
        assert!(
            macos_pkg
                .content
                .contains(r#"brew bundle --file="$COMPILED_DIR/Brewfile""#),
            "must run brew bundle against the compiled Brewfile"
        );
        assert!(
            macos_pkg.content.contains("grep -q '^mas '"),
            "must detect mas entries in the Brewfile"
        );
        assert!(
            macos_pkg.content.contains("brew install mas"),
            "must ensure mas is installed when Brewfile has mas entries"
        );
        assert!(
            macos_pkg.content.contains("apps_manifest.toml"),
            "must read unmanaged apps from apps_manifest.toml"
        );
        assert!(
            macos_pkg.content.contains("must install manually"),
            "must print a manual-install section for unmanaged apps"
        );
        assert!(macos_pkg.content.contains("xcode-select"));
    }

    #[test]
    fn test_ubuntu_generated_scripts_omit_macos_app_restore() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        dotdipper::cfg::init(config_path.clone(), false).unwrap();
        let config = dotdipper::cfg::load(&config_path).unwrap();

        let scripts = install::generate_scripts(&config, "ubuntu").unwrap();

        let install_sh = scripts
            .iter()
            .find(|s| s.name == "install.sh")
            .expect("install.sh");
        assert!(install_sh
            .content
            .contains("Starting Dotdipper installation for ubuntu"));
        assert!(!install_sh.content.contains("$target_os"));

        let ubuntu_pkg = scripts
            .iter()
            .find(|s| s.name == "install_ubuntu.sh")
            .expect("install_ubuntu.sh");

        assert!(
            !ubuntu_pkg.content.contains("Brewfile"),
            "Linux scripts must not contain Brewfile restore logic"
        );
        assert!(!ubuntu_pkg.content.contains("brew bundle"));
        assert!(!ubuntu_pkg.content.contains("apps_manifest"));
        assert!(!ubuntu_pkg.content.contains("xcode-select"));
        assert!(!ubuntu_pkg.content.contains("brew install mas"));
        assert!(!ubuntu_pkg
            .content
            .contains("https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh"));
    }
}

#[cfg(test)]
mod package_map_tests {
    use dotdipper::install::PackageMapper;

    #[test]
    fn test_package_mapper_creation() {
        let mapper = PackageMapper::new("macos").unwrap();

        // Should have mappings loaded
        assert!(mapper.map_binary("git").is_some());
    }

    #[test]
    fn test_package_mapper_common_binaries() {
        let mapper = PackageMapper::new("macos").unwrap();

        // These should all map
        assert!(mapper.map_binary("git").is_some());
        assert!(mapper.map_binary("vim").is_some());
        assert!(mapper.map_binary("curl").is_some());
    }

    #[test]
    fn test_package_mapper_custom_mapping() {
        let mut mapper = PackageMapper::new("ubuntu").unwrap();

        mapper.add_custom_mapping("mybin".to_string(), "mypackage".to_string());

        assert_eq!(mapper.map_binary("mybin"), Some("mypackage".to_string()));
    }

    #[test]
    fn test_package_mapper_unknown_binary() {
        let mapper = PackageMapper::new("macos").unwrap();

        // Unknown binary should return the binary name as the package name
        // (this is by design - assume package name equals binary name)
        assert_eq!(
            mapper.map_binary("totally_unknown_binary_xyz"),
            Some("totally_unknown_binary_xyz".to_string())
        );
    }
}

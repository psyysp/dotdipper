//! Integration tests for package discovery from dotfiles.

use std::collections::HashMap;
use std::io::Write;
use tempfile::{tempdir, NamedTempFile};

// Helper to create a temporary config
fn create_test_config(tracked_files: Vec<std::path::PathBuf>) -> dotdipper::cfg::Config {
    dotdipper::cfg::Config {
        general: dotdipper::cfg::GeneralConfig {
            tracked_files,
            ..Default::default()
        },
        ..Default::default()
    }
}

mod shell_analyzer_tests {
    use super::*;

    #[test]
    fn test_discovers_command_v_checks() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "#!/bin/bash").unwrap();
        writeln!(temp_file, "if command -v fzf > /dev/null; then").unwrap();
        writeln!(temp_file, "    echo 'fzf is installed'").unwrap();
        writeln!(temp_file, "fi").unwrap();
        writeln!(temp_file, "command -v rg && echo 'ripgrep found'").unwrap();

        let config = create_test_config(vec![temp_file.path().to_path_buf()]);
        let discovery_config = dotdipper::install::DiscoveryConfig {
            target_os: "macos".to_string(),
            include_low_confidence: false,
            custom_mappings: HashMap::new(),
            exclude_patterns: Vec::new(),
        };

        let result =
            dotdipper::install::discover::discover_packages(&config, &discovery_config).unwrap();

        assert!(result.packages.contains_key("fzf"), "Should discover fzf");
        assert!(result.packages.contains_key("rg"), "Should discover rg");
    }

    #[test]
    fn test_discovers_which_checks() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "if which bat > /dev/null 2>&1; then").unwrap();
        writeln!(temp_file, "    alias cat='bat'").unwrap();
        writeln!(temp_file, "fi").unwrap();

        let config = create_test_config(vec![temp_file.path().to_path_buf()]);
        let discovery_config = dotdipper::install::DiscoveryConfig {
            target_os: "macos".to_string(),
            include_low_confidence: false,
            custom_mappings: HashMap::new(),
            exclude_patterns: Vec::new(),
        };

        let result =
            dotdipper::install::discover::discover_packages(&config, &discovery_config).unwrap();

        assert!(result.packages.contains_key("bat"), "Should discover bat");
    }

    #[test]
    fn test_discovers_eval_patterns() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "eval \"$(starship init zsh)\"").unwrap();
        writeln!(temp_file, "eval \"$(zoxide init zsh)\"").unwrap();

        let config = create_test_config(vec![temp_file.path().to_path_buf()]);
        let discovery_config = dotdipper::install::DiscoveryConfig {
            target_os: "macos".to_string(),
            include_low_confidence: false,
            custom_mappings: HashMap::new(),
            exclude_patterns: Vec::new(),
        };

        let result =
            dotdipper::install::discover::discover_packages(&config, &discovery_config).unwrap();

        assert!(
            result.packages.contains_key("starship"),
            "Should discover starship"
        );
        assert!(
            result.packages.contains_key("zoxide"),
            "Should discover zoxide"
        );
    }

    #[test]
    fn test_discovers_aliases() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "alias ll='exa -la'").unwrap();
        writeln!(temp_file, "alias cat='bat --paging=never'").unwrap();

        let config = create_test_config(vec![temp_file.path().to_path_buf()]);
        let discovery_config = dotdipper::install::DiscoveryConfig {
            target_os: "macos".to_string(),
            include_low_confidence: false,
            custom_mappings: HashMap::new(),
            exclude_patterns: Vec::new(),
        };

        let result =
            dotdipper::install::discover::discover_packages(&config, &discovery_config).unwrap();

        assert!(
            result.packages.contains_key("exa"),
            "Should discover exa from alias"
        );
        assert!(
            result.packages.contains_key("bat"),
            "Should discover bat from alias"
        );
    }

    #[test]
    fn test_filters_shell_builtins() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "if command -v echo > /dev/null; then").unwrap();
        writeln!(temp_file, "    echo 'test'").unwrap();
        writeln!(temp_file, "fi").unwrap();
        writeln!(temp_file, "command -v cd").unwrap();

        let config = create_test_config(vec![temp_file.path().to_path_buf()]);
        let discovery_config = dotdipper::install::DiscoveryConfig {
            target_os: "macos".to_string(),
            include_low_confidence: false,
            custom_mappings: HashMap::new(),
            exclude_patterns: Vec::new(),
        };

        let result =
            dotdipper::install::discover::discover_packages(&config, &discovery_config).unwrap();

        // echo and cd are builtins, should not be discovered
        assert!(
            !result.packages.contains_key("echo"),
            "Should not discover echo (builtin)"
        );
        assert!(
            !result.packages.contains_key("cd"),
            "Should not discover cd (builtin)"
        );
    }
}

mod package_mapping_tests {
    use super::*;

    #[test]
    fn test_macos_package_mapping() {
        // Use a properly named shell file to ensure shell analyzer is used
        let dir = tempdir().unwrap();
        let zshrc_path = dir.path().join(".zshrc");
        std::fs::write(&zshrc_path, "command -v rg && command -v fd").unwrap();

        let config = create_test_config(vec![zshrc_path]);
        let discovery_config = dotdipper::install::DiscoveryConfig {
            target_os: "macos".to_string(),
            include_low_confidence: false,
            custom_mappings: HashMap::new(),
            exclude_patterns: Vec::new(),
        };

        let result =
            dotdipper::install::discover::discover_packages(&config, &discovery_config).unwrap();

        // rg should map to ripgrep on macOS
        assert_eq!(result.packages.get("rg"), Some(&"ripgrep".to_string()));
        // fd should map to fd on macOS
        assert_eq!(result.packages.get("fd"), Some(&"fd".to_string()));
    }

    #[test]
    fn test_ubuntu_package_mapping() {
        let dir = tempdir().unwrap();
        let bashrc_path = dir.path().join(".bashrc");
        std::fs::write(&bashrc_path, "command -v fd\ncommand -v docker").unwrap();

        let config = create_test_config(vec![bashrc_path]);
        let discovery_config = dotdipper::install::DiscoveryConfig {
            target_os: "ubuntu".to_string(),
            include_low_confidence: false,
            custom_mappings: HashMap::new(),
            exclude_patterns: Vec::new(),
        };

        let result =
            dotdipper::install::discover::discover_packages(&config, &discovery_config).unwrap();

        // fd should map to fd-find on Ubuntu
        assert_eq!(result.packages.get("fd"), Some(&"fd-find".to_string()));
        // docker should map to docker.io on Ubuntu
        assert_eq!(
            result.packages.get("docker"),
            Some(&"docker.io".to_string())
        );
    }

    #[test]
    fn test_arch_package_mapping() {
        let dir = tempdir().unwrap();
        let profile_path = dir.path().join(".profile");
        std::fs::write(&profile_path, "command -v gh").unwrap();

        let config = create_test_config(vec![profile_path]);
        let discovery_config = dotdipper::install::DiscoveryConfig {
            target_os: "arch".to_string(),
            include_low_confidence: false,
            custom_mappings: HashMap::new(),
            exclude_patterns: Vec::new(),
        };

        let result =
            dotdipper::install::discover::discover_packages(&config, &discovery_config).unwrap();

        // gh should map to github-cli on Arch
        assert_eq!(result.packages.get("gh"), Some(&"github-cli".to_string()));
    }

    #[test]
    fn test_custom_mapping() {
        let dir = tempdir().unwrap();
        let zshrc_path = dir.path().join(".zshrc");
        std::fs::write(&zshrc_path, "command -v my-custom-tool").unwrap();

        let config = create_test_config(vec![zshrc_path]);

        let mut custom_mappings = HashMap::new();
        custom_mappings.insert("my-custom-tool".to_string(), "custom-package".to_string());

        let discovery_config = dotdipper::install::DiscoveryConfig {
            target_os: "macos".to_string(),
            include_low_confidence: false,
            custom_mappings,
            exclude_patterns: Vec::new(),
        };

        let result =
            dotdipper::install::discover::discover_packages(&config, &discovery_config).unwrap();

        assert_eq!(
            result.packages.get("my-custom-tool"),
            Some(&"custom-package".to_string())
        );
    }
}

mod git_analyzer_tests {
    use super::*;

    #[test]
    fn test_discovers_delta() {
        let dir = tempdir().unwrap();
        let gitconfig_path = dir.path().join(".gitconfig");

        std::fs::write(
            &gitconfig_path,
            r#"
[core]
    pager = delta

[interactive]
    diffFilter = delta --color-only

[delta]
    navigate = true
    light = false
"#,
        )
        .unwrap();

        let config = create_test_config(vec![gitconfig_path]);
        let discovery_config = dotdipper::install::DiscoveryConfig {
            target_os: "macos".to_string(),
            include_low_confidence: false,
            custom_mappings: HashMap::new(),
            exclude_patterns: Vec::new(),
        };

        let result =
            dotdipper::install::discover::discover_packages(&config, &discovery_config).unwrap();

        assert!(
            result.packages.contains_key("delta"),
            "Should discover delta"
        );
        assert_eq!(result.packages.get("delta"), Some(&"git-delta".to_string()));
    }

    #[test]
    fn test_discovers_gpg() {
        let dir = tempdir().unwrap();
        let gitconfig_path = dir.path().join(".gitconfig");

        std::fs::write(
            &gitconfig_path,
            r#"
[commit]
    gpgsign = true

[gpg]
    program = gpg
"#,
        )
        .unwrap();

        let config = create_test_config(vec![gitconfig_path]);
        let discovery_config = dotdipper::install::DiscoveryConfig {
            target_os: "macos".to_string(),
            include_low_confidence: false,
            custom_mappings: HashMap::new(),
            exclude_patterns: Vec::new(),
        };

        let result =
            dotdipper::install::discover::discover_packages(&config, &discovery_config).unwrap();

        assert!(result.packages.contains_key("gpg"), "Should discover gpg");
    }
}

mod vim_analyzer_tests {
    use super::*;

    #[test]
    fn test_discovers_fzf_from_vimrc() {
        let dir = tempdir().unwrap();
        let vimrc_path = dir.path().join(".vimrc");

        std::fs::write(
            &vimrc_path,
            r#"
" Plugin configuration
call plug#begin()
Plug 'junegunn/fzf', { 'do': { -> fzf#install() } }
Plug 'junegunn/fzf.vim'
call plug#end()

" FZF configuration
let g:fzf_layout = { 'down': '40%' }
"#,
        )
        .unwrap();

        let config = create_test_config(vec![vimrc_path]);
        let discovery_config = dotdipper::install::DiscoveryConfig {
            target_os: "macos".to_string(),
            include_low_confidence: false,
            custom_mappings: HashMap::new(),
            exclude_patterns: Vec::new(),
        };

        let result =
            dotdipper::install::discover::discover_packages(&config, &discovery_config).unwrap();

        assert!(
            result.packages.contains_key("fzf"),
            "Should discover fzf from vim config"
        );
    }

    #[test]
    fn test_discovers_lsp_servers() {
        let dir = tempdir().unwrap();
        let init_lua_path = dir.path().join("init.lua");

        std::fs::write(
            &init_lua_path,
            r#"
-- LSP configuration
require'lspconfig'.rust_analyzer.setup{}
require'lspconfig'.tsserver.setup{}
"#,
        )
        .unwrap();

        let config = create_test_config(vec![init_lua_path]);
        let discovery_config = dotdipper::install::DiscoveryConfig {
            target_os: "macos".to_string(),
            include_low_confidence: false,
            custom_mappings: HashMap::new(),
            exclude_patterns: Vec::new(),
        };

        let result =
            dotdipper::install::discover::discover_packages(&config, &discovery_config).unwrap();

        assert!(
            result.packages.contains_key("rust-analyzer"),
            "Should discover rust-analyzer"
        );
        assert!(
            result.packages.contains_key("typescript-language-server"),
            "Should discover typescript-language-server"
        );
    }
}

mod discovery_result_tests {
    use super::*;

    #[test]
    fn test_unique_packages() {
        let mut temp_file = NamedTempFile::new().unwrap();
        // Both rg and ripgrep should map to the same package
        writeln!(temp_file, "command -v rg").unwrap();
        writeln!(temp_file, "command -v ripgrep").unwrap();
        writeln!(temp_file, "command -v fzf").unwrap();

        let config = create_test_config(vec![temp_file.path().to_path_buf()]);
        let discovery_config = dotdipper::install::DiscoveryConfig {
            target_os: "macos".to_string(),
            include_low_confidence: false,
            custom_mappings: HashMap::new(),
            exclude_patterns: Vec::new(),
        };

        let result =
            dotdipper::install::discover::discover_packages(&config, &discovery_config).unwrap();
        let unique = result.unique_packages();

        // Should have fzf and ripgrep (deduplicated)
        assert!(unique.contains(&"fzf".to_string()));
        assert!(unique.contains(&"ripgrep".to_string()));
        // Count should reflect deduplication
        assert!(unique.len() <= result.packages.len());
    }

    #[test]
    fn test_exclude_patterns() {
        let dir = tempdir().unwrap();

        let zshrc_path = dir.path().join(".zshrc");
        std::fs::write(&zshrc_path, "command -v fzf").unwrap();

        let backup_path = dir.path().join(".zshrc.bak");
        std::fs::write(&backup_path, "command -v obsolete-tool").unwrap();

        let config = create_test_config(vec![zshrc_path, backup_path]);
        let discovery_config = dotdipper::install::DiscoveryConfig {
            target_os: "macos".to_string(),
            include_low_confidence: false,
            custom_mappings: HashMap::new(),
            exclude_patterns: vec!["*.bak".to_string()],
        };

        let result =
            dotdipper::install::discover::discover_packages(&config, &discovery_config).unwrap();

        // Should find fzf but not obsolete-tool (from excluded file)
        assert!(result.packages.contains_key("fzf"));
        assert!(!result.packages.contains_key("obsolete-tool"));
    }

    #[test]
    fn test_handles_missing_files() {
        let config = create_test_config(vec![std::path::PathBuf::from("/nonexistent/path/.zshrc")]);

        let discovery_config = dotdipper::install::DiscoveryConfig {
            target_os: "macos".to_string(),
            include_low_confidence: false,
            custom_mappings: HashMap::new(),
            exclude_patterns: Vec::new(),
        };

        // Should not error, just skip missing files
        let result =
            dotdipper::install::discover::discover_packages(&config, &discovery_config).unwrap();

        assert!(result.packages.is_empty());
        assert!(result.analyzed_files.is_empty());
    }

    #[test]
    fn test_multiple_files() {
        let dir = tempdir().unwrap();

        let zshrc_path = dir.path().join(".zshrc");
        std::fs::write(&zshrc_path, "command -v fzf\neval \"$(starship init zsh)\"").unwrap();

        let gitconfig_path = dir.path().join(".gitconfig");
        std::fs::write(&gitconfig_path, "[core]\n    pager = delta").unwrap();

        let config = create_test_config(vec![zshrc_path, gitconfig_path]);
        let discovery_config = dotdipper::install::DiscoveryConfig {
            target_os: "macos".to_string(),
            include_low_confidence: false,
            custom_mappings: HashMap::new(),
            exclude_patterns: Vec::new(),
        };

        let result =
            dotdipper::install::discover::discover_packages(&config, &discovery_config).unwrap();

        // Should find packages from both files
        assert!(result.packages.contains_key("fzf"));
        assert!(result.packages.contains_key("starship"));
        assert!(result.packages.contains_key("delta"));

        // Should have analyzed both files
        assert_eq!(result.analyzed_files.len(), 2);
    }
}

mod validators_tests {
    #[test]
    fn test_get_install_instructions() {
        let macos_instruction =
            dotdipper::install::validators::get_install_instructions("ripgrep", "macos");
        assert!(macos_instruction.contains("brew install"));

        let ubuntu_instruction =
            dotdipper::install::validators::get_install_instructions("ripgrep", "ubuntu");
        assert!(ubuntu_instruction.contains("apt install"));

        let arch_instruction =
            dotdipper::install::validators::get_install_instructions("ripgrep", "arch");
        assert!(arch_instruction.contains("pacman"));
    }

    #[test]
    fn test_is_binary_installed() {
        // 'ls' should always be installed on Unix
        let ls_installed =
            dotdipper::install::validators::is_binary_installed("ls").unwrap_or(false);
        assert!(ls_installed, "ls should be installed");

        // Random name should not be installed
        let random_installed =
            dotdipper::install::validators::is_binary_installed("xyz123notreal").unwrap_or(true);
        assert!(!random_installed, "random binary should not be installed");
    }
}

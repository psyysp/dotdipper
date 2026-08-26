//! Integration tests for the vcs (version control) module

use std::process::Command;

#[cfg(test)]
mod git_check_tests {
    use super::*;

    fn is_git_installed() -> bool {
        Command::new("git")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[test]
    fn test_git_version_command() {
        if !is_git_installed() {
            println!("Skipping test: git not installed");
            return;
        }

        let output = Command::new("git").arg("--version").output().unwrap();

        assert!(output.status.success());

        let version_str = String::from_utf8_lossy(&output.stdout);
        assert!(version_str.contains("git version"));
    }

    #[test]
    fn test_which_git() {
        if !is_git_installed() {
            println!("Skipping test: git not installed");
            return;
        }

        let result = which::which("git");
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod gh_check_tests {
    use super::*;

    fn is_gh_installed() -> bool {
        Command::new("gh")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[test]
    fn test_gh_version_command() {
        if !is_gh_installed() {
            println!("Skipping test: gh (GitHub CLI) not installed");
            return;
        }

        let output = Command::new("gh").arg("--version").output().unwrap();

        assert!(output.status.success());
    }

    #[test]
    fn test_which_gh() {
        if !is_gh_installed() {
            println!("Skipping test: gh not installed");
            return;
        }

        let result = which::which("gh");
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod github_config_tests {
    use dotdipper::cfg::{Config, GitHubConfig};

    #[test]
    fn test_github_config_default() {
        let config = GitHubConfig::default();

        assert!(config.username.is_none());
        assert!(config.repo_name.is_none());
        assert!(config.branch.is_none());
        assert!(config.private);
    }

    #[test]
    fn test_github_config_with_values() {
        let config = GitHubConfig {
            username: Some("testuser".to_string()),
            repo_name: Some("dotfiles".to_string()),
            branch: None,
            private: true,
        };

        assert_eq!(config.username, Some("testuser".to_string()));
        assert_eq!(config.repo_name, Some("dotfiles".to_string()));
        assert!(config.private);
    }

    #[test]
    fn test_github_config_public_repo() {
        let config = GitHubConfig {
            username: Some("testuser".to_string()),
            repo_name: Some("public-dotfiles".to_string()),
            branch: None,
            private: false,
        };

        assert!(!config.private);
    }

    #[test]
    fn test_github_config_serialization() {
        let config_str = r#"
username = "myuser"
repo_name = "my-dotfiles"
private = false
"#;

        let config: GitHubConfig = toml::from_str(config_str).unwrap();

        assert_eq!(config.username, Some("myuser".to_string()));
        assert_eq!(config.repo_name, Some("my-dotfiles".to_string()));
        assert!(!config.private);
    }

    #[test]
    fn test_github_config_in_full_config() {
        let config_str = r#"
[general]
default_mode = "symlink"

[github]
username = "psyysp"
repo_name = "dotfiles"
private = true
"#;

        let config: Config = toml::from_str(config_str).unwrap();

        assert_eq!(config.github.username, Some("psyysp".to_string()));
        assert_eq!(config.github.repo_name, Some("dotfiles".to_string()));
        assert!(config.github.private);
    }
}

#[cfg(test)]
mod commit_message_tests {
    #[test]
    fn test_commit_message_format() {
        let message = "Update zsh configuration";

        // Commit messages should not be empty
        assert!(!message.is_empty());

        // Should not start with whitespace
        assert!(!message.starts_with(' '));
    }

    #[test]
    fn test_default_commit_message() {
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S");
        let default_message = format!("Dotdipper snapshot: {}", timestamp);

        assert!(default_message.contains("Dotdipper snapshot"));
    }

    #[test]
    fn test_commit_message_with_file_count() {
        let file_count = 42;
        let message = format!("Updated {} files", file_count);

        assert!(message.contains("42"));
    }
}

#[cfg(test)]
mod git_operations_tests {
    use std::fs;
    use tempfile::TempDir;

    fn is_git_installed() -> bool {
        which::which("git").is_ok()
    }

    #[test]
    fn test_git_init() {
        if !is_git_installed() {
            println!("Skipping test: git not installed");
            return;
        }

        let temp_dir = TempDir::new().unwrap();

        let output = std::process::Command::new("git")
            .arg("init")
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        assert!(output.status.success());
        assert!(temp_dir.path().join(".git").exists());
    }

    #[test]
    fn test_git_status_clean_repo() {
        if !is_git_installed() {
            println!("Skipping test: git not installed");
            return;
        }

        let temp_dir = TempDir::new().unwrap();

        // Init repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        // Configure git user
        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        // Status should work
        let output = std::process::Command::new("git")
            .arg("status")
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        assert!(output.status.success());
    }

    #[test]
    fn test_git_add_and_commit() {
        if !is_git_installed() {
            println!("Skipping test: git not installed");
            return;
        }

        let temp_dir = TempDir::new().unwrap();

        // Init repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        // Configure git user
        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        // Create a file
        fs::write(temp_dir.path().join("test.txt"), "content").unwrap();

        // Add file
        let add_output = std::process::Command::new("git")
            .args(["add", "test.txt"])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        assert!(add_output.status.success());

        // Commit
        let commit_output = std::process::Command::new("git")
            .args(["commit", "-m", "Test commit"])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        assert!(commit_output.status.success());
    }
}

#[cfg(test)]
mod push_pull_tests {
    use dotdipper::cfg::Config;

    #[test]
    fn test_push_requires_github_config() {
        let config = Config::default();

        // Without github username/repo, push should require configuration
        assert!(config.github.username.is_none());
        assert!(config.github.repo_name.is_none());
    }

    #[test]
    fn test_pull_requires_github_config() {
        let config = Config::default();

        // Without github username/repo, pull should require configuration
        assert!(config.github.username.is_none());
        assert!(config.github.repo_name.is_none());
    }

    #[test]
    fn test_configured_for_push() {
        let mut config = Config::default();
        config.github.username = Some("testuser".to_string());
        config.github.repo_name = Some("dotfiles".to_string());

        // Now properly configured
        assert!(config.github.username.is_some());
        assert!(config.github.repo_name.is_some());
    }
}

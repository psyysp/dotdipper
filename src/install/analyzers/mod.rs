//! File type specific analyzers for detecting binary dependencies in dotfiles.
//!
//! Each analyzer is specialized for a particular file type and knows how to
//! extract binary/tool references from configuration files.

pub mod generic;
pub mod git;
pub mod shell;
pub mod vim;

use anyhow::Result;
use std::collections::HashSet;
use std::path::Path;

/// Represents the confidence level of a detected binary
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DetectionConfidence {
    /// Explicit command check: `command -v fzf`, `which binary`
    High,
    /// Binary in PATH or alias definition
    Medium,
    /// String match in comments/config
    Low,
}

/// A detected binary with its confidence level
#[derive(Debug, Clone)]
pub struct DetectedBinary {
    pub name: String,
    pub confidence: DetectionConfidence,
    pub source_line: Option<usize>,
}

impl DetectedBinary {
    pub fn new(name: &str, confidence: DetectionConfidence) -> Self {
        Self {
            name: name.to_string(),
            confidence,
            source_line: None,
        }
    }

    pub fn with_line(mut self, line: usize) -> Self {
        self.source_line = Some(line);
        self
    }
}

/// Analyze a file and extract binary dependencies based on file type
pub fn analyze_file(file_path: &Path) -> Result<HashSet<String>> {
    let content = std::fs::read_to_string(file_path)?;
    let file_name = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    let extension = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

    // Determine file type and use appropriate analyzer
    let binaries = match extension {
        "zsh" | "bash" | "sh" => shell::analyze(&content)?,
        "vim" | "nvim" => vim::analyze(&content)?,
        _ => {
            // Try to detect file type from filename
            if is_shell_config(file_name) {
                shell::analyze(&content)?
            } else if is_vim_config(file_name) {
                vim::analyze(&content)?
            } else if is_git_config(file_name) {
                git::analyze(&content)?
            } else {
                // Fall back to generic analysis
                generic::analyze(&content, file_path)?
            }
        }
    };

    Ok(binaries)
}

/// Check if a filename indicates a shell configuration file
fn is_shell_config(name: &str) -> bool {
    matches!(
        name,
        ".zshrc"
            | ".bashrc"
            | ".bash_profile"
            | ".profile"
            | ".zprofile"
            | ".zshenv"
            | ".bash_aliases"
            | ".zsh_aliases"
            | ".aliases"
    )
}

/// Check if a filename indicates a vim/neovim configuration file
fn is_vim_config(name: &str) -> bool {
    matches!(name, ".vimrc" | ".gvimrc" | "init.vim" | "init.lua")
        || name.ends_with(".vim")
        || name.ends_with(".lua")
}

/// Check if a filename indicates a git configuration file
fn is_git_config(name: &str) -> bool {
    matches!(name, ".gitconfig" | ".gitignore" | ".gitattributes")
        || name.contains("git")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_shell_config() {
        assert!(is_shell_config(".zshrc"));
        assert!(is_shell_config(".bashrc"));
        assert!(is_shell_config(".profile"));
        assert!(!is_shell_config(".vimrc"));
        assert!(!is_shell_config("config.toml"));
    }

    #[test]
    fn test_is_vim_config() {
        assert!(is_vim_config(".vimrc"));
        assert!(is_vim_config("init.vim"));
        assert!(is_vim_config("init.lua"));
        assert!(!is_vim_config(".zshrc"));
    }

    #[test]
    fn test_is_git_config() {
        assert!(is_git_config(".gitconfig"));
        assert!(is_git_config(".gitignore"));
        assert!(!is_git_config(".zshrc"));
    }
}

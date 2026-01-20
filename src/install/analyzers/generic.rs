//! Generic text file analyzer for detecting binary dependencies.
//!
//! This analyzer is used as a fallback for files that don't have a specialized
//! analyzer. It looks for common patterns and known binary names in the content.

use anyhow::Result;
use regex::Regex;
use std::collections::HashSet;
use std::path::Path;

/// Well-known binaries that are commonly referenced in config files
const KNOWN_BINARIES: &[&str] = &[
    // Modern CLI tools
    "fzf",
    "ripgrep",
    "rg",
    "bat",
    "fd",
    "exa",
    "eza",
    "lsd",
    "zoxide",
    "starship",
    "atuin",
    "dust",
    "duf",
    "procs",
    "bottom",
    "btm",
    "htop",
    "btop",
    "glances",
    // Shell tools
    "tmux",
    "screen",
    "zellij",
    // Text processing
    "jq",
    "yq",
    "xq",
    "fx",
    "gron",
    // Development tools
    "git",
    "gh",
    "hub",
    "glab",
    "docker",
    "podman",
    "kubectl",
    "k9s",
    "helm",
    "terraform",
    "ansible",
    "vagrant",
    // Languages and runtimes
    "python",
    "python3",
    "pip",
    "pip3",
    "pipenv",
    "poetry",
    "node",
    "npm",
    "npx",
    "yarn",
    "pnpm",
    "bun",
    "deno",
    "cargo",
    "rustc",
    "rustup",
    "go",
    "ruby",
    "gem",
    "bundle",
    "java",
    "gradle",
    "maven",
    // Editors
    "nvim",
    "vim",
    "emacs",
    "code",
    "subl",
    // Network tools
    "curl",
    "wget",
    "httpie",
    "http",
    "aria2c",
    // File tools
    "tree",
    "ncdu",
    "ranger",
    "nnn",
    "lf",
    "broot",
    // Archive tools
    "7z",
    "unrar",
    "zstd",
    // Misc utilities
    "pandoc",
    "ffmpeg",
    "imagemagick",
    "convert",
    "gpg",
    "age",
    "pass",
    "bitwarden-cli",
    "1password-cli",
    "aws",
    "gcloud",
    "az",
    "doctl",
];

/// Analyze generic text content for binary dependencies
pub fn analyze(content: &str, file_path: &Path) -> Result<HashSet<String>> {
    let mut binaries = HashSet::new();

    // Look for known binaries in meaningful contexts
    for binary in KNOWN_BINARIES {
        if is_meaningful_reference(content, binary) {
            binaries.insert(binary.to_string());
        }
    }

    // Pattern-based detection for various config file formats

    // TOML format: key = "binary" or key = 'binary'
    let toml_pattern = Regex::new(r#"=\s*['"]([a-zA-Z0-9_-]+)['"]"#)?;
    for cap in toml_pattern.captures_iter(content) {
        if let Some(value) = cap.get(1) {
            let val = value.as_str();
            if KNOWN_BINARIES.contains(&val) {
                binaries.insert(val.to_string());
            }
        }
    }

    // YAML format: key: binary or key: "binary"
    let yaml_pattern = Regex::new(r#":\s*['"]?([a-zA-Z0-9_-]+)['"]?\s*$"#)?;
    for cap in yaml_pattern.captures_iter(content) {
        if let Some(value) = cap.get(1) {
            let val = value.as_str();
            if KNOWN_BINARIES.contains(&val) {
                binaries.insert(val.to_string());
            }
        }
    }

    // JSON format: "key": "binary"
    let json_pattern = Regex::new(r#""[^"]+"\s*:\s*"([a-zA-Z0-9_-]+)""#)?;
    for cap in json_pattern.captures_iter(content) {
        if let Some(value) = cap.get(1) {
            let val = value.as_str();
            if KNOWN_BINARIES.contains(&val) {
                binaries.insert(val.to_string());
            }
        }
    }

    // Look for command patterns like: command = "binary"
    let command_pattern = Regex::new(r#"(?i)command\s*[=:]\s*['"]?([a-zA-Z0-9_-]+)"#)?;
    for cap in command_pattern.captures_iter(content) {
        if let Some(binary) = cap.get(1) {
            let bin = binary.as_str();
            if !is_config_keyword(bin) {
                binaries.insert(bin.to_string());
            }
        }
    }

    // Look for program patterns like: program = "binary"
    let program_pattern = Regex::new(r#"(?i)program\s*[=:]\s*['"]?([a-zA-Z0-9_-]+)"#)?;
    for cap in program_pattern.captures_iter(content) {
        if let Some(binary) = cap.get(1) {
            let bin = binary.as_str();
            if !is_config_keyword(bin) {
                binaries.insert(bin.to_string());
            }
        }
    }

    // Look for editor patterns like: editor = "binary" or EDITOR=binary
    let editor_pattern = Regex::new(r#"(?i)editor\s*[=:]\s*['"]?([a-zA-Z0-9_-]+)"#)?;
    for cap in editor_pattern.captures_iter(content) {
        if let Some(binary) = cap.get(1) {
            binaries.insert(binary.as_str().to_string());
        }
    }

    // Look for shell patterns like: shell = "binary" or SHELL=binary
    let shell_pattern = Regex::new(r#"(?i)shell\s*[=:]\s*['"]?([a-zA-Z0-9_-]+)"#)?;
    for cap in shell_pattern.captures_iter(content) {
        if let Some(binary) = cap.get(1) {
            let shell = binary.as_str();
            if matches!(shell, "zsh" | "bash" | "fish" | "nu" | "pwsh") {
                binaries.insert(shell.to_string());
            }
        }
    }

    // Detect based on file path/name
    add_context_based_binaries(file_path, &mut binaries);

    Ok(binaries)
}

/// Check if a binary name appears in a meaningful context (not just comments)
fn is_meaningful_reference(content: &str, binary: &str) -> bool {
    let pattern = format!(r"\b{}\b", regex::escape(binary));
    let re = match Regex::new(&pattern) {
        Ok(r) => r,
        Err(_) => return false,
    };

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip pure comments
        if trimmed.starts_with('#')
            || trimmed.starts_with("//")
            || trimmed.starts_with("--")
            || trimmed.starts_with(';')
        {
            continue;
        }

        // Check if binary appears in this line
        if re.is_match(line) {
            // Check for patterns that suggest it's a dependency
            let line_lower = line.to_lowercase();
            if line_lower.contains("requires")
                || line_lower.contains("needs")
                || line_lower.contains("depends")
                || line_lower.contains("install")
                || line_lower.contains("command")
                || line_lower.contains("program")
                || line_lower.contains("binary")
                || line_lower.contains("executable")
                || line.contains('=')
                || line.contains(':')
                || line.contains("$(")
                || line.contains("`")
            {
                return true;
            }
        }
    }

    false
}

/// Add binaries based on the file path context
fn add_context_based_binaries(file_path: &Path, binaries: &mut HashSet<String>) {
    let file_name = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    // Starship configuration
    if file_name == "starship.toml" {
        binaries.insert("starship".to_string());
    }

    // Alacritty configuration
    if file_name == "alacritty.yml" || file_name == "alacritty.toml" {
        binaries.insert("alacritty".to_string());
    }

    // Kitty configuration
    if file_name == "kitty.conf" {
        binaries.insert("kitty".to_string());
    }

    // Wezterm configuration
    if file_name == "wezterm.lua" {
        binaries.insert("wezterm".to_string());
    }

    // Tmux configuration
    if file_name == ".tmux.conf" || file_name == "tmux.conf" {
        binaries.insert("tmux".to_string());
    }

    // Zellij configuration
    if file_name == "config.kdl"
        && file_path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            == Some("zellij")
    {
        binaries.insert("zellij".to_string());
    }

    // Atuin configuration
    if file_name == "config.toml"
        && file_path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            == Some("atuin")
    {
        binaries.insert("atuin".to_string());
    }

    // Helix configuration
    if file_name == "config.toml"
        && file_path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            == Some("helix")
    {
        binaries.insert("helix".to_string());
    }

    // Zoxide
    if file_name.contains("zoxide") {
        binaries.insert("zoxide".to_string());
    }

    // Lazygit
    if file_name == "config.yml"
        && file_path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            == Some("lazygit")
    {
        binaries.insert("lazygit".to_string());
    }
}

/// Check if a string is a common config keyword (not a binary)
fn is_config_keyword(s: &str) -> bool {
    matches!(
        s.to_lowercase().as_str(),
        "true"
            | "false"
            | "yes"
            | "no"
            | "none"
            | "null"
            | "default"
            | "auto"
            | "inherit"
            | "enabled"
            | "disabled"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_toml_detection() {
        let content = r#"
[terminal]
shell = "zsh"
editor = "nvim"
"#;
        let path = PathBuf::from("/tmp/config.toml");
        let binaries = analyze(content, &path).unwrap();
        assert!(binaries.contains("zsh"));
        assert!(binaries.contains("nvim"));
    }

    #[test]
    fn test_starship_config_detection() {
        let content = r#"
[character]
success_symbol = "[➜](bold green)"
"#;
        let path = PathBuf::from("/home/user/.config/starship.toml");
        let binaries = analyze(content, &path).unwrap();
        assert!(binaries.contains("starship"));
    }

    #[test]
    fn test_meaningful_reference() {
        let content = r#"
# This uses fzf for fuzzy finding
command = fzf
"#;
        assert!(is_meaningful_reference(content, "fzf"));
    }

    #[test]
    fn test_skips_comments() {
        let content = r#"
# fzf is optional
# ripgrep can be used instead of grep
"#;
        let path = PathBuf::from("/tmp/config");
        let binaries = analyze(content, &path).unwrap();
        assert!(!binaries.contains("fzf"));
        assert!(!binaries.contains("ripgrep"));
    }
}

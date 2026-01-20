//! Git configuration analyzer for detecting binary dependencies.
//!
//! Analyzes git configuration files (.gitconfig, etc.) to find references to
//! external tools like diff programs, merge tools, and custom commands.

use anyhow::Result;
use regex::Regex;
use std::collections::HashSet;

/// Analyze git configuration content for binary dependencies
pub fn analyze(content: &str) -> Result<HashSet<String>> {
    let mut binaries = HashSet::new();

    // Pattern 1: diff.tool setting
    let diff_tool = Regex::new(r"(?i)\[diff(?:tool)?\s*[^\]]*\]\s*[^\[]*tool\s*=\s*(\w+)")?;
    for cap in diff_tool.captures_iter(content) {
        if let Some(tool) = cap.get(1) {
            let tool_name = tool.as_str();
            if let Some(binary) = map_diff_tool(tool_name) {
                binaries.insert(binary.to_string());
            }
        }
    }

    // Pattern 2: merge.tool setting
    let merge_tool = Regex::new(r"(?i)\[merge(?:tool)?\s*[^\]]*\]\s*[^\[]*tool\s*=\s*(\w+)")?;
    for cap in merge_tool.captures_iter(content) {
        if let Some(tool) = cap.get(1) {
            let tool_name = tool.as_str();
            if let Some(binary) = map_merge_tool(tool_name) {
                binaries.insert(binary.to_string());
            }
        }
    }

    // Pattern 3: core.pager setting
    let pager = Regex::new(r"(?i)pager\s*=\s*([a-zA-Z0-9_-]+)")?;
    for cap in pager.captures_iter(content) {
        if let Some(binary) = cap.get(1) {
            let pager_name = binary.as_str();
            if pager_name != "less" && pager_name != "more" {
                binaries.insert(pager_name.to_string());
            }
        }
    }

    // Pattern 4: delta - popular git diff viewer
    if content.contains("delta") {
        binaries.insert("delta".to_string());
    }

    // Pattern 5: diff-so-fancy
    if content.contains("diff-so-fancy") {
        binaries.insert("diff-so-fancy".to_string());
    }

    // Pattern 6: Git aliases with external commands
    // [alias] section with shell commands
    analyze_aliases(content, &mut binaries)?;

    // Pattern 7: credential helpers
    let credential_helper = Regex::new(r"(?i)helper\s*=\s*([a-zA-Z0-9_-]+)")?;
    for cap in credential_helper.captures_iter(content) {
        if let Some(helper) = cap.get(1) {
            let helper_name = helper.as_str();
            // Common credential helpers
            match helper_name {
                "osxkeychain" | "store" | "cache" | "manager" | "manager-core" => {}
                _ => {
                    binaries.insert(helper_name.to_string());
                }
            }
        }
    }

    // Pattern 8: core.editor
    let editor = Regex::new(r"(?i)editor\s*=\s*([a-zA-Z0-9_-]+)")?;
    for cap in editor.captures_iter(content) {
        if let Some(ed) = cap.get(1) {
            let editor_name = ed.as_str();
            // Add non-standard editors
            if !matches!(editor_name, "vi" | "vim" | "nano" | "emacs") {
                binaries.insert(editor_name.to_string());
            }
        }
    }

    // Pattern 9: commit.gpgsign and gpg program
    if content.contains("gpgsign") || content.contains("gpg.program") {
        binaries.insert("gpg".to_string());
    }

    // Pattern 10: Interactive rebase tool (sequence.editor)
    let sequence_editor = Regex::new(r"(?i)sequence\.editor\s*=\s*([a-zA-Z0-9_-]+)")?;
    for cap in sequence_editor.captures_iter(content) {
        if let Some(ed) = cap.get(1) {
            binaries.insert(ed.as_str().to_string());
        }
    }

    // Pattern 11: git-lfs
    if content.contains("[lfs]") || content.contains("lfs.") || content.contains("git-lfs") {
        binaries.insert("git-lfs".to_string());
    }

    Ok(binaries)
}

/// Analyze git aliases for external command dependencies
fn analyze_aliases(content: &str, binaries: &mut HashSet<String>) -> Result<()> {
    // Pattern for aliases that call external commands with !
    // e.g., alias = !external_command
    let alias_external = Regex::new(r"=\s*!\s*([a-zA-Z0-9_-]+)")?;
    for cap in alias_external.captures_iter(content) {
        if let Some(cmd) = cap.get(1) {
            let cmd_name = cmd.as_str();
            // Filter out common shell commands
            if !is_common_command(cmd_name) {
                binaries.insert(cmd_name.to_string());
            }
        }
    }

    // Pattern for aliases using common tools
    let known_tools = [
        "fzf", "rg", "ripgrep", "bat", "delta", "tig", "lazygit", "gh", "hub", "glab",
    ];

    for tool in known_tools {
        if content.contains(tool) {
            binaries.insert(tool.to_string());
        }
    }

    Ok(())
}

/// Map diff tool names to actual binaries
fn map_diff_tool(tool: &str) -> Option<&str> {
    match tool.to_lowercase().as_str() {
        "vimdiff" | "nvimdiff" => Some("vim"),
        "meld" => Some("meld"),
        "kdiff3" => Some("kdiff3"),
        "opendiff" => Some("opendiff"),
        "p4merge" => Some("p4merge"),
        "bc" | "bc3" | "beyondcompare" => Some("bcompare"),
        "diffmerge" => Some("diffmerge"),
        "winmerge" => Some("winmerge"),
        "araxis" => Some("araxis"),
        "delta" => Some("delta"),
        "difftastic" => Some("difft"),
        _ => None,
    }
}

/// Map merge tool names to actual binaries
fn map_merge_tool(tool: &str) -> Option<&str> {
    match tool.to_lowercase().as_str() {
        "vimdiff" | "nvimdiff" => Some("vim"),
        "meld" => Some("meld"),
        "kdiff3" => Some("kdiff3"),
        "opendiff" => Some("opendiff"),
        "p4merge" => Some("p4merge"),
        "bc" | "bc3" | "beyondcompare" => Some("bcompare"),
        "diffmerge" => Some("diffmerge"),
        "winmerge" => Some("winmerge"),
        "araxis" => Some("araxis"),
        "fugitive" => None, // This is a vim plugin, not a binary
        _ => None,
    }
}

/// Check if a command is a common system command
fn is_common_command(cmd: &str) -> bool {
    matches!(
        cmd,
        "sh"
            | "bash"
            | "zsh"
            | "echo"
            | "cat"
            | "grep"
            | "sed"
            | "awk"
            | "xargs"
            | "head"
            | "tail"
            | "sort"
            | "uniq"
            | "wc"
            | "cut"
            | "tr"
            | "tee"
            | "true"
            | "false"
            | "test"
            | "["
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delta_detection() {
        let content = r#"
[core]
    pager = delta

[interactive]
    diffFilter = delta --color-only
"#;
        let binaries = analyze(content).unwrap();
        assert!(binaries.contains("delta"));
    }

    #[test]
    fn test_diff_tool_detection() {
        let content = r#"
[diff]
    tool = meld
[difftool "meld"]
    cmd = meld "$LOCAL" "$REMOTE"
"#;
        let binaries = analyze(content).unwrap();
        assert!(binaries.contains("meld"));
    }

    #[test]
    fn test_alias_external_command() {
        let content = r#"
[alias]
    fza = !fzf --preview 'git diff'
    lg = !lazygit
"#;
        let binaries = analyze(content).unwrap();
        assert!(binaries.contains("fzf"));
        assert!(binaries.contains("lazygit"));
    }

    #[test]
    fn test_gpg_detection() {
        let content = r#"
[commit]
    gpgsign = true
[gpg]
    program = gpg2
"#;
        let binaries = analyze(content).unwrap();
        assert!(binaries.contains("gpg"));
    }

    #[test]
    fn test_git_lfs_detection() {
        let content = r#"
[filter "lfs"]
    clean = git-lfs clean -- %f
    smudge = git-lfs smudge -- %f
    process = git-lfs filter-process
    required = true
"#;
        let binaries = analyze(content).unwrap();
        assert!(binaries.contains("git-lfs"));
    }
}

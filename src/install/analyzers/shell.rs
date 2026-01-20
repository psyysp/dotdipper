//! Shell script analyzer for detecting binary dependencies.
//!
//! Analyzes shell configuration files (.zshrc, .bashrc, etc.) to find
//! references to external binaries and tools.

use anyhow::Result;
use regex::Regex;
use std::collections::HashSet;

/// Analyze shell script content for binary dependencies
pub fn analyze(content: &str) -> Result<HashSet<String>> {
    let mut binaries = HashSet::new();

    // Pattern 1: command -v <binary>
    let command_v = Regex::new(r"command\s+-v\s+([a-zA-Z0-9_-]+)")?;
    for cap in command_v.captures_iter(content) {
        if let Some(binary) = cap.get(1) {
            binaries.insert(binary.as_str().to_string());
        }
    }

    // Pattern 2: which <binary>
    let which_pattern = Regex::new(r"\bwhich\s+([a-zA-Z0-9_-]+)")?;
    for cap in which_pattern.captures_iter(content) {
        if let Some(binary) = cap.get(1) {
            binaries.insert(binary.as_str().to_string());
        }
    }

    // Pattern 3: $(which <binary>) or `which <binary>`
    let which_subshell = Regex::new(r"[\$`]\(?\s*which\s+([a-zA-Z0-9_-]+)\s*\)?")?;
    for cap in which_subshell.captures_iter(content) {
        if let Some(binary) = cap.get(1) {
            binaries.insert(binary.as_str().to_string());
        }
    }

    // Pattern 4: type <binary> (bash/zsh builtin for checking command existence)
    let type_pattern = Regex::new(r"\btype\s+([a-zA-Z0-9_-]+)")?;
    for cap in type_pattern.captures_iter(content) {
        if let Some(binary) = cap.get(1) {
            let bin_str = binary.as_str();
            if !is_shell_builtin(bin_str) {
                binaries.insert(bin_str.to_string());
            }
        }
    }

    // Pattern 5: Aliases that reference binaries
    // alias name='binary ...' or alias name="binary ..."
    let alias_pattern = Regex::new(r#"alias\s+\w+\s*=\s*['"]([a-zA-Z0-9_-]+)"#)?;
    for cap in alias_pattern.captures_iter(content) {
        if let Some(binary) = cap.get(1) {
            let bin_str = binary.as_str();
            if !is_shell_builtin(bin_str) && !is_common_shell_command(bin_str) {
                binaries.insert(bin_str.to_string());
            }
        }
    }

    // Pattern 6: eval "$(binary ...)" - common for tool initialization
    let eval_pattern = Regex::new(r#"eval\s+"\$\(([a-zA-Z0-9_-]+)"#)?;
    for cap in eval_pattern.captures_iter(content) {
        if let Some(binary) = cap.get(1) {
            binaries.insert(binary.as_str().to_string());
        }
    }

    // Pattern 7: source <(binary ...) - process substitution for sourcing
    let source_pattern = Regex::new(r"source\s+<\(([a-zA-Z0-9_-]+)")?;
    for cap in source_pattern.captures_iter(content) {
        if let Some(binary) = cap.get(1) {
            binaries.insert(binary.as_str().to_string());
        }
    }

    // Pattern 8: Common binary initialization patterns
    // e.g., [ -f ~/.fzf.zsh ] && source ~/.fzf.zsh
    let fzf_pattern = Regex::new(r"\bfzf\b")?;
    if fzf_pattern.is_match(content) {
        binaries.insert("fzf".to_string());
    }

    // Pattern 9: export PATH with binary paths (look for known tools)
    analyze_path_exports(content, &mut binaries);

    // Pattern 10: Plugin managers that install binaries
    analyze_plugin_managers(content, &mut binaries);

    // Filter out shell builtins and common commands that are always available
    binaries.retain(|b| !is_shell_builtin(b) && !is_always_available(b));

    Ok(binaries)
}

/// Analyze PATH exports for known binary locations
fn analyze_path_exports(content: &str, binaries: &mut HashSet<String>) {
    // Look for common tool-specific PATH additions
    let tool_paths = [
        ("cargo", "cargo"),
        ("rustup", "rustup"),
        ("go/bin", "go"),
        ("gopath", "go"),
        (".npm", "npm"),
        ("node", "node"),
        ("pyenv", "pyenv"),
        ("rbenv", "rbenv"),
        ("nvm", "nvm"),
    ];

    for (path_fragment, binary) in tool_paths {
        if content.to_lowercase().contains(path_fragment) {
            binaries.insert(binary.to_string());
        }
    }
}

/// Analyze plugin managers for implicit binary dependencies
fn analyze_plugin_managers(content: &str, binaries: &mut HashSet<String>) {
    // Oh My Zsh plugins often require specific binaries
    let omz_plugins = Regex::new(r"plugins\s*=\s*\([^)]+\)").ok();
    if let Some(re) = omz_plugins {
        if let Some(cap) = re.captures(content) {
            let plugins_str = cap.get(0).map(|m| m.as_str()).unwrap_or("");

            // Common plugins that require binaries
            let plugin_binaries = [
                ("fzf", "fzf"),
                ("ripgrep", "rg"),
                ("fd", "fd"),
                ("bat", "bat"),
                ("docker", "docker"),
                ("kubectl", "kubectl"),
                ("terraform", "terraform"),
                ("aws", "aws"),
                ("gcloud", "gcloud"),
            ];

            for (plugin, binary) in plugin_binaries {
                if plugins_str.contains(plugin) {
                    binaries.insert(binary.to_string());
                }
            }
        }
    }

    // Zinit/zinit plugins
    if content.contains("zinit") {
        binaries.insert("git".to_string()); // zinit requires git
    }

    // Antigen
    if content.contains("antigen") {
        binaries.insert("git".to_string());
    }
}

/// Check if a command is a shell builtin
fn is_shell_builtin(cmd: &str) -> bool {
    matches!(
        cmd,
        "echo"
            | "cd"
            | "pwd"
            | "test"
            | "["
            | "[["
            | "if"
            | "then"
            | "else"
            | "elif"
            | "fi"
            | "for"
            | "while"
            | "until"
            | "do"
            | "done"
            | "case"
            | "esac"
            | "export"
            | "source"
            | "."
            | "return"
            | "exit"
            | "true"
            | "false"
            | "eval"
            | "exec"
            | "set"
            | "unset"
            | "shift"
            | "break"
            | "continue"
            | "read"
            | "readonly"
            | "local"
            | "declare"
            | "typeset"
            | "alias"
            | "unalias"
            | "function"
            | "builtin"
            | "command"
            | "type"
            | "hash"
            | "times"
            | "trap"
            | "umask"
            | "wait"
            | "bg"
            | "fg"
            | "jobs"
            | "kill"
            | "let"
            | "printf"
            | "pushd"
            | "popd"
            | "dirs"
            | "enable"
            | "disable"
            | "help"
            | "history"
            | "logout"
            | "mapfile"
            | "readarray"
            | "shopt"
            | "caller"
            | "compgen"
            | "complete"
            | "compopt"
            | "coproc"
            | "getopts"
            | "suspend"
            | "ulimit"
    )
}

/// Check if a command is always available on Unix systems
fn is_always_available(cmd: &str) -> bool {
    matches!(
        cmd,
        "ls" | "cat"
            | "cp"
            | "mv"
            | "rm"
            | "mkdir"
            | "rmdir"
            | "chmod"
            | "chown"
            | "ln"
            | "touch"
            | "head"
            | "tail"
            | "wc"
            | "sort"
            | "uniq"
            | "cut"
            | "tr"
            | "tee"
            | "xargs"
            | "basename"
            | "dirname"
            | "date"
            | "sleep"
            | "env"
            | "uname"
            | "whoami"
            | "id"
            | "groups"
            | "df"
            | "du"
            | "free"
            | "ps"
            | "kill"
            | "nohup"
    )
}

/// Check if a command is a common shell command (but may need installation)
fn is_common_shell_command(cmd: &str) -> bool {
    matches!(
        cmd,
        "grep" | "sed" | "awk" | "find" | "tar" | "gzip" | "gunzip" | "zip" | "unzip"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_v_detection() {
        let content = r#"
if command -v fzf > /dev/null; then
    echo "fzf found"
fi
"#;
        let binaries = analyze(content).unwrap();
        assert!(binaries.contains("fzf"));
    }

    #[test]
    fn test_which_detection() {
        let content = r#"
if which rg > /dev/null 2>&1; then
    export RIPGREP=1
fi
"#;
        let binaries = analyze(content).unwrap();
        assert!(binaries.contains("rg"));
    }

    #[test]
    fn test_alias_detection() {
        let content = r#"
alias ll='exa -la'
alias cat='bat --paging=never'
"#;
        let binaries = analyze(content).unwrap();
        assert!(binaries.contains("exa"));
        assert!(binaries.contains("bat"));
    }

    #[test]
    fn test_eval_detection() {
        let content = r#"
eval "$(starship init zsh)"
eval "$(zoxide init zsh)"
"#;
        let binaries = analyze(content).unwrap();
        assert!(binaries.contains("starship"));
        assert!(binaries.contains("zoxide"));
    }

    #[test]
    fn test_filters_builtins() {
        let content = r#"
if command -v echo > /dev/null; then
    echo "test"
fi
"#;
        let binaries = analyze(content).unwrap();
        assert!(!binaries.contains("echo"));
    }

    #[test]
    fn test_fzf_pattern() {
        let content = r#"
[ -f ~/.fzf.zsh ] && source ~/.fzf.zsh
"#;
        let binaries = analyze(content).unwrap();
        assert!(binaries.contains("fzf"));
    }
}

use anyhow::{Context, Result};
use regex::Regex;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Parsed brew bundle dump / Brewfile contents.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BrewfilePlan {
    pub taps: Vec<String>,
    pub formulae: Vec<String>,
    pub casks: Vec<String>,
    pub mas: Vec<String>,
}

/// Run `brew bundle dump` and return the Brewfile text from stdout.
pub fn dump_brewfile() -> Result<String> {
    let brew = find_brew()?;
    let output = Command::new(&brew)
        .args(["bundle", "dump", "--file=-", "--force", "--no-vscode"])
        .output()
        .context("Failed to run `brew bundle dump`")?;

    if !output.status.success() {
        anyhow::bail!(
            "`brew bundle dump` failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// List installed Homebrew cask tokens via `brew list --cask`.
pub fn list_installed_casks() -> Result<Vec<String>> {
    let brew = find_brew()?;
    let output = Command::new(&brew)
        .args(["list", "--cask"])
        .output()
        .context("Failed to run `brew list --cask`")?;

    if !output.status.success() {
        anyhow::bail!(
            "`brew list --cask` failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(parse_cask_list(&String::from_utf8_lossy(&output.stdout)))
}

/// Restore packages from a Brewfile, streaming brew output live.
pub fn bundle_install(brewfile: &Path) -> Result<()> {
    let brew = find_brew()?;
    let file_arg = format!("--file={}", brewfile.display());
    let status = Command::new(&brew)
        .args(["bundle", &file_arg])
        .status()
        .context("Failed to run `brew bundle`")?;

    if !status.success() {
        anyhow::bail!("`brew bundle` failed with exit code {:?}", status.code());
    }

    Ok(())
}

pub fn find_brew() -> Result<PathBuf> {
    if let Ok(path) = which::which("brew") {
        return Ok(path);
    }
    for candidate in ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"] {
        let path = PathBuf::from(candidate);
        if path.is_file() {
            return Ok(path);
        }
    }
    anyhow::bail!(
        "Homebrew (brew) was not found. Install it from https://brew.sh and ensure it is on your PATH."
    );
}

/// Split `brew list --cask` output into individual cask tokens.
pub fn parse_cask_list(output: &str) -> Vec<String> {
    output
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .map(|token| token.to_string())
        .collect()
}

/// Parse taps, formulae, casks, and MAS entries from Brewfile content.
pub fn parse_brewfile(content: &str) -> BrewfilePlan {
    let mut plan = BrewfilePlan::default();
    let tap_re = line_name_regex("tap");
    let brew_re = line_name_regex("brew");
    let cask_re = line_name_regex("cask");
    let mas_re = Regex::new(r#"(?m)^\s*mas\s+["']([^"']+)["']"#).expect("valid mas regex");

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(caps) = tap_re.captures(line) {
            plan.taps.push(caps[1].to_string());
        } else if let Some(caps) = brew_re.captures(line) {
            plan.formulae.push(caps[1].to_string());
        } else if let Some(caps) = cask_re.captures(line) {
            plan.casks.push(caps[1].to_string());
        } else if let Some(caps) = mas_re.captures(line) {
            plan.mas.push(caps[1].to_string());
        }
    }

    plan
}

fn line_name_regex(kind: &str) -> Regex {
    Regex::new(&format!(r#"(?m)^\s*{}\s+["']([^"']+)["']"#, kind)).expect("valid brewfile regex")
}

/// True when the Brewfile already installs the `mas` formula.
pub fn brewfile_has_mas_formula(brewfile: &str) -> bool {
    Regex::new(r#"(?m)^\s*brew\s+["']mas["']"#)
        .expect("valid mas formula regex")
        .is_match(brewfile)
}

/// True when the Brewfile contains `mas "..."` app entries.
pub fn brewfile_has_mas_apps(brewfile: &str) -> bool {
    Regex::new(r#"(?m)^\s*mas\s+["']"#)
        .expect("valid mas app regex")
        .is_match(brewfile)
}

/// Prepend `brew "mas"` when the Brewfile does not already include it.
pub fn ensure_mas_formula(brewfile: &str) -> String {
    if brewfile_has_mas_formula(brewfile) {
        brewfile.to_string()
    } else if brewfile.is_empty() {
        "brew \"mas\"\n".to_string()
    } else {
        format!("brew \"mas\"\n{}", brewfile)
    }
}

/// Case-insensitive / hyphen-normalized match between an app name and a cask token.
pub fn cask_matches_app(app_name: &str, cask_name: &str) -> bool {
    let app = normalize_token(app_name);
    let cask = normalize_token(cask_name);
    if app.is_empty() || cask.is_empty() {
        return false;
    }
    app == cask
}

pub fn normalize_token(value: &str) -> String {
    let stripped = value
        .trim()
        .trim_end_matches(".app")
        .trim_end_matches(".APP");
    let mut out = String::new();
    let mut last_dash = false;
    for ch in stripped.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cask_list_splits_whitespace() {
        let names = parse_cask_list("kitty\nvisual-studio-code  firefox\n");
        assert_eq!(
            names,
            vec![
                "kitty".to_string(),
                "visual-studio-code".to_string(),
                "firefox".to_string()
            ]
        );
    }

    #[test]
    fn parse_brewfile_extracts_entries() {
        let content = r#"
# header
tap "homebrew/core"
tap 'homebrew/cask'
brew "git"
brew "wget", args: ["with-foo"]
cask "kitty"
cask "visual-studio-code"
mas "Xcode", id: 497799835
mas "Amphetamine", id: 937984704
"#;
        let plan = parse_brewfile(content);
        assert_eq!(
            plan.taps,
            vec!["homebrew/core".to_string(), "homebrew/cask".to_string()]
        );
        assert_eq!(plan.formulae, vec!["git".to_string(), "wget".to_string()]);
        assert_eq!(
            plan.casks,
            vec!["kitty".to_string(), "visual-studio-code".to_string()]
        );
        assert_eq!(
            plan.mas,
            vec!["Xcode".to_string(), "Amphetamine".to_string()]
        );
    }

    #[test]
    fn brewfile_has_mas_formula_ignores_mas_app_lines() {
        assert!(brewfile_has_mas_formula("brew \"mas\"\n"));
        assert!(brewfile_has_mas_formula("brew 'mas', link: true\n"));
        assert!(!brewfile_has_mas_formula("brew \"mas-cli\"\n"));
        assert!(!brewfile_has_mas_formula("mas \"Xcode\", id: 497799835\n"));
        assert!(!brewfile_has_mas_formula("cask \"mas\"\n"));
    }

    #[test]
    fn brewfile_has_mas_apps_detects_mas_lines() {
        assert!(brewfile_has_mas_apps("mas \"Xcode\", id: 497799835\n"));
        assert!(!brewfile_has_mas_apps("brew \"mas\"\n"));
    }

    #[test]
    fn ensure_mas_formula_prepends_when_missing() {
        let original = "brew \"git\"\nmas \"Xcode\", id: 497799835\n";
        let updated = ensure_mas_formula(original);
        assert!(updated.starts_with("brew \"mas\"\n"));
        assert!(updated.contains("brew \"git\""));
        assert_eq!(ensure_mas_formula(&updated), updated);
    }

    #[test]
    fn cask_matches_app_normalizes_names() {
        assert!(cask_matches_app("Kitty", "kitty"));
        assert!(cask_matches_app("Visual Studio Code", "visual-studio-code"));
        assert!(cask_matches_app("iTerm2.app", "iterm2"));
        assert!(!cask_matches_app("Slack", "discord"));
        assert!(!cask_matches_app("", "kitty"));
    }
}

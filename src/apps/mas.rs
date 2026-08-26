use anyhow::{Context, Result};
use regex::Regex;
use std::path::PathBuf;
use std::process::Command;

use super::MasApp;

/// Return installed Mac App Store apps, or an empty list when `mas` is absent.
pub fn list_installed() -> Result<Vec<MasApp>> {
    let Some(mas) = find_mas() else {
        return Ok(Vec::new());
    };

    let output = Command::new(&mas)
        .arg("list")
        .output()
        .with_context(|| format!("Failed to run `{} list`", mas.display()))?;

    if !output.status.success() {
        crate::ui::warn(&format!(
            "`mas list` failed ({}); skipping Mac App Store apps",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
        return Ok(Vec::new());
    }

    Ok(parse_mas_list(&String::from_utf8_lossy(&output.stdout)))
}

fn find_mas() -> Option<PathBuf> {
    if let Ok(path) = which::which("mas") {
        return Some(path);
    }
    for candidate in ["/opt/homebrew/bin/mas", "/usr/local/bin/mas"] {
        let path = PathBuf::from(candidate);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

/// Parse `mas list` output (`"497799835  Xcode  (15.0)"`) into structured apps.
pub fn parse_mas_list(output: &str) -> Vec<MasApp> {
    let re = Regex::new(r"^(\d+)\s+(.+)\s+\(([^)]+)\)\s*$").expect("valid mas list regex");
    let mut apps = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some(caps) = re.captures(line) else {
            continue;
        };
        let Ok(id) = caps[1].parse::<u64>() else {
            continue;
        };
        let name = caps[2].trim().to_string();
        let version = caps[3].trim().to_string();
        if name.is_empty() {
            continue;
        }
        apps.push(MasApp { id, name, version });
    }

    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mas_list_typical_lines() {
        let output = "\
497799835  Xcode  (15.0)
408981434  iMovie  (10.3.5)
937984704 Amphetamine (5.3)
";
        let apps = parse_mas_list(output);
        assert_eq!(apps.len(), 3);
        assert_eq!(apps[0].name, "Amphetamine");
        assert_eq!(apps[0].id, 937984704);
        assert_eq!(apps[1].name, "iMovie");
        assert_eq!(apps[1].version, "10.3.5");
        assert_eq!(apps[2].name, "Xcode");
        assert_eq!(apps[2].id, 497799835);
        assert_eq!(apps[2].version, "15.0");
    }

    #[test]
    fn parse_mas_list_skips_malformed_and_empty() {
        let output = "\nnot-a-line\n12345 NoVersion\n";
        assert!(parse_mas_list(output).is_empty());
    }

    #[test]
    fn parse_mas_list_keeps_names_with_spaces() {
        let apps = parse_mas_list("409201541  Pages  (13.2)\n");
        assert_eq!(apps[0].name, "Pages");
        let apps = parse_mas_list("497799835  Visual Studio  (17.0)\n");
        assert_eq!(apps[0].name, "Visual Studio");
        assert_eq!(apps[0].version, "17.0");
    }
}

//! Captures macOS system preferences as a replayable `defaults` script.
//!
//! The obvious implementation — copy `~/Library/Preferences/*.plist` into the
//! store — is the one that must not be used. A Finder plist on this machine
//! holds `FXRecentFolders` naming private projects, and `NewWindowTargetPath`
//! carrying an absolute home path; a Dock plist holds `persistent-apps`, a
//! nested structure of file URLs. Those are not incidental: recording where
//! someone has been is what those keys are *for*.
//!
//! So this module never reads a plist. It reads an explicit allowlist of
//! individual keys, and two structural rules keep it honest:
//!
//! 1. Nothing is captured for merely being set. A key absent from
//!    [`ALLOWLIST`] is never read, so a preference a future macOS invents
//!    cannot appear in the output by default.
//! 2. Scalars only, and a value that looks like a path or a URL is refused
//!    even when its key is allowlisted. The first rule is the policy; the
//!    second is what makes a careless addition to the policy fail safe.
//!
//! The pinned Dock lineup (`persistent-apps`) is deliberately absent. It is
//! genuinely useful on a new machine and it is also a nested array of paths,
//! which is precisely the shape that leaked last time. Rebuilding a Dock by
//! hand once is the cheaper side of that trade.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// One preference worth carrying to another machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Setting {
    pub domain: &'static str,
    pub key: &'static str,
}

const fn s(domain: &'static str, key: &'static str) -> Setting {
    Setting { domain, key }
}

/// Global domain, spelled the way `defaults` expects it in a script.
const GLOBAL: &str = "NSGlobalDomain";

/// Every preference this tool will read. Additions are a deliberate act.
pub const ALLOWLIST: &[Setting] = &[
    // Dock behaviour.
    s("com.apple.dock", "autohide"),
    s("com.apple.dock", "autohide-delay"),
    s("com.apple.dock", "autohide-time-modifier"),
    s("com.apple.dock", "magnification"),
    s("com.apple.dock", "tilesize"),
    s("com.apple.dock", "largesize"),
    s("com.apple.dock", "mineffect"),
    s("com.apple.dock", "minimize-to-application"),
    s("com.apple.dock", "orientation"),
    s("com.apple.dock", "mru-spaces"),
    s("com.apple.dock", "show-recents"),
    s("com.apple.dock", "show-process-indicators"),
    s("com.apple.dock", "showAppExposeGestureEnabled"),
    s("com.apple.dock", "expose-group-apps"),
    s("com.apple.dock", "launchanim"),
    // Hot corners. The modifiers matter as much as the corners: capture one
    // without the other and the corner restores to the wrong gesture.
    s("com.apple.dock", "wvous-tl-corner"),
    s("com.apple.dock", "wvous-tl-modifier"),
    s("com.apple.dock", "wvous-tr-corner"),
    s("com.apple.dock", "wvous-tr-modifier"),
    s("com.apple.dock", "wvous-bl-corner"),
    s("com.apple.dock", "wvous-bl-modifier"),
    s("com.apple.dock", "wvous-br-corner"),
    s("com.apple.dock", "wvous-br-modifier"),
    // Finder. Note the absence of FXRecentFolders and NewWindowTargetPath.
    s("com.apple.finder", "AppleShowAllFiles"),
    s("com.apple.finder", "ShowPathbar"),
    s("com.apple.finder", "ShowStatusBar"),
    s("com.apple.finder", "FXPreferredViewStyle"),
    s("com.apple.finder", "FXDefaultSearchScope"),
    s("com.apple.finder", "FXEnableExtensionChangeWarning"),
    s("com.apple.finder", "_FXSortFoldersFirst"),
    s("com.apple.finder", "ShowHardDrivesOnDesktop"),
    s("com.apple.finder", "ShowExternalHardDrivesOnDesktop"),
    s("com.apple.finder", "ShowRemovableMediaOnDesktop"),
    // Keyboard, text substitution, and interface.
    s(GLOBAL, "KeyRepeat"),
    s(GLOBAL, "InitialKeyRepeat"),
    s(GLOBAL, "ApplePressAndHoldEnabled"),
    s(GLOBAL, "AppleShowAllExtensions"),
    s(GLOBAL, "AppleInterfaceStyle"),
    s(GLOBAL, "AppleShowScrollBars"),
    s(GLOBAL, "NSAutomaticSpellingCorrectionEnabled"),
    s(GLOBAL, "NSAutomaticCapitalizationEnabled"),
    s(GLOBAL, "NSAutomaticDashSubstitutionEnabled"),
    s(GLOBAL, "NSAutomaticQuoteSubstitutionEnabled"),
    s(GLOBAL, "NSAutomaticPeriodSubstitutionEnabled"),
    s(GLOBAL, "NSNavPanelExpandedStateForSaveMode"),
    s(GLOBAL, "PMPrintingExpandedStateForPrint"),
    s(GLOBAL, "com.apple.swipescrolldirection"),
    s(GLOBAL, "com.apple.springing.enabled"),
    s(GLOBAL, "com.apple.mouse.scaling"),
    s(GLOBAL, "com.apple.trackpad.scaling"),
    // Trackpad. Two domains, because macOS keeps built-in and Bluetooth
    // trackpads separately and a Mac may have either.
    s("com.apple.AppleMultitouchTrackpad", "Clicking"),
    s(
        "com.apple.AppleMultitouchTrackpad",
        "TrackpadThreeFingerDrag",
    ),
    s("com.apple.AppleMultitouchTrackpad", "ActuationStrength"),
    s("com.apple.AppleMultitouchTrackpad", "FirstClickThreshold"),
    s("com.apple.AppleMultitouchTrackpad", "SecondClickThreshold"),
    s(
        "com.apple.driver.AppleBluetoothMultitouch.trackpad",
        "Clicking",
    ),
    s(
        "com.apple.driver.AppleBluetoothMultitouch.trackpad",
        "TrackpadThreeFingerDrag",
    ),
    // Screenshots. `location` is excluded: it is a path.
    s("com.apple.screencapture", "type"),
    s("com.apple.screencapture", "disable-shadow"),
    s("com.apple.screencapture", "show-thumbnail"),
    // Windows, spaces, accessibility.
    s("com.apple.WindowManager", "GloballyEnabled"),
    s(
        "com.apple.WindowManager",
        "EnableStandardClickToShowDesktop",
    ),
    s("com.apple.WindowManager", "StandardHideDesktopIcons"),
    s("com.apple.spaces", "spans-displays"),
    s("com.apple.universalaccess", "reduceMotion"),
    s("com.apple.universalaccess", "reduceTransparency"),
];

/// A scalar preference value, in the four shapes `defaults write` accepts.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Float(String),
    Str(String),
}

impl Value {
    /// The `defaults write` flag and literal for this value.
    fn write_args(&self) -> (&'static str, String) {
        match self {
            Value::Bool(b) => ("-bool", if *b { "true".into() } else { "false".into() }),
            Value::Int(i) => ("-int", i.to_string()),
            Value::Float(f) => ("-float", f.clone()),
            Value::Str(v) => (
                "-string",
                format!("\"{}\"", v.replace('\\', r"\\").replace('"', "\\\"")),
            ),
        }
    }
}

/// True when a value must not be recorded regardless of its key.
///
/// The allowlist is the policy; this is the backstop that makes a careless
/// addition to it fail safe rather than leak. A preference whose value is a
/// filesystem path or a URL describes where its owner works, which is the
/// class of thing this module exists to keep out.
pub fn is_unsafe_value(raw: &str) -> bool {
    let v = raw.trim();
    v.starts_with('/')
        || v.starts_with("~/")
        || v.contains("://")
        || v.contains("/Users/")
        || v.contains("/Volumes/")
        || v.contains("/home/")
}

/// Interprets `defaults read-type` output plus a raw value.
///
/// Returns `None` for anything that is not a scalar — an array or a
/// dictionary is exactly the nested, path-bearing shape this module refuses.
pub fn parse_value(type_line: &str, raw: &str) -> Option<Value> {
    let raw = raw.trim();
    if raw.is_empty() || is_unsafe_value(raw) {
        return None;
    }
    // "Type is boolean" and friends.
    let kind = type_line.trim().rsplit(' ').next()?;
    match kind {
        "boolean" => match raw {
            "1" | "true" | "YES" => Some(Value::Bool(true)),
            "0" | "false" | "NO" => Some(Value::Bool(false)),
            _ => None,
        },
        "integer" => raw.parse::<i64>().ok().map(Value::Int),
        "float" => {
            // Kept as text so 0.4 does not come back as 0.40000000000000002.
            raw.parse::<f64>().ok()?;
            Some(Value::Float(raw.to_string()))
        }
        // Multi-line strings would break a one-line `defaults write`, and no
        // preference worth carrying has one.
        "string" if !raw.contains('\n') => Some(Value::Str(raw.to_string())),
        _ => None,
    }
}

/// Renders captured settings as an idempotent shell script.
pub fn render(captured: &[(Setting, Value)]) -> String {
    let mut out = String::from(HEADER);

    let mut current_domain = "";
    for (setting, value) in captured {
        if setting.domain != current_domain {
            out.push_str(&format!("\n# {}\n", setting.domain));
            current_domain = setting.domain;
        }
        let (flag, literal) = value.write_args();
        out.push_str(&format!(
            "defaults write {} \"{}\" {} {}\n",
            setting.domain, setting.key, flag, literal
        ));
    }

    let mut restart: Vec<&str> = Vec::new();
    for (setting, _) in captured {
        let app = match setting.domain {
            "com.apple.dock" | "com.apple.spaces" | "com.apple.WindowManager" => "Dock",
            "com.apple.finder" => "Finder",
            _ => continue,
        };
        if !restart.contains(&app) {
            restart.push(app);
        }
    }
    if !restart.is_empty() {
        out.push_str("\n# Restart the affected services so the changes take effect.\n");
        for app in &restart {
            out.push_str(&format!("killall {} 2>/dev/null || true\n", app));
        }
    }

    out.push_str(TRAILER);
    out
}

const HEADER: &str = r#"#!/usr/bin/env bash
#
# macOS preferences, captured by `dotdipper macos capture`.
# Generated file — re-run the command rather than editing it.
#
# Only an explicit allowlist of individual keys is captured, and only scalar
# values. No preference plist is ever copied: Finder and Dock plists carry
# recent-folder names, absolute home paths, and the pinned app lineup, none
# of which belong in a dotfiles repository.
#
# Re-running is safe; every line is idempotent.

set -uo pipefail

if [ "$(uname -s)" != "Darwin" ]; then
  echo "macos defaults: not macOS, nothing to do." >&2
  exit 0
fi
"#;

const TRAILER: &str = r#"
# Trackpad and keyboard changes may need a log out and back in to fully apply.
echo "macOS preferences applied."
"#;

/// Reads one setting from the live system. `None` when unset or unsafe.
fn read_setting(setting: &Setting) -> Option<Value> {
    let type_out = Command::new("defaults")
        .args(["read-type", setting.domain, setting.key])
        .output()
        .ok()?;
    if !type_out.status.success() {
        return None;
    }
    let value_out = Command::new("defaults")
        .args(["read", setting.domain, setting.key])
        .output()
        .ok()?;
    if !value_out.status.success() {
        return None;
    }
    parse_value(
        &String::from_utf8_lossy(&type_out.stdout),
        &String::from_utf8_lossy(&value_out.stdout),
    )
}

/// Where the generated script lives.
pub fn script_path() -> Result<PathBuf> {
    Ok(dirs::home_dir()
        .context("Cannot resolve the home directory")?
        .join(".config/macos/defaults.sh"))
}

pub struct CaptureResult {
    pub path: PathBuf,
    pub captured: usize,
    pub skipped_unset: usize,
    pub refused: Vec<String>,
}

/// Reads every allowlisted key and writes the script.
pub fn capture(dest: Option<&Path>) -> Result<CaptureResult> {
    let mut captured = Vec::new();
    let mut skipped_unset = 0usize;
    let mut refused = Vec::new();

    for setting in ALLOWLIST {
        match read_setting(setting) {
            Some(value) => captured.push((*setting, value)),
            None => {
                // Distinguish "not set on this machine" from "set, but this
                // module will not record it" — the second is worth saying.
                let exists = Command::new("defaults")
                    .args(["read", setting.domain, setting.key])
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false);
                if exists {
                    refused.push(format!("{} {}", setting.domain, setting.key));
                } else {
                    skipped_unset += 1;
                }
            }
        }
    }

    let path = match dest {
        Some(p) => p.to_path_buf(),
        None => script_path()?,
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    std::fs::write(&path, render(&captured))
        .with_context(|| format!("Failed to write {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms)?;
    }

    Ok(CaptureResult {
        path,
        captured: captured.len(),
        skipped_unset,
        refused,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_allowlist_excludes_every_key_known_to_carry_a_path_or_history() {
        // Named explicitly, because the cost of one of these reappearing is
        // a leak rather than a bug. FXRecentFolders on the author's machine
        // names private project directories.
        for banned in [
            "FXRecentFolders",
            "NewWindowTargetPath",
            "persistent-apps",
            "persistent-others",
            "recent-apps",
            "location",
            "FXDesktopVolumePositions",
            "GoToField",
            "SGTRecentFileSearches",
        ] {
            assert!(
                !ALLOWLIST.iter().any(|s| s.key == banned),
                "{banned} must never be in the allowlist"
            );
        }
    }

    #[test]
    fn nested_values_are_refused_because_that_is_where_paths_hide() {
        assert_eq!(parse_value("Type is array", "( a, b )"), None);
        assert_eq!(parse_value("Type is dictionary", "{ a = 1; }"), None);
        assert_eq!(parse_value("Type is data", "<deadbeef>"), None);
    }

    #[test]
    fn a_path_valued_setting_is_refused_even_when_its_key_is_allowlisted() {
        // The backstop: the allowlist is the policy, this is what makes a
        // careless addition to the policy fail safe instead of leaking.
        assert_eq!(
            parse_value("Type is string", "/Users/someone/Desktop"),
            None
        );
        assert_eq!(
            parse_value("Type is string", "file:///Users/someone/Downloads/"),
            None
        );
        assert_eq!(parse_value("Type is string", "~/Documents"), None);
        assert_eq!(parse_value("Type is string", "/Volumes/Backup"), None);
        // And a legitimate scalar still gets through.
        assert_eq!(
            parse_value("Type is string", "genie"),
            Some(Value::Str("genie".into()))
        );
    }

    #[test]
    fn scalars_round_trip_into_the_right_defaults_flag() {
        assert_eq!(parse_value("Type is boolean", "1"), Some(Value::Bool(true)));
        assert_eq!(
            parse_value("Type is boolean", "0"),
            Some(Value::Bool(false))
        );
        assert_eq!(parse_value("Type is integer", "62"), Some(Value::Int(62)));
        // Kept as text: 0.4 must not come back as 0.40000000000000002.
        assert_eq!(
            parse_value("Type is float", "0.4"),
            Some(Value::Float("0.4".into()))
        );

        let script = render(&[
            (s("com.apple.dock", "autohide"), Value::Bool(true)),
            (s("com.apple.dock", "tilesize"), Value::Float("62".into())),
            (s("com.apple.dock", "mineffect"), Value::Str("genie".into())),
        ]);
        assert!(script.contains(r#"defaults write com.apple.dock "autohide" -bool true"#));
        assert!(script.contains(r#"defaults write com.apple.dock "tilesize" -float 62"#));
        assert!(script.contains(r#"defaults write com.apple.dock "mineffect" -string "genie""#));
    }

    #[test]
    fn a_string_value_cannot_break_out_of_its_quotes() {
        let script = render(&[(
            s("com.apple.dock", "mineffect"),
            Value::Str("a\" ; rm -rf / ; echo \"b".into()),
        )]);
        assert!(
            script.contains(r#"-string "a\" ; rm -rf / ; echo \"b""#),
            "unescaped quote in: {script}"
        );
    }

    #[test]
    fn only_the_services_that_were_touched_get_restarted() {
        let dock_only = render(&[(s("com.apple.dock", "autohide"), Value::Bool(true))]);
        assert!(dock_only.contains("killall Dock"));
        assert!(!dock_only.contains("killall Finder"));

        let keyboard_only = render(&[(s(GLOBAL, "KeyRepeat"), Value::Int(2))]);
        assert!(!keyboard_only.contains("killall"));
    }

    #[test]
    fn hot_corners_carry_their_modifiers() {
        // A corner without its modifier restores to the wrong gesture.
        for corner in ["tl", "tr", "bl", "br"] {
            let has_corner = ALLOWLIST
                .iter()
                .any(|s| s.key == format!("wvous-{corner}-corner"));
            let has_modifier = ALLOWLIST
                .iter()
                .any(|s| s.key == format!("wvous-{corner}-modifier"));
            assert!(has_corner && has_modifier, "wvous-{corner} is incomplete");
        }
    }

    #[test]
    fn the_generated_script_is_valid_bash() {
        let script = render(&[
            (s("com.apple.dock", "autohide"), Value::Bool(true)),
            (s("com.apple.finder", "ShowPathbar"), Value::Bool(true)),
            (s(GLOBAL, "KeyRepeat"), Value::Int(2)),
        ]);
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("defaults.sh");
        std::fs::write(&path, &script).unwrap();
        let out = std::process::Command::new("bash")
            .arg("-n")
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "not valid bash: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

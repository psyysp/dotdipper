//! Synthesises a tool-installation script from the captured app inventory.
//!
//! dotdipper exists so a second machine can be brought up with the same
//! tools. The captured `Brewfile` and `apps_manifest.toml` serve that
//! purpose, but they carry a good deal more than it needs: the machine's
//! name, when the capture ran, the exact version of everything installed,
//! and the applications that were installed by hand and cannot be installed
//! from a command line at all.
//!
//! None of that helps a new machine. Two parts of it actively hurt when the
//! mirror is public. A version inventory is a vulnerability inventory: it
//! tells a reader precisely which outdated builds to look up. And the
//! hand-installed list describes its owner rather than their toolchain — a
//! corporate VPN client names an employer's network vendor, and the rest is
//! taste, not tooling.
//!
//! What remains after removing all of that is the thing the user actually
//! asked for: a script that installs the tools.

use anyhow::{Context, Result};
use regex::Regex;
use serde::Deserialize;

/// The half of `apps_manifest.toml` that describes installable software.
///
/// Deliberately narrow. The manifest also records the machine's name, the
/// capture time, the installed version of everything, and applications
/// installed by hand — and none of those have a field here to land in.
/// Unknown keys are ignored, so the type is itself the filter: nothing
/// reaches the generated script without a field added on purpose.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct AppsInventory {
    #[serde(default)]
    pub mas: Vec<MasEntry>,
    #[serde(default)]
    pub casks: Vec<CaskEntry>,
}

/// A Mac App Store title. No version field, by design.
#[derive(Debug, Clone, Deserialize)]
pub struct MasEntry {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CaskEntry {
    pub name: String,
}

impl AppsInventory {
    pub fn parse(text: &str) -> Result<Self> {
        toml::from_str(text).context("Failed to parse apps manifest")
    }
}

/// The parts of a Brewfile that install something.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BrewfilePlan {
    pub taps: Vec<String>,
    pub formulae: Vec<String>,
    pub casks: Vec<String>,
    pub mas: Vec<String>,
    /// App Store ids from `mas "Name", id: 12345` lines. The names alone
    /// cannot install anything, so without these a store that captured a
    /// Brewfile but no manifest lost its App Store entries entirely.
    pub mas_ids: Vec<u64>,
}

/// Extracts tap, formula, cask, and App Store entries from a Brewfile.
///
/// Lives here rather than beside the rest of the Homebrew code because that
/// module is macOS-only, and a Linux machine still has to be able to publish
/// a mirror of a store captured on a Mac.
pub fn parse_brewfile(content: &str) -> BrewfilePlan {
    let mut plan = BrewfilePlan::default();
    let tap_re = line_name_regex("tap");
    let brew_re = line_name_regex("brew");
    let cask_re = line_name_regex("cask");
    let mas_re = Regex::new(r#"(?m)^\s*mas\s+["']([^"']+)["']"#).expect("valid mas regex");
    let mas_id_re = Regex::new(r"\bid:\s*(\d+)").expect("valid mas id regex");

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
            if let Some(id) = mas_id_re
                .captures(line)
                .and_then(|c| c[1].parse::<u64>().ok())
            {
                plan.mas_ids.push(id);
            }
        }
    }

    plan
}

fn line_name_regex(kind: &str) -> Regex {
    Regex::new(&format!(r#"(?m)^\s*{}\s+["']([^"']+)["']"#, kind)).expect("valid brewfile regex")
}

/// The inventory a script is generated from. Both halves are optional so a
/// store that captured only one of them still produces a usable script.
#[derive(Debug, Default, Clone, Copy)]
pub struct Inventory<'a> {
    pub brewfile: Option<&'a str>,
    pub apps: Option<&'a AppsInventory>,
}

/// One installable item, resolved from the inventory.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub taps: Vec<String>,
    pub formulae: Vec<String>,
    pub casks: Vec<String>,
    /// Mac App Store ids. Ids only: the script installs by id, and a list
    /// of purchase titles identifies its owner.
    pub mas: Vec<u64>,
    /// Entries dropped because they sit under an omitted namespace.
    pub omitted: usize,
}

/// Resolves the inventory into a deduplicated, sorted plan.
///
/// `omit_namespaces` drops taps owned by those accounts. A personal tap is
/// the one Homebrew entry that names its owner, so in a public mirror it
/// would either disclose the identity or — once the redactor reaches it —
/// be rewritten into a tap that does not exist. Dropping it keeps the
/// script honest and runnable.
pub fn plan(inventory: &Inventory<'_>, omit_namespaces: &[String]) -> Plan {
    let mut plan = Plan::default();
    let mut omitted = 0usize;

    {
        // Applies to taps (`owner/tap`) and to fully-qualified packages
        // (`owner/tap/formula`) alike: in both, the first segment is the owner.
        let mut keep = |name: &str| {
            let namespace = name.split('/').next().unwrap_or_default();
            let owned = name.contains('/')
                && omit_namespaces
                    .iter()
                    .any(|n| n.eq_ignore_ascii_case(namespace));
            if owned {
                omitted += 1;
            }
            !owned
        };

        if let Some(brewfile) = inventory.brewfile {
            let parsed = parse_brewfile(brewfile);
            plan.taps
                .extend(parsed.taps.into_iter().filter(|t| keep(t)));
            plan.formulae
                .extend(parsed.formulae.into_iter().filter(|f| keep(f)));
            plan.casks
                .extend(parsed.casks.into_iter().filter(|c| keep(c)));
            plan.mas.extend(parsed.mas_ids);
        }

        if let Some(apps) = inventory.apps {
            plan.casks.extend(
                apps.casks
                    .iter()
                    .map(|c| c.name.clone())
                    .filter(|c| keep(c)),
            );
            plan.mas.extend(apps.mas.iter().map(|m| m.id));
        }
    }

    plan.omitted = omitted;

    dedup_sorted(&mut plan.taps);
    dedup_sorted(&mut plan.formulae);
    dedup_sorted(&mut plan.casks);
    plan.mas.sort_unstable();
    plan.mas.dedup();

    plan
}

fn dedup_sorted(items: &mut Vec<String>) {
    items.sort();
    items.dedup();
}

/// Renders the plan as a standalone, idempotent bash script.
pub fn render(plan: &Plan) -> String {
    let mut s = String::new();
    s.push_str(HEADER);

    if plan.omitted > 0 {
        s.push_str(&format!(
            "# {} {} from a personal tap {} omitted: a tap, and the packages\n\
             # under it, name their owner, and this script is written to be\n\
             # shareable. Install those from your own machine instead.\n\n",
            plan.omitted,
            if plan.omitted == 1 {
                "entry"
            } else {
                "entries"
            },
            if plan.omitted == 1 { "was" } else { "were" },
        ));
    }

    s.push_str(&array("TAPS", plan.taps.iter().cloned()));
    s.push_str(&array("FORMULAE", plan.formulae.iter().cloned()));
    s.push_str(&array("CASKS", plan.casks.iter().cloned()));
    s.push_str(&array("MAS_IDS", plan.mas.iter().map(|id| id.to_string())));

    if !plan.mas.is_empty() {
        // Deliberately ids only. The script installs by id, so the titles
        // were decoration — and a list of App Store purchases names its
        // owner exactly the way the hand-installed applications this
        // feature already drops do.
        s.push_str(&format!(
            "# {} Mac App Store title(s), listed by id. `mas info <id>` names one.\n\n",
            plan.mas.len()
        ));
    }

    s.push_str(BODY);
    s
}

/// Convenience wrapper: resolve and render in one step.
pub fn generate(inventory: &Inventory<'_>, omit_namespaces: &[String]) -> String {
    render(&plan(inventory, omit_namespaces))
}

/// Emits a bash array. Single-quoted with the standard `'\''` escape, so a
/// name containing a quote cannot break out of the literal.
fn array(name: &str, items: impl Iterator<Item = String>) -> String {
    let mut s = format!("{name}=(\n");
    let mut empty = true;
    for item in items {
        empty = false;
        s.push_str(&format!("  '{}'\n", item.replace('\'', r"'\''")));
    }
    s.push_str(")\n");
    if empty {
        s = format!("{name}=()\n");
    }
    s.push('\n');
    s
}

const HEADER: &str = r#"#!/usr/bin/env bash
#
# Generated by dotdipper. Installs the tools this configuration expects.
#
# Synthesised from a Homebrew bundle and a Mac App Store inventory. It does
# not reproduce installed versions, the machine it was captured on, when it
# was captured, or applications installed by hand — none of that helps a new
# machine install tools, and a version list in particular tells a reader
# exactly which outdated builds to look up.
#
# Every step is idempotent, so re-running it is safe. A package that fails
# does not abort the run; failures are collected and reported at the end.

set -uo pipefail

log()  { printf '\033[0;32m[dotdipper]\033[0m %s\n' "$1"; }
warn() { printf '\033[1;33m[dotdipper]\033[0m %s\n' "$1" >&2; }
die()  { printf '\033[0;31m[dotdipper]\033[0m %s\n' "$1" >&2; exit 1; }

[[ "$(uname -s)" == "Darwin" ]] || die "This script targets macOS."

FAILED=()

"#;

const BODY: &str = r#"if ! command -v brew >/dev/null 2>&1; then
  log "Installing Homebrew..."
  /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)" \
    || die "Homebrew installation failed."
  for candidate in /opt/homebrew/bin/brew /usr/local/bin/brew; do
    if [[ -x "$candidate" ]]; then eval "$("$candidate" shellenv)"; break; fi
  done
fi
command -v brew >/dev/null 2>&1 || die "Homebrew is installed but not on PATH."

for tap in ${TAPS[@]+"${TAPS[@]}"}; do
  log "Tapping $tap"
  brew tap "$tap" || FAILED+=("tap $tap")
done

for formula in ${FORMULAE[@]+"${FORMULAE[@]}"}; do
  if brew list --formula "$formula" >/dev/null 2>&1; then
    log "Already installed: $formula"
  else
    log "Installing $formula"
    brew install "$formula" || FAILED+=("brew $formula")
  fi
done

for cask in ${CASKS[@]+"${CASKS[@]}"}; do
  if brew list --cask "$cask" >/dev/null 2>&1; then
    log "Already installed: $cask"
  else
    log "Installing cask $cask"
    brew install --cask "$cask" || FAILED+=("cask $cask")
  fi
done

if [[ ${#MAS_IDS[@]} -gt 0 ]]; then
  if ! command -v mas >/dev/null 2>&1; then
    warn "mas is not installed; skipping Mac App Store titles."
  elif ! mas account >/dev/null 2>&1; then
    warn "Not signed in to the App Store; skipping Mac App Store titles."
    warn "Sign in, then re-run this script to install them."
  else
    for id in "${MAS_IDS[@]}"; do
      log "Installing App Store title $id"
      mas install "$id" || FAILED+=("mas $id")
    done
  fi
fi

# ---------------------------------------------------------------------------
# System and driver extensions.
#
# `brew install --cask` reports success once the app is in place, but an app
# that ships a driver extension is not finished at that point: macOS will not
# load the extension until a human approves it, and some apps do not even
# submit it for approval until the app itself has been run once. An unattended
# install therefore leaves a working-looking machine where the extension does
# nothing. Report that here rather than let it pass as success.
#
# This stage never activates anything. Activation opens a GUI approval dialog
# and, for Karabiner, a file:// URL — neither belongs in an unattended run.
# ---------------------------------------------------------------------------
extension_state() {
  systemextensionsctl list 2>/dev/null | grep -F "$1" | head -1
}

check_extensions() {
  command -v systemextensionsctl >/dev/null 2>&1 || return 0
  local pending=()

  # Any extension already known to macOS but not yet enabled is awaiting a
  # human. This catches every vendor, not just the ones named below.
  while IFS= read -r line; do
    case "$line" in
      *"[activated enabled]"*) ;;
      *"[activated"*|*"[terminated"*)
        pending+=("$(printf '%s' "$line" | awk -F'\t' '{print $4}')")
        ;;
    esac
  done < <(systemextensionsctl list 2>/dev/null | grep -E '^\*|^ ' || true)

  # Karabiner submits its DriverKit extension only when the app or its helper
  # runs, so a fresh unattended install leaves no row at all to detect.
  local km="/Applications/.Karabiner-VirtualHIDDevice-Manager.app/Contents/MacOS/Karabiner-VirtualHIDDevice-Manager"
  if [[ -x "$km" ]] && [[ -z "$(extension_state org.pqrs.Karabiner-DriverKit-VirtualHIDDevice)" ]]; then
    warn "Karabiner installed its driver extension but never submitted it."
    warn "  Run: \"$km\" activate"
    warn "  Then approve it, and note that Karabiner also needs Input Monitoring."
  fi

  if [[ ${#pending[@]} -gt 0 ]]; then
    warn "${#pending[@]} system extension(s) are installed but not enabled:"
    for ext in "${pending[@]}"; do warn "  $ext"; done
    warn "Approve them in System Settings > General > Login Items & Extensions."
  fi
}

[[ "$(uname -s)" == "Darwin" ]] && check_extensions

if [[ ${#FAILED[@]} -gt 0 ]]; then
  warn "${#FAILED[@]} item(s) did not install:"
  for item in "${FAILED[@]}"; do warn "  $item"; done
  exit 1
fi

log "All tools installed."
"#;

#[cfg(test)]
mod tests {
    use super::*;

    /// A real captured manifest, parsed the way publish parses it. Going
    /// through the text rather than constructing the struct is the point:
    /// it proves the machine details have nowhere to land.
    const MANIFEST: &str = r#"
[meta]
captured_at = "2026-09-20T05:26:48Z"
hostname = "Someones-MacBook-Air.local"
os = "macos"

[[mas]]
id = 497799835
name = "Xcode"
version = "26.6"

[[unmanaged]]
name = "Ivanti Secure Access"
bundle_id = "net.pulsesecure.Pulse-Secure"
path = "/Applications/Ivanti Secure Access.app"
version = "22.7.1"
homepage = "https://www.ivanti.com/support/secure-access"

[[casks]]
name = "kitty"
"#;

    fn manifest() -> AppsInventory {
        AppsInventory::parse(MANIFEST).unwrap()
    }

    #[test]
    fn the_script_installs_tools_without_describing_the_machine() {
        let brewfile = "tap \"homebrew/cask\"\nbrew \"git\"\ncask \"kitty\"\n";
        let m = manifest();
        let script = generate(
            &Inventory {
                brewfile: Some(brewfile),
                apps: Some(&m),
            },
            &[],
        );

        // The part that installs tools survives.
        assert!(script.contains("'git'"));
        assert!(script.contains("'kitty'"));
        assert!(script.contains("'497799835'"));
        assert!(script.contains("'homebrew/cask'"));

        // The part that describes the machine does not. A version list is a
        // vulnerability list, and an app installed by hand cannot be
        // installed by this script anyway.
        assert!(!script.contains("Someones-MacBook-Air"));
        assert!(!script.contains("2026-09-20"));
        assert!(!script.contains("26.6"));
        assert!(!script.contains("22.7.1"));
        assert!(!script.contains("Ivanti"));
        assert!(!script.contains("pulsesecure"));
    }

    #[test]
    fn a_personal_tap_is_dropped_rather_than_published_or_broken() {
        // The fully-qualified formula matters as much as the tap line: it
        // carries the owner's name too, and a redactor rewriting it would
        // leave a package reference that resolves to nothing.
        let brewfile = concat!(
            "tap \"homebrew/cask\"\n",
            "tap \"someuser/tools\"\n",
            "brew \"git\"\n",
            "brew \"someuser/tools/widget\"\n",
            "cask \"someuser/tools/thing\"\n",
        );
        let p = plan(
            &Inventory {
                brewfile: Some(brewfile),
                apps: None,
            },
            &["someuser".to_string()],
        );

        assert_eq!(p.taps, vec!["homebrew/cask".to_string()]);
        assert_eq!(p.formulae, vec!["git".to_string()]);
        assert!(p.casks.is_empty());
        assert_eq!(p.omitted, 3);

        // Silently dropping it would leave the reader wondering why a tap
        // they rely on is missing.
        let script = render(&p);
        assert!(script.contains("personal tap"));
        assert!(!script.contains("someuser"));
    }

    #[test]
    fn entries_are_deduplicated_and_ordered_so_output_is_stable() {
        let brewfile = "cask \"kitty\"\nbrew \"ripgrep\"\nbrew \"bat\"\n";
        let m = manifest(); // also lists the kitty cask
        let inventory = Inventory {
            brewfile: Some(brewfile),
            apps: Some(&m),
        };

        let p = plan(&inventory, &[]);
        assert_eq!(p.casks, vec!["kitty".to_string()], "cask listed twice");
        assert_eq!(p.formulae, vec!["bat".to_string(), "ripgrep".to_string()]);

        // Ordering is what keeps the published diff readable across
        // captures, and `brew bundle dump` does not promise an order.
        let mut shuffled = p.casks.clone();
        shuffled.reverse();
        shuffled.sort();
        assert_eq!(p.casks, shuffled, "output must not depend on input order");
    }

    #[test]
    fn a_name_containing_a_quote_cannot_break_out_of_the_shell_literal() {
        // The Brewfile parser cannot itself produce such a name, but the
        // manifest is captured from arbitrary application names, and these
        // values are interpolated into a shell script.
        let script = render(&Plan {
            casks: vec!["od'd".to_string()],
            ..Plan::default()
        });
        assert!(script.contains(r"'od'\''d'"), "unescaped quote: {script}");
    }

    #[test]
    fn an_empty_inventory_still_renders_a_runnable_script() {
        // "Runnable" is the claim, so run the parser. The previous version
        // asserted that a substring of the BODY constant appeared in the
        // BODY constant, which no regression could break. macOS ships bash
        // 3.2, where an unguarded empty-array expansion under `set -u` is an
        // unbound-variable error — exactly what this has to catch.
        let script = generate(&Inventory::default(), &[]);
        assert_syntax_ok(&script);
        assert!(script.contains("TAPS=()"));
    }

    #[test]
    fn a_full_inventory_renders_a_script_bash_accepts() {
        let m = manifest();
        let script = generate(
            &Inventory {
                brewfile: Some("tap \"a/b\"\nbrew \"git\"\ncask \"kitty\"\n"),
                apps: Some(&m),
            },
            &[],
        );
        assert_syntax_ok(&script);
    }

    #[test]
    fn a_quoted_name_from_the_manifest_survives_into_valid_bash() {
        // The escape test above builds a Plan directly. This one proves a
        // hostile name reaches `array` through the real parse path and
        // still leaves the script parseable.
        let apps = AppsInventory::parse(
            "[[casks]]\nname = \"od'd; rm -rf /\"\n\n[[casks]]\nname = \"$(id)\"\n",
        )
        .unwrap();
        let script = generate(
            &Inventory {
                brewfile: None,
                apps: Some(&apps),
            },
            &[],
        );
        assert_syntax_ok(&script);
        assert!(script.contains(r"'od'\''d; rm -rf /'"));
        assert!(script.contains("'$(id)'"));
    }

    #[test]
    fn the_extension_check_reports_both_ways_an_extension_can_be_unusable() {
        // A driver extension fails silently in two distinct ways after an
        // unattended cask install, and `brew` calls both of them success.
        let script = generate(
            &Inventory {
                brewfile: None,
                apps: None,
            },
            &[],
        );
        assert_syntax_ok(&script);

        // Submitted but not approved — found by state, for any vendor.
        assert!(script.contains("[activated enabled]"), "{script}");
        assert!(
            script.contains("System Settings > General > Login Items & Extensions"),
            "must name the pane that actually holds the control on current macOS"
        );
        // Never submitted — invisible to systemextensionsctl, so it is found
        // by the helper's presence instead.
        assert!(
            script.contains("org.pqrs.Karabiner-DriverKit-VirtualHIDDevice"),
            "{script}"
        );
        assert!(
            script.contains("Karabiner-VirtualHIDDevice-Manager"),
            "{script}"
        );

        // The stage must never activate: activation raises a GUI prompt and
        // opens a file:// URL, neither of which belongs in an unattended run.
        // Naming the command inside a warning is the point; running it is the
        // defect, so judge each line by whether it executes or reports.
        for line in script.lines() {
            let trimmed = line.trim();
            if !trimmed.contains("activate") {
                continue;
            }
            let reports = trimmed.starts_with('#')
                || trimmed.starts_with("warn ")
                || trimmed.starts_with("log ")
                || trimmed.contains("[activated")
                || trimmed.contains("*\"[activated");
            assert!(
                reports,
                "this line runs an activation instead of reporting it: {trimmed}"
            );
        }
    }

    /// Parses the script with the system bash. A generated shell script that
    /// does not parse is the one defect no unit assertion would reveal.
    fn assert_syntax_ok(script: &str) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("install-apps.sh");
        std::fs::write(&path, script).unwrap();
        let out = std::process::Command::new("bash")
            .arg("-n")
            .arg(&path)
            .output()
            .expect("bash must be available to check the generated script");
        assert!(
            out.status.success(),
            "generated script is not valid bash: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

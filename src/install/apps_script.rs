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
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Plan {
    pub taps: Vec<String>,
    pub formulae: Vec<String>,
    pub casks: Vec<String>,
    /// Mac App Store ids. Ids only: the script installs by id, and a list
    /// of purchase titles identifies its owner.
    pub mas: Vec<u64>,
    /// Entries dropped because they sit under an omitted namespace.
    pub omitted: usize,
    /// Entries dropped because they are not a plain Homebrew name. See
    /// [`is_safe_token`].
    pub rejected: Vec<String>,
}

/// True when `token` is shaped like a Homebrew name and nothing else:
/// `name`, `owner/tap`, or `owner/tap/name`, built from letters, digits and
/// `@ . _ + -`, and starting with a letter or digit.
///
/// A Brewfile is Ruby. `brew bundle` evaluates it, so a pulled Brewfile
/// carrying `system "..."`, or a name holding `#{...}` interpolation, runs
/// code on the machine that installs it. dotdipper therefore never hands a
/// pulled Brewfile to `brew bundle`; it extracts names with a regex and
/// passes them to `brew install` as argv. This check is the second half of
/// that: the regex extraction would happily capture `foo#{`id`}`, and a name
/// beginning with `-` would be read as an option. Anything this function
/// refuses never reaches brew.
pub fn is_safe_token(token: &str) -> bool {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"^[A-Za-z0-9@._+-]+(/[A-Za-z0-9@._+-]+){0,2}$").expect("valid token regex")
    });
    token
        .split('/')
        .all(|segment| segment.starts_with(|c: char| c.is_ascii_alphanumeric()))
        && re.is_match(token)
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
    let mut rejected: Vec<String> = Vec::new();

    {
        // Applies to taps (`owner/tap`) and to fully-qualified packages
        // (`owner/tap/formula`) alike: in both, the first segment is the owner.
        let mut keep = |name: &str| {
            if !is_safe_token(name) {
                rejected.push(name.to_string());
                return false;
            }
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
    dedup_sorted(&mut rejected);
    plan.rejected = rejected;

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

    if !plan.rejected.is_empty() {
        // Count only: the names themselves are what was unsafe to emit.
        s.push_str(&format!(
            "# {} Brewfile or manifest {} not a plain Homebrew name and {} left out.\n\n",
            plan.rejected.len(),
            if plan.rejected.len() == 1 {
                "entry was"
            } else {
                "entries were"
            },
            if plan.rejected.len() == 1 {
                "was"
            } else {
                "were"
            },
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

# ---------------------------------------------------------------------------
# Taps. A third-party tap is someone's git repository of Ruby, evaluated with
# your privileges whenever anything from it installs, and `brew install
# owner/tap/name` taps on demand without asking. So third-party taps are
# opt-in: set DOTDIPPER_ALLOW_TAPS=1 to tap them, and otherwise every package
# under a tap that is not already present is skipped and listed.
# ---------------------------------------------------------------------------
SKIPPED=()
lower() { printf '%s' "$1" | tr '[:upper:]' '[:lower:]'; }
is_official_tap() {
  case "$(lower "$1")" in homebrew/core|homebrew/cask) return 0 ;; esac
  return 1
}
TAPPED="$(brew tap 2>/dev/null | tr '[:upper:]' '[:lower:]')"
is_tapped() { printf '%s\n' "$TAPPED" | grep -qxF -- "$(lower "$1")"; }

TAPS_HELD=0
for tap in ${TAPS[@]+"${TAPS[@]}"}; do
  if is_official_tap "$tap" || is_tapped "$tap"; then
    continue
  fi
  if [[ "${DOTDIPPER_ALLOW_TAPS:-0}" == "1" ]]; then
    log "Tapping $tap"
    brew tap -- "$tap" || FAILED+=("tap $tap")
  else
    TAPS_HELD=1
    warn "Not tapping $tap (third-party code)."
  fi
done
if [[ "$TAPS_HELD" -eq 1 ]]; then
  warn "Third-party taps were not added. Read them, then re-run with"
  warn "DOTDIPPER_ALLOW_TAPS=1 to tap them and install what they provide."
fi
TAPPED="$(brew tap 2>/dev/null | tr '[:upper:]' '[:lower:]')"

# True when installing the name cannot tap anything new.
installable() {
  [[ "$1" == */*/* ]] || return 0
  is_official_tap "${1%/*}" || is_tapped "${1%/*}"
}

for formula in ${FORMULAE[@]+"${FORMULAE[@]}"}; do
  if ! installable "$formula"; then
    SKIPPED+=("brew $formula (tap ${formula%/*} not tapped)")
  elif brew list --formula -- "$formula" >/dev/null 2>&1; then
    log "Already installed: $formula"
  else
    log "Installing $formula"
    brew install --formula -- "$formula" || FAILED+=("brew $formula")
  fi
done

for cask in ${CASKS[@]+"${CASKS[@]}"}; do
  if ! installable "$cask"; then
    SKIPPED+=("cask $cask (tap ${cask%/*} not tapped)")
  elif brew list --cask -- "$cask" >/dev/null 2>&1; then
    log "Already installed: $cask"
  else
    log "Installing cask $cask"
    brew install --cask -- "$cask" || FAILED+=("cask $cask")
  fi
done

# ---------------------------------------------------------------------------
# App Store. mas 7 removed `mas account`, so sign-in can only be ruled out
# when an older mas says so; otherwise each install is attempted and a
# failure is collected like any other. mas 2 and later install as root,
# which `mas help install` states.
# ---------------------------------------------------------------------------
if [[ ${#MAS_IDS[@]} -gt 0 ]]; then
  if ! command -v mas >/dev/null 2>&1; then
    warn "mas is not installed; skipping Mac App Store titles."
  elif mas account 2>&1 | grep -qi 'not signed in'; then
    warn "Not signed in to the App Store; skipping Mac App Store titles."
    warn "Sign in, then re-run this script to install them."
  else
    MAS_SUDO=""
    if mas help install 2>&1 | grep -q 'Requires root'; then
      MAS_SUDO="sudo"
    fi
    MAS_FAILED=0
    for id in "${MAS_IDS[@]}"; do
      log "Installing App Store title $id"
      if ! $MAS_SUDO mas install "$id"; then
        FAILED+=("mas $id")
        MAS_FAILED=1
      fi
    done
    if [[ "$MAS_FAILED" -eq 1 ]]; then
      warn "If App Store installs failed, sign in with the App Store app and re-run."
    fi
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

if [[ ${#SKIPPED[@]} -gt 0 ]]; then
  warn "${#SKIPPED[@]} item(s) skipped because their tap is not present:"
  for item in "${SKIPPED[@]}"; do warn "  $item"; done
  warn "Re-run with DOTDIPPER_ALLOW_TAPS=1 to tap and install them."
fi

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
    fn a_hostile_name_from_the_manifest_is_left_out_and_counted() {
        // The escape test above builds a Plan directly and still proves the
        // quoting. Through the real parse path, a name that is not a plain
        // Homebrew token no longer reaches the script at all: the same plan
        // feeds `install apps`, where such a name would reach brew.
        let apps = AppsInventory::parse(
            "[[casks]]\nname = \"od'd; rm -rf /\"\n\n[[casks]]\nname = \"$(id)\"\n\n[[casks]]\nname = \"kitty\"\n",
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
        assert!(script.contains("'kitty'"));
        assert!(!script.contains("rm -rf"), "{script}");
        assert!(!script.contains("$(id)"), "{script}");
        assert!(script.contains("2 Brewfile or manifest entries were not a plain Homebrew name"));
    }

    #[test]
    fn ruby_in_a_brewfile_never_becomes_an_install_entry() {
        // `brew bundle` would execute every one of these lines.
        let brewfile = concat!(
            "system \"touch /tmp/pwned\"\n",
            "brew \"git#{`touch /tmp/pwned`}\"\n",
            "cask \"--force\"\n",
            "brew \"../../etc\"\n",
            "brew \"jq\"\n",
            "tap \"olets/tap\"\n",
        );
        let p = plan(
            &Inventory {
                brewfile: Some(brewfile),
                apps: None,
            },
            &[],
        );
        assert_eq!(p.formulae, vec!["jq".to_string()]);
        assert_eq!(p.taps, vec!["olets/tap".to_string()]);
        assert!(p.casks.is_empty());
        assert_eq!(p.rejected.len(), 3, "{:?}", p.rejected);
    }

    #[test]
    fn plain_homebrew_names_are_safe_tokens_and_nothing_else_is() {
        for ok in [
            "jq",
            "python@3.13",
            "c++utilities",
            "olets/tap",
            "olets/tap/git-prompt-kit",
            "font-fira-code",
        ] {
            assert!(is_safe_token(ok), "{ok}");
        }
        for bad in [
            "", "-v", "--force", "a/b/c/d", "git#{x}", "a b", "a;b", "$(id)", "../etc", "a/../b",
            "/abs", "a/",
        ] {
            assert!(!is_safe_token(bad), "{bad}");
        }
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

    #[test]
    fn third_party_taps_are_opt_in_and_mas_is_driven_the_way_mas_7_needs() {
        let script = generate(&Inventory::default(), &[]);
        assert_syntax_ok(&script);
        assert!(script.contains("DOTDIPPER_ALLOW_TAPS"));
        assert!(script.contains("brew tap -- \"$tap\""));
        assert!(
            !script.contains("brew tap \"$tap\""),
            "an unconditional tap"
        );
        assert!(script.contains("Requires root"));
        assert!(
            !script.contains("! mas account >/dev/null"),
            "mas 7 has no `account`, so a failing `mas account` must not skip the App Store"
        );
    }

    /// Runs the generated script with fake `brew`, `mas` and `sudo` that
    /// record their argv. `brew tap` reports `trusted/tools` as present, and
    /// `mas` behaves like 7.0: no `account`, and `install` requires root.
    #[cfg(target_os = "macos")]
    fn run_script(brewfile: &str, allow_taps: bool) -> (Vec<String>, String) {
        let dir = tempfile::TempDir::new().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        let log = dir.path().join("calls.log");
        let fakes = [
            (
                "brew",
                "[[ \"$1\" == tap && $# -eq 1 ]] && { echo trusted/tools; exit 0; }\n\
                 [[ \"$1\" == list ]] && exit 1\n",
            ),
            (
                "mas",
                "[[ \"$1\" == account ]] && { echo \"Error: Unexpected argument 'account'\" >&2; exit 64; }\n\
                 [[ \"$1\" == help ]] && { echo 'Requires root privileges to install apps'; exit 0; }\n",
            ),
            ("sudo", ""),
            ("systemextensionsctl", "exit 0\n"),
        ];
        for (tool, prelude) in fakes {
            let path = bin.join(tool);
            std::fs::write(
                &path,
                format!(
                    "#!/bin/bash\n{prelude}printf '{tool}' >> '{log}'\nprintf ' %s' \"$@\" >> '{log}'\necho >> '{log}'\n",
                    log = log.display()
                ),
            )
            .unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let script = generate(
            &Inventory {
                brewfile: Some(brewfile),
                apps: None,
            },
            &[],
        );
        let file = dir.path().join("install-apps.sh");
        std::fs::write(&file, script).unwrap();
        let out = std::process::Command::new("bash")
            .arg(&file)
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .env("DOTDIPPER_ALLOW_TAPS", if allow_taps { "1" } else { "0" })
            .output()
            .unwrap();
        let calls = std::fs::read_to_string(&log)
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.starts_with("systemextensionsctl"))
            .map(str::to_string)
            .collect();
        (calls, String::from_utf8_lossy(&out.stderr).into_owned())
    }

    #[cfg(target_os = "macos")]
    const MIXED_TAPS: &str = concat!(
        "tap \"homebrew/cask\"\n",
        "tap \"trusted/tools\"\n",
        "tap \"evil/tools\"\n",
        "brew \"jq\"\n",
        "brew \"trusted/tools/a\"\n",
        "brew \"evil/tools/x\"\n",
        "cask \"evil/tools/y\"\n",
        "mas \"Xcode\", id: 497799835\n",
    );

    #[cfg(target_os = "macos")]
    #[test]
    fn the_script_taps_nothing_third_party_by_default_and_skips_what_needs_it() {
        let (calls, stderr) = run_script(MIXED_TAPS, false);
        assert!(calls.iter().all(|c| !c.contains("evil")), "{calls:?}");
        assert!(
            calls.iter().all(|c| !c.starts_with("brew tap --")),
            "{calls:?}"
        );
        assert!(calls.contains(&"brew install --formula -- jq".to_string()));
        assert!(calls.contains(&"brew install --formula -- trusted/tools/a".to_string()));
        // mas 7: no `account`, so the install is attempted, as root.
        assert!(
            calls.contains(&"sudo mas install 497799835".to_string()),
            "{calls:?}"
        );
        assert!(stderr.contains("Not tapping evil/tools"), "{stderr}");
        assert!(
            stderr.contains("brew evil/tools/x (tap evil/tools not tapped)"),
            "{stderr}"
        );
        assert!(
            stderr.contains("cask evil/tools/y (tap evil/tools not tapped)"),
            "{stderr}"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_script_taps_third_party_taps_only_when_allowed() {
        let (calls, _) = run_script(MIXED_TAPS, true);
        assert!(
            calls.contains(&"brew tap -- evil/tools".to_string()),
            "{calls:?}"
        );
        assert!(
            !calls
                .iter()
                .any(|c| c.contains("tap -- trusted/tools") || c.contains("tap -- homebrew/cask")),
            "official and present taps are not re-tapped: {calls:?}"
        );
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

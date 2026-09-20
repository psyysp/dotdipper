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

use crate::apps::brew;
use crate::apps::AppsManifest;

/// The inventory a script is generated from. Both halves are optional so a
/// store that captured only one of them still produces a usable script.
#[derive(Debug, Default, Clone, Copy)]
pub struct Inventory<'a> {
    pub brewfile: Option<&'a str>,
    pub manifest: Option<&'a AppsManifest>,
}

/// One installable item, resolved from the inventory.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub taps: Vec<String>,
    pub formulae: Vec<String>,
    pub casks: Vec<String>,
    /// Mac App Store ids. Names are kept only as a shell comment, so the
    /// script reads sensibly without depending on them.
    pub mas: Vec<(u64, String)>,
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
            let parsed = brew::parse_brewfile(brewfile);
            plan.taps
                .extend(parsed.taps.into_iter().filter(|t| keep(t)));
            plan.formulae
                .extend(parsed.formulae.into_iter().filter(|f| keep(f)));
            plan.casks
                .extend(parsed.casks.into_iter().filter(|c| keep(c)));
        }

        if let Some(manifest) = inventory.manifest {
            plan.casks.extend(
                manifest
                    .casks
                    .iter()
                    .map(|c| c.name.clone())
                    .filter(|c| keep(c)),
            );
            plan.mas
                .extend(manifest.mas.iter().map(|m| (m.id, m.name.clone())));
        }
    }

    plan.omitted = omitted;

    dedup_sorted(&mut plan.taps);
    dedup_sorted(&mut plan.formulae);
    dedup_sorted(&mut plan.casks);
    plan.mas.sort();
    plan.mas.dedup_by_key(|(id, _)| *id);

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
    s.push_str(&array(
        "MAS_IDS",
        plan.mas.iter().map(|(id, _)| id.to_string()),
    ));

    if !plan.mas.is_empty() {
        s.push_str("# Mac App Store titles, for reference only:\n");
        for (id, name) in &plan.mas {
            s.push_str(&format!("#   {id}  {}\n", name.replace('\n', " ")));
        }
        s.push('\n');
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
    use crate::apps::{AppsManifest, CaskEntry, ManifestMeta, MasApp, UnmanagedApp};

    fn manifest() -> AppsManifest {
        AppsManifest {
            meta: ManifestMeta {
                captured_at: "2026-09-20T05:26:48Z".to_string(),
                hostname: "Someones-MacBook-Air.local".to_string(),
                os: "macos".to_string(),
            },
            mas: vec![MasApp {
                id: 497799835,
                name: "Xcode".to_string(),
                version: "26.6".to_string(),
            }],
            unmanaged: vec![UnmanagedApp {
                name: "Ivanti Secure Access".to_string(),
                bundle_id: Some("net.pulsesecure.Pulse-Secure".to_string()),
                path: "/Applications/Ivanti Secure Access.app".to_string(),
                version: Some("22.7.1".to_string()),
                homepage: Some("https://www.ivanti.com/support/secure-access".to_string()),
            }],
            casks: vec![CaskEntry {
                name: "kitty".to_string(),
            }],
        }
    }

    #[test]
    fn the_script_installs_tools_without_describing_the_machine() {
        let brewfile = "tap \"homebrew/cask\"\nbrew \"git\"\ncask \"kitty\"\n";
        let m = manifest();
        let script = generate(
            &Inventory {
                brewfile: Some(brewfile),
                manifest: Some(&m),
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
                manifest: None,
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
            manifest: Some(&m),
        };

        let p = plan(&inventory, &[]);
        assert_eq!(p.casks, vec!["kitty".to_string()], "cask listed twice");
        assert_eq!(p.formulae, vec!["bat".to_string(), "ripgrep".to_string()]);

        assert_eq!(generate(&inventory, &[]), generate(&inventory, &[]));
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
        let script = generate(&Inventory::default(), &[]);
        // Under `set -u`, bash 3.2 treats an unguarded empty array expansion
        // as an unbound variable, so the guard is not decoration.
        assert!(script.contains("TAPS=()"));
        assert!(script.contains(r#"${TAPS[@]+"${TAPS[@]}"}"#));
    }
}

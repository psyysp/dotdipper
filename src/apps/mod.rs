//! Capture and restore actually-installed macOS applications.
//!
//! Writes a Homebrew `Brewfile` and `apps_manifest.toml` into the compiled
//! repo so they can be pushed with the rest of the user's dotfiles.

pub mod applications;
pub mod brew;
pub mod mas;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::cfg::Config;
use crate::ui;

pub use applications::{extract_plist_string, is_managed_app};
pub use brew::{
    brewfile_has_mas_apps, brewfile_has_mas_formula, cask_matches_app, ensure_mas_formula,
    parse_brewfile, parse_cask_list, BrewfilePlan,
};
pub use mas::parse_mas_list;

pub const BREWFILE_NAME: &str = "Brewfile";
pub const MANIFEST_NAME: &str = "apps_manifest.toml";

/// Summary returned by [`capture`].
#[derive(Debug, Clone)]
pub struct CaptureResult {
    pub formulae: usize,
    pub casks: usize,
    pub mas: usize,
    pub unmanaged: usize,
    pub brewfile_path: PathBuf,
    pub manifest_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppsManifest {
    pub meta: ManifestMeta,
    #[serde(default)]
    pub mas: Vec<MasApp>,
    #[serde(default)]
    pub unmanaged: Vec<UnmanagedApp>,
    #[serde(default)]
    pub casks: Vec<CaskEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestMeta {
    pub captured_at: String,
    pub hostname: String,
    pub os: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MasApp {
    pub id: u64,
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnmanagedApp {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_id: Option<String>,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CaskEntry {
    pub name: String,
}

/// Dump Homebrew, MAS, and /Applications state into the compiled repo.
pub fn capture(config: &Config) -> Result<CaptureResult> {
    ui::info("Capturing Homebrew packages...");
    let mut brewfile = brew::dump_brewfile()?;
    let cask_list = brew::list_installed_casks().unwrap_or_else(|err| {
        ui::warn(&format!(
            "Could not list Homebrew casks ({:#}); falling back to Brewfile cask lines",
            err
        ));
        Vec::new()
    });

    ui::info("Capturing Mac App Store apps...");
    let mas_apps = mas::list_installed()?;

    if !mas_apps.is_empty() || brew::brewfile_has_mas_apps(&brewfile) {
        brewfile = brew::ensure_mas_formula(&brewfile);
    }

    let scan_applications = config
        .apps
        .as_ref()
        .map(|apps| apps.scan_applications)
        .unwrap_or(true);

    let unmanaged = if scan_applications {
        ui::info("Scanning /Applications...");
        let installed = applications::scan_applications()?;
        let plan = brew::parse_brewfile(&brewfile);
        let mut cask_tokens = cask_list.clone();
        for name in plan.casks {
            if !cask_tokens.iter().any(|existing| existing == &name) {
                cask_tokens.push(name);
            }
        }
        applications::find_unmanaged(&installed, &cask_tokens, &mas_apps)
    } else {
        Vec::new()
    };

    let compiled = crate::paths::compiled_dir()?;
    fs::create_dir_all(&compiled).context("Failed to create compiled directory")?;

    let brewfile_path = compiled.join(BREWFILE_NAME);
    fs::write(&brewfile_path, &brewfile)
        .with_context(|| format!("Failed to write Brewfile to {}", brewfile_path.display()))?;

    let hostname = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "unknown".to_string());

    let mut casks: Vec<CaskEntry> = cask_list
        .into_iter()
        .map(|name| CaskEntry { name })
        .collect();
    if casks.is_empty() {
        casks = brew::parse_brewfile(&brewfile)
            .casks
            .into_iter()
            .map(|name| CaskEntry { name })
            .collect();
    }
    casks.sort_by_key(|a| a.name.to_lowercase());

    let mut mas_apps = mas_apps;
    mas_apps.sort_by_key(|a| a.name.to_lowercase());

    let manifest = AppsManifest {
        meta: ManifestMeta {
            captured_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            hostname,
            os: "macos".to_string(),
        },
        mas: mas_apps,
        unmanaged,
        casks,
    };

    let manifest_path = compiled.join(MANIFEST_NAME);
    let toml_string =
        toml::to_string_pretty(&manifest).context("Failed to serialize apps manifest")?;
    fs::write(&manifest_path, toml_string).with_context(|| {
        format!(
            "Failed to write apps manifest to {}",
            manifest_path.display()
        )
    })?;

    let plan = brew::parse_brewfile(&brewfile);
    Ok(CaptureResult {
        formulae: plan.formulae.len(),
        casks: manifest.casks.len(),
        mas: manifest.mas.len(),
        unmanaged: manifest.unmanaged.len(),
        brewfile_path,
        manifest_path,
    })
}

/// Restore Homebrew packages from the compiled Brewfile, then report unmanaged apps.
pub fn install(_config: &Config) -> Result<()> {
    let brewfile_path = compiled_brewfile()?;
    if !brewfile_path.exists() {
        anyhow::bail!(
            "No Brewfile found at {}. Run 'dotdipper apps capture' first.",
            brewfile_path.display()
        );
    }

    ui::info(&format!(
        "Running brew bundle --file={}",
        brewfile_path.display()
    ));
    brew::bundle_install(&brewfile_path)?;
    ui::success("Homebrew bundle completed");

    report_unmanaged_apps()?;
    Ok(())
}

/// Print what `brew bundle` would install without running it.
pub fn dry_run_install() -> Result<()> {
    let brewfile_path = compiled_brewfile()?;
    if !brewfile_path.exists() {
        anyhow::bail!(
            "No Brewfile found at {}. Run 'dotdipper apps capture' first.",
            brewfile_path.display()
        );
    }

    let content = fs::read_to_string(&brewfile_path)
        .with_context(|| format!("Failed to read Brewfile at {}", brewfile_path.display()))?;
    let plan = brew::parse_brewfile(&content);

    ui::section("Would install from Brewfile:");
    print_named_list("Taps", &plan.taps);
    print_named_list("Formulae", &plan.formulae);
    print_named_list("Casks", &plan.casks);
    print_named_list("Mac App Store", &plan.mas);

    if let Ok(manifest) = load_manifest() {
        if manifest.unmanaged.is_empty() {
            ui::info("No unmanaged apps would need manual installation");
        } else {
            ui::warn(&format!(
                "{} unmanaged apps would still need to be installed manually",
                manifest.unmanaged.len()
            ));
            for app in &manifest.unmanaged {
                println!("    {} ({})", app.name, app.path);
            }
        }
    }

    Ok(())
}

/// Pretty-print the current compiled apps manifest, grouped by source.
pub fn list() -> Result<()> {
    let manifest = load_manifest()?;
    let plan = compiled_brewfile()
        .ok()
        .filter(|path| path.exists())
        .and_then(|path| fs::read_to_string(path).ok())
        .map(|content| brew::parse_brewfile(&content));

    ui::section("Captured applications");
    ui::info(&format!(
        "Captured at {} on {} ({})",
        manifest.meta.captured_at, manifest.meta.hostname, manifest.meta.os
    ));

    if let Some(plan) = &plan {
        ui::section(&format!("Homebrew formulae ({})", plan.formulae.len()));
        if plan.formulae.is_empty() {
            println!("  (none)");
        } else {
            for name in &plan.formulae {
                println!("  {}", name);
            }
        }

        ui::section(&format!("Homebrew taps ({})", plan.taps.len()));
        if plan.taps.is_empty() {
            println!("  (none)");
        } else {
            for name in &plan.taps {
                println!("  {}", name);
            }
        }
    }

    ui::section(&format!("Homebrew casks ({})", manifest.casks.len()));
    if manifest.casks.is_empty() {
        println!("  (none)");
    } else {
        for cask in &manifest.casks {
            println!("  {}", cask.name);
        }
    }

    ui::section(&format!("Mac App Store ({})", manifest.mas.len()));
    if manifest.mas.is_empty() {
        println!("  (none)");
    } else {
        for app in &manifest.mas {
            println!("  {} ({}) [{}]", app.name, app.version, app.id);
        }
    }

    ui::section(&format!("Unmanaged apps ({})", manifest.unmanaged.len()));
    if manifest.unmanaged.is_empty() {
        println!("  (none)");
    } else {
        for app in &manifest.unmanaged {
            let mut extra = app.path.clone();
            if let Some(version) = &app.version {
                extra = format!("{} · {}", extra, version);
            }
            println!("  {} ({})", app.name, extra);
        }
    }

    Ok(())
}

fn compiled_brewfile() -> Result<PathBuf> {
    Ok(crate::paths::compiled_dir()?.join(BREWFILE_NAME))
}

fn compiled_manifest() -> Result<PathBuf> {
    Ok(crate::paths::compiled_dir()?.join(MANIFEST_NAME))
}

fn load_manifest() -> Result<AppsManifest> {
    let path = compiled_manifest()?;
    if !path.exists() {
        anyhow::bail!(
            "No apps manifest found at {}. Run 'dotdipper apps capture' first.",
            path.display()
        );
    }
    let contents = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read apps manifest at {}", path.display()))?;
    toml::from_str(&contents).context("Failed to parse apps_manifest.toml")
}

fn report_unmanaged_apps() -> Result<()> {
    let Ok(manifest) = load_manifest() else {
        return Ok(());
    };
    if manifest.unmanaged.is_empty() {
        return Ok(());
    }

    ui::warn(&format!(
        "{} app(s) were not installed by Homebrew or the Mac App Store and must be installed manually:",
        manifest.unmanaged.len()
    ));
    for app in &manifest.unmanaged {
        match (&app.bundle_id, &app.version) {
            (Some(bundle_id), Some(version)) => {
                ui::warn(&format!(
                    "  {} {} ({}) — {}",
                    app.name, version, bundle_id, app.path
                ));
            }
            (Some(bundle_id), None) => {
                ui::warn(&format!("  {} ({}) — {}", app.name, bundle_id, app.path));
            }
            (None, Some(version)) => {
                ui::warn(&format!("  {} {} — {}", app.name, version, app.path));
            }
            (None, None) => {
                ui::warn(&format!("  {} — {}", app.name, app.path));
            }
        }
    }
    Ok(())
}

fn print_named_list(label: &str, items: &[String]) {
    ui::info(&format!("{} ({})", label, items.len()));
    if items.is_empty() {
        println!("    (none)");
        return;
    }
    for item in items {
        println!("    {}", item);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_round_trip() {
        let manifest = AppsManifest {
            meta: ManifestMeta {
                captured_at: "2026-08-26T20:00:00Z".into(),
                hostname: "testhost".into(),
                os: "macos".into(),
            },
            mas: vec![MasApp {
                id: 497799835,
                name: "Xcode".into(),
                version: "15.0".into(),
            }],
            unmanaged: vec![UnmanagedApp {
                name: "SomeApp".into(),
                bundle_id: Some("com.foo.SomeApp".into()),
                path: "/Applications/SomeApp.app".into(),
                version: Some("1.2.3".into()),
            }],
            casks: vec![CaskEntry {
                name: "kitty".into(),
            }],
        };

        let encoded = toml::to_string_pretty(&manifest).unwrap();
        assert!(encoded.contains("captured_at"));
        assert!(encoded.contains("[[mas]]"));
        assert!(encoded.contains("[[unmanaged]]"));
        assert!(encoded.contains("[[casks]]"));

        let decoded: AppsManifest = toml::from_str(&encoded).unwrap();
        assert_eq!(decoded, manifest);
    }
}

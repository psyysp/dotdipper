use anyhow::Result;
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::brew::{cask_matches_app, normalize_token};
use super::{MasApp, UnmanagedApp};

/// An application bundle discovered under /Applications or ~/Applications.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledApp {
    pub name: String,
    pub path: PathBuf,
    pub bundle_id: Option<String>,
    pub version: Option<String>,
}

/// Scan `/Applications` and `~/Applications` (top-level only) for `*.app` bundles.
pub fn scan_applications() -> Result<Vec<InstalledApp>> {
    let mut apps = Vec::new();
    apps.extend(scan_dir(Path::new("/Applications")));
    if let Some(home) = dirs::home_dir() {
        apps.extend(scan_dir(&home.join("Applications")));
    }
    apps.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.path.cmp(&b.path))
    });
    Ok(apps)
}

pub fn scan_dir(dir: &Path) -> Vec<InstalledApp> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut apps = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let is_app = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("app"));
        if !is_app {
            continue;
        }
        // Hidden bundles (e.g. .Karabiner-VirtualHIDDevice-Manager.app) are
        // helper components, not user-installable apps.
        let is_hidden = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with('.'));
        if is_hidden {
            continue;
        }
        apps.push(read_app_bundle(&path));
    }
    apps
}

pub fn read_app_bundle(path: &Path) -> InstalledApp {
    let name = path
        .file_stem()
        .or_else(|| path.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());

    let plist_path = path.join("Contents").join("Info.plist");
    let (bundle_id, version) = match read_plist_xml(&plist_path) {
        Some(xml) => (
            extract_plist_string(&xml, "CFBundleIdentifier"),
            extract_plist_string(&xml, "CFBundleShortVersionString"),
        ),
        None => (None, None),
    };

    InstalledApp {
        name,
        path: path.to_path_buf(),
        bundle_id,
        version,
    }
}

fn read_plist_xml(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    if bytes.starts_with(b"bplist") {
        let output = Command::new("plutil")
            .args(["-convert", "xml1", "-o", "-", "--"])
            .arg(path)
            .output()
            .ok()?;
        if output.status.success() {
            String::from_utf8(output.stdout).ok()
        } else {
            None
        }
    } else {
        String::from_utf8(bytes).ok()
    }
}

/// Best-effort extraction of a `<key>…</key><string>…</string>` pair from XML plist text.
pub fn extract_plist_string(plist_xml: &str, key: &str) -> Option<String> {
    let pattern = format!(
        r"(?s)<key>\s*{}\s*</key>\s*<string>([^<]*)</string>",
        regex::escape(key)
    );
    let re = Regex::new(&pattern).ok()?;
    let value = re.captures(plist_xml)?.get(1)?.as_str().trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

/// True when the app name matches a Homebrew cask token (case-insensitive, hyphen-normalized).
pub fn is_managed_by_cask(app_name: &str, cask_names: &[String]) -> bool {
    cask_names
        .iter()
        .any(|cask| cask_matches_app(app_name, cask))
}

/// True when the app name matches a Mac App Store app name.
pub fn is_managed_by_mas(app_name: &str, mas_names: &[String]) -> bool {
    let app = normalize_token(app_name);
    mas_names.iter().any(|name| normalize_token(name) == app)
}

pub fn is_managed_app(app_name: &str, cask_names: &[String], mas_names: &[String]) -> bool {
    is_managed_by_cask(app_name, cask_names) || is_managed_by_mas(app_name, mas_names)
}

pub fn find_unmanaged(
    apps: &[InstalledApp],
    cask_names: &[String],
    mas_apps: &[MasApp],
) -> Vec<UnmanagedApp> {
    let mas_names: Vec<String> = mas_apps.iter().map(|app| app.name.clone()).collect();
    let mut unmanaged = Vec::new();

    for app in apps {
        if is_managed_app(&app.name, cask_names, &mas_names) {
            continue;
        }
        unmanaged.push(UnmanagedApp {
            name: app.name.clone(),
            bundle_id: app.bundle_id.clone(),
            path: app.path.display().to_string(),
            version: app.version.clone(),
        });
    }

    unmanaged.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.path.cmp(&b.path))
    });
    unmanaged
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    const SAMPLE_PLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>
    <string>com.foo.SomeApp</string>
    <key>CFBundleName</key>
    <string>SomeApp</string>
    <key>CFBundleShortVersionString</key>
    <string>1.2.3</string>
</dict>
</plist>
"#;

    #[test]
    fn extract_plist_string_reads_identifier_and_version() {
        assert_eq!(
            extract_plist_string(SAMPLE_PLIST, "CFBundleIdentifier").as_deref(),
            Some("com.foo.SomeApp")
        );
        assert_eq!(
            extract_plist_string(SAMPLE_PLIST, "CFBundleShortVersionString").as_deref(),
            Some("1.2.3")
        );
        assert_eq!(extract_plist_string(SAMPLE_PLIST, "MissingKey"), None);
        assert_eq!(
            extract_plist_string("<plist></plist>", "CFBundleIdentifier"),
            None
        );
    }

    #[test]
    fn extract_plist_string_handles_compact_whitespace() {
        let xml = "<key>CFBundleIdentifier</key><string>com.bar.Baz</string>";
        assert_eq!(
            extract_plist_string(xml, "CFBundleIdentifier").as_deref(),
            Some("com.bar.Baz")
        );
    }

    #[test]
    fn managed_detection_matches_cask_and_mas_names() {
        let casks = vec!["kitty".into(), "visual-studio-code".into()];
        let mas = vec!["Xcode".into()];
        assert!(is_managed_app("Kitty", &casks, &mas));
        assert!(is_managed_app("Visual Studio Code", &casks, &mas));
        assert!(is_managed_app("Xcode", &casks, &mas));
        assert!(!is_managed_app("SecretApp", &casks, &mas));
    }

    #[test]
    fn find_unmanaged_filters_cask_and_mas() {
        let apps = vec![
            InstalledApp {
                name: "Kitty".into(),
                path: PathBuf::from("/Applications/Kitty.app"),
                bundle_id: Some("net.kovidgoyal.kitty".into()),
                version: Some("0.32".into()),
            },
            InstalledApp {
                name: "Xcode".into(),
                path: PathBuf::from("/Applications/Xcode.app"),
                bundle_id: Some("com.apple.dt.Xcode".into()),
                version: Some("15.0".into()),
            },
            InstalledApp {
                name: "SomeApp".into(),
                path: PathBuf::from("/Applications/SomeApp.app"),
                bundle_id: Some("com.foo.SomeApp".into()),
                version: Some("1.2.3".into()),
            },
        ];
        let unmanaged = find_unmanaged(
            &apps,
            &["kitty".into()],
            &[MasApp {
                id: 497799835,
                name: "Xcode".into(),
                version: "15.0".into(),
            }],
        );
        assert_eq!(unmanaged.len(), 1);
        assert_eq!(unmanaged[0].name, "SomeApp");
        assert_eq!(unmanaged[0].bundle_id.as_deref(), Some("com.foo.SomeApp"));
    }

    #[test]
    fn read_app_bundle_from_xml_plist() {
        let tmp = TempDir::new().unwrap();
        let app_dir = tmp.path().join("SomeApp.app");
        fs::create_dir_all(app_dir.join("Contents")).unwrap();
        let mut file = fs::File::create(app_dir.join("Contents/Info.plist")).unwrap();
        file.write_all(SAMPLE_PLIST.as_bytes()).unwrap();
        drop(file);

        let scanned = scan_dir(tmp.path());
        assert_eq!(scanned.len(), 1);
        assert_eq!(scanned[0].name, "SomeApp");
        assert_eq!(scanned[0].bundle_id.as_deref(), Some("com.foo.SomeApp"));
        assert_eq!(scanned[0].version.as_deref(), Some("1.2.3"));
    }
}

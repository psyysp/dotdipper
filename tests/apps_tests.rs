//! Integration tests for macOS application capture parsers and manifest format.
//! These tests are pure (no brew/mas/network).

#![cfg(target_os = "macos")]

use dotdipper::apps::{
    brewfile_has_mas_apps, brewfile_has_mas_formula, cask_matches_app, ensure_mas_formula,
    extract_plist_string, is_managed_app, parse_brewfile, parse_cask_list, parse_mas_list,
    AppsManifest, CaskEntry, ManifestMeta, MasApp, UnmanagedApp,
};

#[test]
fn mas_list_parser_handles_spacing_and_versions() {
    let output = "\
497799835  Xcode  (15.0)
408981434  iMovie  (10.3.5)
409201541 Pages (13.2)
";
    let apps = parse_mas_list(output);
    assert_eq!(apps.len(), 3);
    let xcode = apps.iter().find(|a| a.name == "Xcode").unwrap();
    assert_eq!(xcode.id, 497799835);
    assert_eq!(xcode.version, "15.0");
    let pages = apps.iter().find(|a| a.name == "Pages").unwrap();
    assert_eq!(pages.id, 409201541);
}

#[test]
fn mas_list_parser_skips_malformed_lines() {
    assert!(parse_mas_list("not a mas line\n").is_empty());
    assert!(parse_mas_list("").is_empty());
}

#[test]
fn plist_extraction_reads_identifier_and_version() {
    let xml = r#"
<dict>
    <key>CFBundleIdentifier</key>
    <string>com.foo.SomeApp</string>
    <key>CFBundleShortVersionString</key>
    <string>1.2.3</string>
</dict>
"#;
    assert_eq!(
        extract_plist_string(xml, "CFBundleIdentifier").as_deref(),
        Some("com.foo.SomeApp")
    );
    assert_eq!(
        extract_plist_string(xml, "CFBundleShortVersionString").as_deref(),
        Some("1.2.3")
    );
    assert!(extract_plist_string(xml, "CFBundleName").is_none());
}

#[test]
fn brewfile_mas_formula_detection() {
    assert!(brewfile_has_mas_formula(
        "brew \"mas\"\nmas \"Xcode\", id: 497799835\n"
    ));
    assert!(brewfile_has_mas_formula("brew 'mas'\n"));
    assert!(!brewfile_has_mas_formula("mas \"Xcode\", id: 497799835\n"));
    assert!(!brewfile_has_mas_formula("brew \"mas-cli\"\n"));
    assert!(brewfile_has_mas_apps("mas \"Xcode\", id: 497799835\n"));
    assert!(!brewfile_has_mas_apps("brew \"git\"\n"));
}

#[test]
fn brewfile_ensure_mas_formula_is_idempotent() {
    let original = "tap \"homebrew/core\"\nbrew \"git\"\nmas \"Xcode\", id: 497799835\n";
    let once = ensure_mas_formula(original);
    assert!(once.starts_with("brew \"mas\"\n"));
    assert!(brewfile_has_mas_formula(&once));
    assert_eq!(ensure_mas_formula(&once), once);
}

#[test]
fn brewfile_parser_and_cask_list() {
    let content = r#"
tap "homebrew/cask"
brew "ripgrep"
cask "kitty"
cask "visual-studio-code"
mas "Xcode", id: 497799835
"#;
    let plan = parse_brewfile(content);
    assert_eq!(plan.taps, vec!["homebrew/cask".to_string()]);
    assert_eq!(plan.formulae, vec!["ripgrep".to_string()]);
    assert_eq!(
        plan.casks,
        vec!["kitty".to_string(), "visual-studio-code".to_string()]
    );
    assert_eq!(plan.mas, vec!["Xcode".to_string()]);

    assert_eq!(
        parse_cask_list("kitty visual-studio-code\nfirefox"),
        vec![
            "kitty".to_string(),
            "visual-studio-code".to_string(),
            "firefox".to_string()
        ]
    );
}

#[test]
fn cask_name_matching_is_case_and_hyphen_insensitive() {
    assert!(cask_matches_app("Kitty", "kitty"));
    assert!(cask_matches_app(
        "Visual Studio Code.app",
        "visual-studio-code"
    ));
    assert!(is_managed_app(
        "Xcode",
        &["kitty".into()],
        &["Xcode".into()]
    ));
    assert!(!is_managed_app(
        "SecretApp",
        &["kitty".into()],
        &["Xcode".into()]
    ));
}

#[test]
fn apps_manifest_toml_round_trip() {
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
    assert!(encoded.contains("os = \"macos\""));
    assert!(encoded.contains("id = 497799835"));
    assert!(encoded.contains("bundle_id = \"com.foo.SomeApp\""));
    assert!(encoded.contains("name = \"kitty\""));

    let decoded: AppsManifest = toml::from_str(&encoded).unwrap();
    assert_eq!(decoded, manifest);
}

#[test]
fn apps_manifest_parses_spec_example() {
    let toml_src = r#"
[meta]
captured_at = "2026-08-26T20:00:00Z"
hostname = "testhost"
os = "macos"

[[mas]]
id = 497799835
name = "Xcode"
version = "15.0"

[[unmanaged]]
name = "SomeApp"
bundle_id = "com.foo.SomeApp"
path = "/Applications/SomeApp.app"
version = "1.2.3"

[[casks]]
name = "kitty"
"#;
    let manifest: AppsManifest = toml::from_str(toml_src).unwrap();
    assert_eq!(manifest.meta.os, "macos");
    assert_eq!(manifest.mas[0].id, 497799835);
    assert_eq!(manifest.unmanaged[0].name, "SomeApp");
    assert_eq!(manifest.casks[0].name, "kitty");
}

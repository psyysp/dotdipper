//! Integration tests for macOS application capture parsers and manifest format.
//! These tests are pure (no brew/mas/network).

#![cfg(target_os = "macos")]

use dotdipper::apps::{
    append_cask_entries, append_mas_entries, brewfile_has_mas_apps, brewfile_has_mas_formula,
    cask_matches_app, classify_scanned_app, ensure_mas_formula, extract_plist_string, homepage_for,
    is_managed_app, parse_brewfile, parse_cask_list, parse_mas_list, resolve_cask, resolve_mas,
    AppsManifest, CaskEntry, InstalledApp, ManifestMeta, MasApp, ScanClass, UnmanagedApp,
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
    assert!(cask_matches_app("iTerm", "iterm2"));
    assert!(cask_matches_app("zoom.us", "zoom"));
    assert!(cask_matches_app("iStat Menus 6", "istat-menus"));
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
fn brewfile_append_cask_and_mas_entries() {
    let brewfile = "cask \"kitty\"\nmas \"Xcode\", id: 497799835\n";
    let with_casks = append_cask_entries(brewfile, &["kitty".into(), "google-chrome".into()]);
    assert!(with_casks.contains("# Apps captured from /Applications (not previously in Brewfile)"));
    assert!(with_casks.contains("cask \"google-chrome\""));
    assert_eq!(with_casks.matches("cask \"kitty\"").count(), 1);
    assert_eq!(append_cask_entries(brewfile, &["kitty".into()]), brewfile);

    let with_mas = append_mas_entries(
        brewfile,
        &[
            MasApp {
                id: 497799835,
                name: "Xcode".into(),
                version: "15.0".into(),
            },
            MasApp {
                id: 937984704,
                name: "Amphetamine".into(),
                version: "5.3".into(),
            },
        ],
    );
    assert!(with_mas.contains("mas \"Amphetamine\", id: 937984704"));
    assert_eq!(with_mas.matches("mas \"Xcode\"").count(), 1);
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
            homepage: Some("https://example.com".into()),
        }],
        casks: vec![CaskEntry {
            name: "kitty".into(),
        }],
    };

    let encoded = toml::to_string_pretty(&manifest).unwrap();
    assert!(encoded.contains("os = \"macos\""));
    assert!(encoded.contains("id = 497799835"));
    assert!(encoded.contains("bundle_id = \"com.foo.SomeApp\""));
    assert!(encoded.contains("homepage = \"https://example.com\""));
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

fn scanned_app(name: &str, bundle_id: Option<&str>) -> InstalledApp {
    InstalledApp {
        name: name.into(),
        path: format!("/Applications/{name}.app").into(),
        bundle_id: bundle_id.map(str::to_string),
        version: Some("1.0".into()),
    }
}

#[test]
fn capture_classification_loop_promotes_formerly_unmanaged_apps() {
    let catalog = vec![
        "google-chrome".into(),
        "arc".into(),
        "cursor".into(),
        "docker-desktop".into(),
        "chatgpt".into(),
        "claude".into(),
        "kitty".into(),
        "telegram".into(),
        "whatsapp".into(),
        "zed".into(),
        "zen".into(),
        "rectangle".into(),
        "tailscale-app".into(),
        "ollama-app".into(),
        "logi-options+".into(),
        "helium-browser".into(),
        "grok-bot".into(),
        "t3-code".into(),
        "emacs-app".into(),
        "iterm2".into(),
        "zoom".into(),
        "omnidisksweeper".into(),
        "orca".into(),
        "helium".into(),
    ];

    let fixture = vec![
        scanned_app("Google Chrome", Some("com.google.Chrome")),
        scanned_app("Arc", Some("company.thebrowser.Browser")),
        scanned_app("Cursor", Some("com.todesktop.230313mzl4w4u92")),
        scanned_app("Docker", Some("com.docker.docker")),
        scanned_app("ChatGPT", Some("com.openai.codex")),
        scanned_app("Claude", Some("com.anthropic.claudefordesktop")),
        scanned_app("kitty", Some("net.kovidgoyal.kitty")),
        scanned_app("Telegram", Some("ru.keepcoder.Telegram")),
        scanned_app("WhatsApp", Some("net.whatsapp.WhatsApp")),
        scanned_app("Zed", Some("dev.zed.Zed")),
        scanned_app("Zen", Some("app.zen-browser.zen")),
        scanned_app("Rectangle", Some("com.knollsoft.Rectangle")),
        scanned_app("Tailscale", Some("io.tailscale.ipn.macsys")),
        scanned_app("Ollama", Some("com.electron.ollama")),
        scanned_app("logioptionsplus", Some("com.logi.optionsplus")),
        scanned_app("Helium", Some("net.imput.helium")),
        scanned_app("Grok Bot", Some("com.anysphere.sand")),
        scanned_app("T3 Code", Some("com.t3tools.t3code")),
        scanned_app("Emacs", None),
        scanned_app("iTerm", Some("com.googlecode.iterm2")),
        scanned_app("zoom.us", Some("us.zoom.xos")),
        scanned_app("OmniDiskSweeper", Some("com.omnigroup.OmniDiskSweeper")),
        scanned_app("Amphetamine", Some("com.if.Amphetamine")),
        scanned_app("ColorSlurp", Some("com.IdeaPunch.ColorSlurp")),
        scanned_app("Xcode", Some("com.apple.dt.Xcode")),
        scanned_app("Gestimer", Some("io.maddin.Gestimer")),
        scanned_app("PDFgear", Some("com.pdfeditor.pdfeditormac")),
        scanned_app("OneTab", Some("com.one-tab.OneTab")),
        scanned_app("DevCleaner", Some("com.oneminutegames.XcodeCleaner")),
        scanned_app("Safari", Some("com.apple.Safari")),
        scanned_app("Keynote", Some("com.apple.iWork.Keynote")),
        scanned_app("Pages", Some("com.apple.iWork.Pages")),
        scanned_app("Numbers", Some("com.apple.iWork.Numbers")),
        scanned_app("GarageBand", Some("com.apple.garageband10")),
        scanned_app("iMovie", Some("com.apple.iMovieApp")),
        scanned_app(
            "Karabiner-EventViewer",
            Some("org.pqrs.Karabiner-EventViewer"),
        ),
        scanned_app(
            "Claude Code URL Handler",
            Some("com.anthropic.claude-code-url-handler"),
        ),
        scanned_app("BlueStacksMIM", Some("com.now.gg.BlueStacksMIM")),
        scanned_app(
            "Steinberg Activation Manager",
            Some("com.steinberg.SteinbergActivationManager"),
        ),
        scanned_app("Among Us", None),
        scanned_app(
            "DaVinci Resolve",
            Some("com.blackmagic-design.DaVinciResolve"),
        ),
        scanned_app(
            "Blackmagic Proxy Generator Lite",
            Some("com.blackmagic-design.BlackmagicProxyGeneratorLite"),
        ),
        scanned_app("Eclipse", Some("org.eclipse.platform.ide")),
        scanned_app("Ivanti Secure Access", Some("net.pulsesecure.Pulse-Secure")),
        scanned_app("Orca", Some("com.stablyai.orca")),
    ];

    let mut cask_tokens = Vec::new();
    let mut mas_names = Vec::new();
    let mut promoted_casks = Vec::new();
    let mut promoted_mas = Vec::new();
    let mut unmanaged = Vec::new();
    let mut skipped_stock = 0usize;
    let mut skipped_helpers = 0usize;

    for app in &fixture {
        match classify_scanned_app(app, &catalog, &cask_tokens, &mas_names, true) {
            ScanClass::Stock => skipped_stock += 1,
            ScanClass::Helper => skipped_helpers += 1,
            ScanClass::AlreadyManaged => {}
            ScanClass::Mas(mas_app) => {
                mas_names.push(mas_app.name.clone());
                promoted_mas.push(mas_app);
            }
            ScanClass::Cask(cask) => {
                promoted_casks.push(cask.clone());
                cask_tokens.push(cask);
            }
            ScanClass::Unmanaged { homepage } => {
                unmanaged.push((app.name.clone(), homepage));
            }
        }
    }

    assert_eq!(
        promoted_casks,
        vec![
            "google-chrome",
            "arc",
            "cursor",
            "docker-desktop",
            "chatgpt",
            "claude",
            "kitty",
            "telegram",
            "whatsapp",
            "zed",
            "zen",
            "rectangle",
            "tailscale-app",
            "ollama-app",
            "logi-options+",
            "helium-browser",
            "grok-bot",
            "t3-code",
            "emacs-app",
            "iterm2",
            "zoom",
            "omnidisksweeper",
        ]
    );
    assert_eq!(
        promoted_mas
            .iter()
            .map(|mas| (mas.name.as_str(), mas.id))
            .collect::<Vec<_>>(),
        vec![
            ("Amphetamine", 937984704),
            ("ColorSlurp", 1287239339),
            ("Xcode", 497799835),
            ("Gestimer", 990588172),
            ("PDFgear", 6469021132),
            ("OneTab", 1540160809),
            ("DevCleaner", 1388020431),
        ]
    );
    assert_eq!(skipped_stock, 6);
    assert_eq!(skipped_helpers, 4);
    assert_eq!(
        unmanaged
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "Among Us",
            "DaVinci Resolve",
            "Blackmagic Proxy Generator Lite",
            "Eclipse",
            "Ivanti Secure Access",
            "Orca",
        ]
    );
    for (name, homepage) in &unmanaged {
        assert!(homepage.is_some(), "{name} leftover should have a homepage");
    }

    assert!(resolve_cask(&scanned_app("Orca", Some("com.stablyai.orca")), &catalog).is_none());
    assert_ne!(
        resolve_cask(
            &scanned_app("Helium", Some("net.imput.helium")),
            &["helium".into()]
        )
        .as_deref(),
        Some("helium")
    );
    assert!(resolve_mas(&scanned_app("Xcode", Some("com.apple.dt.Xcode"))).is_some());
    assert!(homepage_for(&scanned_app("Eclipse", Some("org.eclipse.platform.ide"))).is_some());
}

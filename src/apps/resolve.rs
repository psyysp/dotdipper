//! Map scanned /Applications bundles onto Homebrew casks, MAS ids, or a
//! manual-install homepage so restore can actually reinstall them.

use super::applications::{is_managed_app, InstalledApp};
use super::brew::normalize_token;
use super::MasApp;

/// How [`super::capture`] should treat a scanned `/Applications` bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanClass {
    Stock,
    Helper,
    AlreadyManaged,
    Mas(MasApp),
    Cask(String),
    Unmanaged { homepage: Option<String> },
}

/// Homebrew cask tokens that share a common app name with something else.
/// Exact name matching is skipped for these unless a bundle-id mapping hits.
const AMBIGUOUS_CASK_TOKENS: &[&str] = &[
    "orca",   // plotly/orca, not stablyai Orca
    "helium", // koush Helium, not the Helium browser
];

/// Stable bundle id → Homebrew cask (preferred over name matching).
const BUNDLE_TO_CASK: &[(&str, &str)] = &[
    ("cc.arduino.IDE2", "arduino-ide"),
    ("com.anthropic.claudefordesktop", "claude"),
    ("com.aptonic.Dropzone4", "dropzone"),
    ("com.bjango.istatmenus", "istat-menus"),
    ("com.docker.docker", "docker-desktop"),
    ("com.electron.ollama", "ollama-app"),
    ("com.google.antigravity", "antigravity"),
    ("com.google.Chrome", "google-chrome"),
    ("com.googlecode.iterm2", "iterm2"),
    ("com.kagi.kagimacOS", "orion"),
    ("com.knollsoft.Rectangle", "rectangle"),
    ("com.logi.optionsplus", "logi-options+"),
    ("com.microsoft.Excel", "microsoft-excel"),
    ("com.microsoft.Outlook", "microsoft-outlook"),
    ("com.microsoft.Powerpoint", "microsoft-powerpoint"),
    ("com.microsoft.teams", "microsoft-teams"),
    ("com.microsoft.Word", "microsoft-word"),
    ("com.now.gg.BlueStacks", "bluestacks"),
    ("com.openai.codex", "chatgpt"),
    ("com.superlist.superlist.app", "superlist"),
    ("com.todesktop.230313mzl4w4u92", "cursor"),
    ("com.tradingview.tradingviewapp.desktop", "tradingview"),
    ("company.thebrowser.Browser", "arc"),
    ("cx.c3.theunarchiver", "the-unarchiver"),
    ("dev.zed.Zed", "zed"),
    ("io.balena.etcher", "balenaetcher"),
    ("io.tailscale.ipn.macsys", "tailscale-app"),
    ("net.imput.helium", "helium-browser"),
    ("net.kovidgoyal.kitty", "kitty"),
    ("net.matthewpalmer.Rocket", "rocket"),
    ("net.whatsapp.WhatsApp", "whatsapp"),
    ("com.anysphere.sand", "grok-bot"),
    ("com.t3tools.t3code", "t3-code"),
    ("com.omnigroup.OmniDiskSweeper", "omnidisksweeper"),
    ("org.dolphin-emu.dolphin", "dolphin"),
    ("ru.keepcoder.Telegram", "telegram"),
    ("us.zoom.xos", "zoom"),
    ("app.zen-browser.zen", "zen"),
];

/// Normalized app name → cask, for bundles without a useful bundle id.
const NAME_TO_CASK: &[(&str, &str)] = &[
    ("iterm", "iterm2"),
    ("zoom-us", "zoom"),
    ("logioptionsplus", "logi-options+"),
    ("google-chrome", "google-chrome"),
    ("balenaetcher", "balenaetcher"),
    ("arduino-ide", "arduino-ide"),
    ("emacs", "emacs-app"),
    ("dropzone-4", "dropzone"),
    ("istat-menus-6", "istat-menus"),
];

/// Bundle id → Mac App Store id (apps not listed by `mas list` because they
/// were installed from the store UI rather than the mas CLI).
const BUNDLE_TO_MAS: &[(&str, &str, u64)] = &[
    ("com.if.Amphetamine", "Amphetamine", 937984704),
    ("com.IdeaPunch.ColorSlurp", "ColorSlurp", 1287239339),
    ("com.apple.dt.Xcode", "Xcode", 497799835),
    ("io.maddin.Gestimer", "Gestimer", 990588172),
    ("com.pdfeditor.pdfeditormac", "PDFgear", 6469021132),
    ("com.one-tab.OneTab", "OneTab", 1540160809),
    ("com.oneminutegames.XcodeCleaner", "DevCleaner", 1388020431),
];

/// Manual download pages for apps with no cask/MAS mapping.
const BUNDLE_TO_HOMEPAGE: &[(&str, &str)] = &[
    (
        "com.blackmagic-design.DaVinciResolve",
        "https://www.blackmagicdesign.com/products/davinciresolve",
    ),
    (
        "com.blackmagic-design.BlackmagicProxyGeneratorLite",
        "https://www.blackmagicdesign.com/products/blackmagicproxydsk",
    ),
    ("com.stablyai.orca", "https://orca.stably.ai/"),
    (
        "net.pulsesecure.Pulse-Secure",
        "https://www.ivanti.com/support/secure-access",
    ),
    (
        "com.steinberg.SteinbergActivationManager",
        "https://www.steinberg.net/licensing/",
    ),
    (
        "com.steinberg.HALionLibraryManager",
        "https://www.steinberg.net/licensing/",
    ),
    (
        "org.eclipse.platform.ide",
        "https://www.eclipse.org/downloads/",
    ),
];

const NAME_TO_HOMEPAGE: &[(&str, &str)] = &[
    (
        "Among Us",
        "https://store.steampowered.com/app/945360/Among_Us/",
    ),
    ("Emacs", "https://emacsformacosx.com/"),
];

/// Optional Apple apps that do *not* ship with macOS and should still be
/// restored (currently Xcode and related developer tools).
fn is_optional_apple_bundle(bundle_id: &str) -> bool {
    bundle_id.starts_with("com.apple.dt.")
}

/// Built-in Apple apps that come with macOS / iWork / iLife — skip them.
pub fn is_stock_apple_app(app: &InstalledApp) -> bool {
    let Some(bundle_id) = app.bundle_id.as_deref() else {
        return matches!(
            app.name.as_str(),
            "Safari" | "Mail" | "Preview" | "Photos" | "Music" | "TV" | "Podcasts"
        );
    };
    if is_optional_apple_bundle(bundle_id) {
        return false;
    }
    bundle_id.starts_with("com.apple.")
}

/// Companion / helper bundles that are installed as part of another app.
pub fn is_helper_app(app: &InstalledApp) -> bool {
    let name = app.name.as_str();
    if name.contains("EventViewer") {
        return true;
    }
    if name.contains("URL Handler") {
        return true;
    }
    if name.ends_with("MIM") && name.contains("BlueStacks") {
        return true;
    }
    if let Some(id) = app.bundle_id.as_deref() {
        if id.starts_with("com.steinberg.") && name.contains("Manager") {
            return true;
        }
    }
    false
}

/// Resolve a scanned app to a Homebrew cask token, if we can do so safely.
pub fn resolve_cask(app: &InstalledApp, available_casks: &[String]) -> Option<String> {
    if let Some(bundle_id) = app.bundle_id.as_deref() {
        if let Some((_, cask)) = BUNDLE_TO_CASK.iter().find(|(id, _)| *id == bundle_id) {
            if cask_available(cask, available_casks) {
                return Some((*cask).to_string());
            }
        }
    }

    let token = normalize_token(&app.name);
    if token.is_empty() {
        return None;
    }

    if let Some((_, cask)) = NAME_TO_CASK.iter().find(|(name, _)| *name == token) {
        if cask_available(cask, available_casks) {
            return Some((*cask).to_string());
        }
    }

    let candidates = [token.clone(), strip_trailing_version(&token)];
    for candidate in &candidates {
        if AMBIGUOUS_CASK_TOKENS.contains(&candidate.as_str()) {
            continue;
        }
        if cask_available(candidate, available_casks) {
            return Some(candidate.clone());
        }
    }

    None
}

/// Resolve a scanned app to a Mac App Store entry.
pub fn resolve_mas(app: &InstalledApp) -> Option<MasApp> {
    let bundle_id = app.bundle_id.as_deref()?;
    BUNDLE_TO_MAS
        .iter()
        .find(|(id, _, _)| *id == bundle_id)
        .map(|(_, name, mas_id)| MasApp {
            id: *mas_id,
            name: (*name).to_string(),
            version: app.version.clone().unwrap_or_default(),
        })
}

/// Classify a scanned app the same way [`super::capture`] does.
pub fn classify_scanned_app(
    app: &InstalledApp,
    available_casks: &[String],
    cask_tokens: &[String],
    mas_names: &[String],
    promote: bool,
) -> ScanClass {
    if is_stock_apple_app(app) {
        return ScanClass::Stock;
    }
    if is_helper_app(app) {
        return ScanClass::Helper;
    }
    if is_managed_app(&app.name, cask_tokens, mas_names) {
        return ScanClass::AlreadyManaged;
    }
    if promote {
        // MAS first so App Store–signed copies restore via mas, not a zip cask.
        if let Some(mas_app) = resolve_mas(app) {
            if mas_names.iter().any(|existing| existing == &mas_app.name) {
                return ScanClass::AlreadyManaged;
            }
            return ScanClass::Mas(mas_app);
        }
        if let Some(cask) = resolve_cask(app, available_casks) {
            if cask_tokens.iter().any(|existing| existing == &cask) {
                return ScanClass::AlreadyManaged;
            }
            return ScanClass::Cask(cask);
        }
    }
    ScanClass::Unmanaged {
        homepage: homepage_for(app),
    }
}

/// Homepage for apps that still need a manual download.
pub fn homepage_for(app: &InstalledApp) -> Option<String> {
    if let Some(bundle_id) = app.bundle_id.as_deref() {
        if let Some((_, url)) = BUNDLE_TO_HOMEPAGE.iter().find(|(id, _)| *id == bundle_id) {
            return Some((*url).to_string());
        }
    }
    NAME_TO_HOMEPAGE
        .iter()
        .find(|(name, _)| *name == app.name)
        .map(|(_, url)| (*url).to_string())
}

pub fn strip_trailing_version(token: &str) -> String {
    let mut parts: Vec<&str> = token.split('-').collect();
    if parts.len() >= 2
        && parts
            .last()
            .is_some_and(|p| p.chars().all(|c| c.is_ascii_digit()))
    {
        parts.pop();
        return parts.join("-");
    }
    token.to_string()
}

fn cask_available(cask: &str, available_casks: &[String]) -> bool {
    if available_casks.is_empty() {
        // Catalog lookup failed; still accept well-known mappings.
        return true;
    }
    available_casks.iter().any(|existing| existing == cask)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn app(name: &str, bundle_id: Option<&str>) -> InstalledApp {
        InstalledApp {
            name: name.into(),
            path: PathBuf::from(format!("/Applications/{name}.app")),
            bundle_id: bundle_id.map(str::to_string),
            version: Some("1.0".into()),
        }
    }

    /// Catalog containing the expected restore tokens plus ambiguous decoys.
    fn expected_cask_catalog() -> Vec<String> {
        [
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
            "dropzone",
            "istat-menus",
            "rocket",
            // Decoys that must not win over bundle-id / alias mappings.
            "orca",
            "helium",
        ]
        .into_iter()
        .map(str::to_string)
        .collect()
    }

    fn formerly_unmanaged_casks() -> Vec<(&'static str, Option<&'static str>, &'static str)> {
        vec![
            ("Google Chrome", Some("com.google.Chrome"), "google-chrome"),
            ("Arc", Some("company.thebrowser.Browser"), "arc"),
            ("Cursor", Some("com.todesktop.230313mzl4w4u92"), "cursor"),
            ("Docker", Some("com.docker.docker"), "docker-desktop"),
            ("ChatGPT", Some("com.openai.codex"), "chatgpt"),
            ("Claude", Some("com.anthropic.claudefordesktop"), "claude"),
            ("kitty", Some("net.kovidgoyal.kitty"), "kitty"),
            ("Telegram", Some("ru.keepcoder.Telegram"), "telegram"),
            ("WhatsApp", Some("net.whatsapp.WhatsApp"), "whatsapp"),
            ("Zed", Some("dev.zed.Zed"), "zed"),
            ("Zen", Some("app.zen-browser.zen"), "zen"),
            ("Rectangle", Some("com.knollsoft.Rectangle"), "rectangle"),
            (
                "Tailscale",
                Some("io.tailscale.ipn.macsys"),
                "tailscale-app",
            ),
            ("Ollama", Some("com.electron.ollama"), "ollama-app"),
            (
                "logioptionsplus",
                Some("com.logi.optionsplus"),
                "logi-options+",
            ),
            ("Helium", Some("net.imput.helium"), "helium-browser"),
            ("Grok Bot", Some("com.anysphere.sand"), "grok-bot"),
            ("T3 Code", Some("com.t3tools.t3code"), "t3-code"),
            ("Emacs", None, "emacs-app"),
            ("iTerm", Some("com.googlecode.iterm2"), "iterm2"),
            ("zoom.us", Some("us.zoom.xos"), "zoom"),
            (
                "OmniDiskSweeper",
                Some("com.omnigroup.OmniDiskSweeper"),
                "omnidisksweeper",
            ),
        ]
    }

    fn formerly_unmanaged_mas() -> Vec<(&'static str, &'static str, u64)> {
        vec![
            ("Amphetamine", "com.if.Amphetamine", 937984704),
            ("ColorSlurp", "com.IdeaPunch.ColorSlurp", 1287239339),
            ("Xcode", "com.apple.dt.Xcode", 497799835),
            ("Gestimer", "io.maddin.Gestimer", 990588172),
            ("PDFgear", "com.pdfeditor.pdfeditormac", 6469021132),
            ("OneTab", "com.one-tab.OneTab", 1540160809),
            ("DevCleaner", "com.oneminutegames.XcodeCleaner", 1388020431),
        ]
    }

    fn stock_skip_apps() -> Vec<(&'static str, &'static str)> {
        vec![
            ("Safari", "com.apple.Safari"),
            ("Keynote", "com.apple.iWork.Keynote"),
            ("Pages", "com.apple.iWork.Pages"),
            ("Numbers", "com.apple.iWork.Numbers"),
            ("GarageBand", "com.apple.garageband10"),
            ("iMovie", "com.apple.iMovieApp"),
        ]
    }

    fn helper_skip_apps() -> Vec<(&'static str, &'static str)> {
        vec![
            ("Karabiner-EventViewer", "org.pqrs.Karabiner-EventViewer"),
            (
                "Claude Code URL Handler",
                "com.anthropic.claude-code-url-handler",
            ),
            ("BlueStacksMIM", "com.now.gg.BlueStacksMIM"),
            (
                "Steinberg Activation Manager",
                "com.steinberg.SteinbergActivationManager",
            ),
        ]
    }

    fn leftover_apps() -> Vec<(&'static str, Option<&'static str>)> {
        vec![
            ("Among Us", None),
            (
                "DaVinci Resolve",
                Some("com.blackmagic-design.DaVinciResolve"),
            ),
            (
                "Blackmagic Proxy Generator Lite",
                Some("com.blackmagic-design.BlackmagicProxyGeneratorLite"),
            ),
            ("Eclipse", Some("org.eclipse.platform.ide")),
            ("Ivanti Secure Access", Some("net.pulsesecure.Pulse-Secure")),
            ("Orca", Some("com.stablyai.orca")),
        ]
    }

    #[test]
    fn stock_apple_apps_are_skipped() {
        for (name, bundle_id) in stock_skip_apps() {
            assert!(
                is_stock_apple_app(&app(name, Some(bundle_id))),
                "{name} ({bundle_id}) should be treated as stock Apple"
            );
        }
        assert!(!is_stock_apple_app(&app(
            "Xcode",
            Some("com.apple.dt.Xcode")
        )));
        assert!(!is_stock_apple_app(&app(
            "Chrome",
            Some("com.google.Chrome")
        )));
    }

    #[test]
    fn helper_apps_are_skipped() {
        for (name, bundle_id) in helper_skip_apps() {
            assert!(
                is_helper_app(&app(name, Some(bundle_id))),
                "{name} should be treated as a helper"
            );
        }
        assert!(!is_helper_app(&app(
            "Arc",
            Some("company.thebrowser.Browser")
        )));
    }

    #[test]
    fn formerly_unmanaged_casks_resolve_from_catalog() {
        let casks = expected_cask_catalog();
        for (name, bundle_id, expected) in formerly_unmanaged_casks() {
            assert_eq!(
                resolve_cask(&app(name, bundle_id), &casks).as_deref(),
                Some(expected),
                "{name} should promote to cask {expected}"
            );
        }
    }

    #[test]
    fn formerly_unmanaged_mas_ids_resolve() {
        for (name, bundle_id, expected_id) in formerly_unmanaged_mas() {
            let mas = resolve_mas(&app(name, Some(bundle_id)))
                .unwrap_or_else(|| panic!("{name} should resolve to MAS id {expected_id}"));
            assert_eq!(mas.id, expected_id);
            assert_eq!(mas.name, name);
        }
    }

    #[test]
    fn ambiguous_names_are_not_auto_promoted() {
        let decoys = vec!["orca".into(), "helium".into(), "rocket".into()];
        assert!(
            resolve_cask(&app("Orca", Some("com.stablyai.orca")), &decoys).is_none(),
            "Orca must not resolve to cask orca"
        );
        assert!(
            resolve_cask(&app("Helium", Some("net.imput.helium")), &decoys).is_none(),
            "Helium must not resolve to cask helium when that is the only token"
        );
        assert_eq!(
            resolve_cask(
                &app("Helium", Some("net.imput.helium")),
                &["helium-browser".into()]
            )
            .as_deref(),
            Some("helium-browser")
        );
        assert_ne!(
            resolve_cask(
                &app("Helium", Some("net.imput.helium")),
                &expected_cask_catalog()
            )
            .as_deref(),
            Some("helium")
        );
        assert_eq!(
            resolve_cask(&app("Rocket", Some("net.matthewpalmer.Rocket")), &decoys).as_deref(),
            Some("rocket")
        );
    }

    #[test]
    fn name_aliases_and_version_suffixes() {
        let casks = expected_cask_catalog();
        assert_eq!(
            resolve_cask(&app("iTerm", None), &casks).as_deref(),
            Some("iterm2")
        );
        assert_eq!(
            resolve_cask(&app("zoom.us", None), &casks).as_deref(),
            Some("zoom")
        );
        assert_eq!(
            resolve_cask(&app("logioptionsplus", None), &casks).as_deref(),
            Some("logi-options+")
        );
        assert_eq!(
            resolve_cask(&app("Dropzone 4", None), &casks).as_deref(),
            Some("dropzone")
        );
        assert_eq!(strip_trailing_version("istat-menus-6"), "istat-menus");
        assert_eq!(
            resolve_cask(&app("Emacs", None), &casks).as_deref(),
            Some("emacs-app")
        );
    }

    #[test]
    fn leftover_apps_have_homepages_and_no_cask_or_mas() {
        let casks = expected_cask_catalog();
        for (name, bundle_id) in leftover_apps() {
            let scanned = app(name, bundle_id);
            assert!(
                resolve_cask(&scanned, &casks).is_none(),
                "{name} should not promote to a cask"
            );
            assert!(
                resolve_mas(&scanned).is_none(),
                "{name} should not promote to MAS"
            );
            assert!(
                homepage_for(&scanned).is_some(),
                "{name} should have a homepage for leftover restore"
            );
        }
        assert_eq!(
            homepage_for(&app("Eclipse", Some("org.eclipse.platform.ide"))).as_deref(),
            Some("https://www.eclipse.org/downloads/")
        );
        assert_eq!(
            homepage_for(&app("Among Us", None)).as_deref(),
            Some("https://store.steampowered.com/app/945360/Among_Us/")
        );
    }

    #[test]
    fn classify_scanned_app_matches_capture_loop() {
        let catalog = expected_cask_catalog();
        let mut cask_tokens = Vec::new();
        let mut mas_names = Vec::new();
        let mut promoted_casks = Vec::new();
        let mut promoted_mas = Vec::new();
        let mut unmanaged = Vec::new();
        let mut skipped_stock = 0usize;
        let mut skipped_helpers = 0usize;
        let mut skipped_managed = 0usize;

        let mut scanned = Vec::new();
        for (name, bundle_id, _) in formerly_unmanaged_casks() {
            scanned.push(app(name, bundle_id));
        }
        for (name, bundle_id, _) in formerly_unmanaged_mas() {
            scanned.push(app(name, Some(bundle_id)));
        }
        for (name, bundle_id) in stock_skip_apps() {
            scanned.push(app(name, Some(bundle_id)));
        }
        for (name, bundle_id) in helper_skip_apps() {
            scanned.push(app(name, Some(bundle_id)));
        }
        for (name, bundle_id) in leftover_apps() {
            scanned.push(app(name, bundle_id));
        }
        // Duplicate kitty after the first promotion should count as already-managed.
        scanned.push(app("kitty", Some("net.kovidgoyal.kitty")));

        for scanned_app in &scanned {
            match classify_scanned_app(scanned_app, &catalog, &cask_tokens, &mas_names, true) {
                ScanClass::Stock => skipped_stock += 1,
                ScanClass::Helper => skipped_helpers += 1,
                ScanClass::AlreadyManaged => skipped_managed += 1,
                ScanClass::Mas(mas_app) => {
                    mas_names.push(mas_app.name.clone());
                    promoted_mas.push(mas_app);
                }
                ScanClass::Cask(cask) => {
                    promoted_casks.push(cask.clone());
                    cask_tokens.push(cask);
                }
                ScanClass::Unmanaged { homepage } => {
                    unmanaged.push((scanned_app.name.clone(), homepage));
                }
            }
        }

        let expected_casks: Vec<String> = formerly_unmanaged_casks()
            .into_iter()
            .map(|(_, _, cask)| cask.to_string())
            .collect();
        assert_eq!(promoted_casks, expected_casks);
        assert_eq!(
            skipped_managed, 1,
            "duplicate kitty should be already-managed"
        );

        let expected_mas: Vec<(&str, u64)> = formerly_unmanaged_mas()
            .into_iter()
            .map(|(name, _, id)| (name, id))
            .collect();
        let got_mas: Vec<(&str, u64)> = promoted_mas
            .iter()
            .map(|mas| (mas.name.as_str(), mas.id))
            .collect();
        assert_eq!(got_mas, expected_mas);

        assert_eq!(skipped_stock, stock_skip_apps().len());
        assert_eq!(skipped_helpers, helper_skip_apps().len());

        let leftover_names: Vec<&str> = leftover_apps().into_iter().map(|(name, _)| name).collect();
        assert_eq!(
            unmanaged
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            leftover_names
        );
        for (name, homepage) in &unmanaged {
            assert!(
                homepage.is_some(),
                "{name} leftover should carry a homepage"
            );
        }

        // Decoy tokens in the catalog must not leak into promotions.
        assert!(!promoted_casks.iter().any(|cask| cask == "orca"));
        assert!(!promoted_casks.iter().any(|cask| cask == "helium"));
    }
}

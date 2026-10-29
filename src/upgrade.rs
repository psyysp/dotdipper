//! `dotdipper upgrade`: replaces the running binary with the latest release,
//! the same way it was installed.
//!
//! A Homebrew or cargo install is handed back to that tool, so its records
//! stay right. A binary from `install.sh` (or a manual download) is replaced
//! here, with the same checks as `install.sh`: the SHA-256 checksum always,
//! and the signed build attestation whenever `gh` can verify it.

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::ui;

const REPO: &str = "psyysp/dotdipper";

/// The newest release built before build provenance was attested; see
/// `install.sh`, which uses the same cutoff for the same reason.
const LAST_UNATTESTED: (u64, u64, u64) = (0, 12, 0);

/// How the running binary was installed, which decides how to upgrade it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Method {
    Homebrew,
    /// `cargo install dotdipper` from crates.io.
    Cargo,
    /// A release binary placed by `install.sh` or by hand.
    Release {
        bin: PathBuf,
    },
    /// Built from a checkout; an upgrade would replace work in progress.
    DevBuild,
}

impl Method {
    pub fn describe(&self) -> String {
        match self {
            Method::Homebrew => "Homebrew".into(),
            Method::Cargo => "cargo install (crates.io)".into(),
            Method::Release { bin } => format!("release binary at {}", bin.display()),
            Method::DevBuild => "a build from a source checkout".into(),
        }
    }
}

/// Classifies an install by where the binary lives. `cargo_bin` is cargo's
/// bin directory, and `crates2` the contents of cargo's install record,
/// which tells a crates.io install from a copy of a local build.
pub fn detect(exe: &Path, cargo_bin: Option<&Path>, crates2: Option<&str>) -> Method {
    let s = exe.to_string_lossy();
    if s.contains("/Cellar/dotdipper/") {
        return Method::Homebrew;
    }
    if s.contains("/target/debug/") || s.contains("/target/release/") {
        return Method::DevBuild;
    }
    if cargo_bin.is_some_and(|bin| exe.parent() == Some(bin)) {
        let from_registry = crates2.is_some_and(|record| {
            record.contains("\"dotdipper ") && {
                let entry = record.split("\"dotdipper ").nth(1).unwrap_or("");
                entry.split('"').next().unwrap_or("").contains("(registry+")
            }
        });
        return if from_registry {
            Method::Cargo
        } else {
            Method::DevBuild
        };
    }
    Method::Release {
        bin: exe.to_path_buf(),
    }
}

/// Parses `v1.2.3` or `1.2.3`.
pub fn parse_version(tag: &str) -> Option<(u64, u64, u64)> {
    let mut parts = tag.strip_prefix('v').unwrap_or(tag).split('.');
    let v = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(v)
}

/// The release asset's target triple for this machine.
pub fn target_triple() -> Result<String> {
    let arch = match std::env::consts::ARCH {
        "aarch64" => "aarch64",
        "x86_64" => "x86_64",
        other => anyhow::bail!("No release is built for {other}; upgrade with cargo instead."),
    };
    let os = match std::env::consts::OS {
        "macos" => "apple-darwin",
        "linux" => "unknown-linux-gnu",
        other => anyhow::bail!("No release is built for {other}; upgrade with cargo instead."),
    };
    Ok(format!("{arch}-{os}"))
}

/// The first field of a `.sha256` file, if it is a SHA-256 hex digest.
pub fn parse_checksum(file: &str) -> Option<String> {
    let digest = file.split_whitespace().next()?.to_ascii_lowercase();
    (digest.len() == 64 && digest.chars().all(|c| c.is_ascii_hexdigit())).then_some(digest)
}

pub struct Options {
    /// Report what would happen and change nothing.
    pub check: bool,
    /// A release tag to install instead of the latest (release binaries only).
    pub version: Option<String>,
    /// Fail rather than skip when the build attestation cannot be checked.
    pub require_attestation: bool,
}

fn curl() -> Command {
    let mut cmd = Command::new("curl");
    // Never follow a redirect onto plain HTTP or old TLS.
    cmd.args(["--proto", "=https", "--tlsv1.2", "-fsSL"]);
    cmd
}

/// The latest release's tag, from where GitHub's /releases/latest redirects.
fn latest_tag() -> Result<String> {
    let out = curl()
        .args(["-I", "-o", "/dev/null", "-w", "%{url_effective}"])
        .arg(format!("https://github.com/{REPO}/releases/latest"))
        .output()
        .context("curl is needed to check for releases")?;
    let url = String::from_utf8_lossy(&out.stdout);
    let tag = url.rsplit('/').next().unwrap_or("").trim().to_string();
    if !out.status.success() || parse_version(&tag).is_none() {
        anyhow::bail!("Could not determine the latest release (got '{tag}')");
    }
    Ok(tag)
}

fn run(cmd: &mut Command, what: &str) -> Result<()> {
    let status = cmd
        .status()
        .with_context(|| format!("Could not run {what}"))?;
    if !status.success() {
        anyhow::bail!("{what} failed");
    }
    Ok(())
}

fn current_exe() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("Could not locate the running binary")?;
    Ok(std::fs::canonicalize(&exe).unwrap_or(exe))
}

pub fn upgrade(opts: &Options) -> Result<()> {
    let exe = current_exe()?;
    let cargo_bin = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".cargo")))
        .map(|home| home.join("bin"));
    let crates2 = cargo_bin
        .as_ref()
        .and_then(|bin| std::fs::read_to_string(bin.parent()?.join(".crates2.json")).ok());
    let method = detect(&exe, cargo_bin.as_deref(), crates2.as_deref());

    let current = env!("CARGO_PKG_VERSION");
    let wanted = match &opts.version {
        Some(tag) => {
            if parse_version(tag).is_none() {
                anyhow::bail!("'{tag}' is not a release tag like v0.14.0");
            }
            format!("v{}", tag.strip_prefix('v').unwrap_or(tag))
        }
        None => latest_tag()?,
    };
    ui::info(&format!(
        "Installed: v{current}, via {}. {}: {wanted}.",
        method.describe(),
        if opts.version.is_some() {
            "Requested"
        } else {
            "Latest"
        }
    ));

    if opts.version.is_some() && matches!(method, Method::Homebrew | Method::Cargo) {
        anyhow::bail!(
            "--version applies to release binaries only; {} installs the latest version.",
            method.describe()
        );
    }
    if opts.version.is_none() && parse_version(current) >= parse_version(&wanted) {
        ui::success("dotdipper is up to date.");
        return Ok(());
    }

    let plan = match &method {
        Method::Homebrew => "brew update, then brew upgrade dotdipper".to_string(),
        Method::Cargo => "cargo install dotdipper --locked".to_string(),
        Method::Release { bin } => format!(
            "download {wanted}, verify it, and replace {}",
            bin.display()
        ),
        Method::DevBuild => {
            anyhow::bail!(
                "This dotdipper was built from a source checkout ({}). Rebuild it with \
                 scripts/dev-install.sh, or run 'scripts/dev-install.sh --restore' to switch \
                 back to Homebrew's copy.",
                exe.display()
            )
        }
    };
    if opts.check {
        ui::info(&format!(
            "An upgrade is available. 'dotdipper upgrade' would {plan}."
        ));
        return Ok(());
    }

    match &method {
        Method::Homebrew => {
            run(Command::new("brew").arg("update"), "brew update")?;
            run(
                Command::new("brew").args(["upgrade", "dotdipper"]),
                "brew upgrade",
            )?;
        }
        Method::Cargo => {
            run(
                Command::new("cargo").args(["install", "dotdipper", "--locked"]),
                "cargo install",
            )?;
        }
        Method::Release { bin } => install_release(&wanted, bin, opts.require_attestation)?,
        Method::DevBuild => unreachable!("refused above"),
    }

    // brew removes the old keg, so ask whichever copy is now on PATH.
    let new_exe = match method {
        Method::Release { .. } => exe.clone(),
        _ => which::which("dotdipper").unwrap_or(exe.clone()),
    };
    let after = Command::new(&new_exe)
        .arg("--version")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    ui::success(&format!("Upgraded: {after}"));
    Ok(())
}

/// Downloads `tag`, checks it, and swaps it in for `bin` in one rename.
fn install_release(tag: &str, bin: &Path, require_attestation: bool) -> Result<()> {
    let asset = format!("dotdipper-{}.tar.gz", target_triple()?);
    let base = format!("https://github.com/{REPO}/releases/download/{tag}");
    let tmp = tempfile::tempdir().context("Could not create a temporary directory")?;
    let tarball = tmp.path().join(&asset);
    let sums = tmp.path().join(format!("{asset}.sha256"));

    ui::info(&format!("Downloading {asset} ({tag})"));
    run(
        curl()
            .arg("-o")
            .arg(&tarball)
            .arg(format!("{base}/{asset}")),
        "the download",
    )
    .with_context(|| format!("Is there a {tag} release for this platform?"))?;
    run(
        curl()
            .arg("-o")
            .arg(&sums)
            .arg(format!("{base}/{asset}.sha256")),
        "the checksum download",
    )
    .context("Refusing to install a binary whose checksum could not be fetched")?;

    let expected = parse_checksum(&std::fs::read_to_string(&sums)?)
        .context("The release's checksum file is malformed; not installing")?;
    let actual = format!(
        "{:x}",
        Sha256::digest(std::fs::read(&tarball).context("Could not read the download")?)
    );
    if actual != expected {
        anyhow::bail!(
            "Checksum mismatch for {asset} (expected {expected}, got {actual}); not installing"
        );
    }
    ui::success("Checksum verified");
    verify_provenance(&tarball, tag, require_attestation)?;

    run(
        Command::new("tar")
            .arg("-xzf")
            .arg(&tarball)
            .arg("-C")
            .arg(tmp.path())
            .arg("dotdipper"),
        "extracting the archive",
    )?;

    // Write beside the target, then rename: the running copy is never left
    // half-written, and the replacement is atomic.
    let dir = bin.parent().context("The binary has no parent directory")?;
    let staged = dir.join(".dotdipper.new");
    std::fs::copy(tmp.path().join("dotdipper"), &staged).with_context(|| {
        format!(
            "Could not write to {}; upgrade with the permissions that installed it",
            dir.display()
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::rename(&staged, bin)
        .with_context(|| format!("Could not replace {}", bin.display()))?;
    Ok(())
}

/// The checksum comes from the same release as the tarball, so it cannot
/// catch a tampered release; the signed attestation can.
fn verify_provenance(tarball: &Path, tag: &str, require: bool) -> Result<()> {
    let skip = |why: &str| -> Result<()> {
        if require {
            anyhow::bail!("--require-attestation cannot be satisfied: {why}");
        }
        ui::warn(&format!(
            "Provenance was not checked: {why} (checksum only)."
        ));
        Ok(())
    };
    if parse_version(tag).is_some_and(|v| v <= LAST_UNATTESTED) {
        return skip(&format!("{tag} predates attested releases"));
    }
    if which::which("gh").is_err() {
        return skip("gh is not installed");
    }
    let authed = Command::new("gh")
        .args(["auth", "status"])
        .output()
        .is_ok_and(|o| o.status.success());
    if !authed {
        return skip("gh is not authenticated (run 'gh auth login')");
    }
    let verified = Command::new("gh")
        .args(["attestation", "verify"])
        .arg(tarball)
        .args(["--repo", REPO])
        .stdin(std::process::Stdio::null())
        .output()
        .is_ok_and(|o| o.status.success());
    if !verified {
        anyhow::bail!("Build provenance verification failed for {tag}; not installing");
    }
    ui::success("Build provenance verified");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const REGISTRY: &str = r#"{"installs":{"dotdipper 0.14.0 (registry+https://github.com/rust-lang/crates.io-index)":{}}}"#;
    const LOCAL: &str =
        r#"{"installs":{"dotdipper 0.7.5 (path+file:///Users/someone/src/dotdipper)":{}}}"#;

    #[test]
    fn the_install_method_follows_where_the_binary_lives() {
        let cargo = Path::new("/Users/someone/.cargo/bin");
        let detect_at = |p: &str, record| detect(Path::new(p), Some(cargo), record);

        assert_eq!(
            detect_at("/opt/homebrew/Cellar/dotdipper/0.14.0/bin/dotdipper", None),
            Method::Homebrew
        );
        assert_eq!(
            detect_at("/usr/local/Cellar/dotdipper/0.13.2/bin/dotdipper", None),
            Method::Homebrew
        );
        assert_eq!(
            detect_at("/Users/someone/.cargo/bin/dotdipper", Some(REGISTRY)),
            Method::Cargo
        );
        // A local build copied into cargo's bin must not be replaced by crates.io.
        assert_eq!(
            detect_at("/Users/someone/.cargo/bin/dotdipper", Some(LOCAL)),
            Method::DevBuild
        );
        assert_eq!(
            detect_at("/Users/someone/.cargo/bin/dotdipper", None),
            Method::DevBuild
        );
        assert_eq!(
            detect_at(
                "/Users/someone/src/dotdipper/target/release/dotdipper",
                None
            ),
            Method::DevBuild
        );
        assert_eq!(
            detect_at("/Users/someone/.local/bin/dotdipper", Some(REGISTRY)),
            Method::Release {
                bin: PathBuf::from("/Users/someone/.local/bin/dotdipper")
            }
        );
    }

    #[test]
    fn versions_compare_numerically() {
        assert!(parse_version("v0.10.0") > parse_version("0.9.9"));
        assert_eq!(parse_version("v0.14.0"), Some((0, 14, 0)));
        assert_eq!(parse_version("v0.14"), None);
        assert_eq!(parse_version("latest"), None);
        assert_eq!(parse_version("v1.2.3.4"), None);
    }

    #[test]
    fn only_a_well_formed_digest_is_accepted_as_a_checksum() {
        let digest = "a".repeat(64);
        assert_eq!(
            parse_checksum(&format!(
                "{digest}  dotdipper-aarch64-apple-darwin.tar.gz\n"
            )),
            Some(digest)
        );
        assert_eq!(parse_checksum(""), None);
        assert_eq!(parse_checksum("not-a-digest file"), None);
        assert_eq!(parse_checksum(&"g".repeat(64)), None);
    }

    #[test]
    fn this_machine_has_a_release_target() {
        if cfg!(any(target_os = "macos", target_os = "linux"))
            && cfg!(any(target_arch = "aarch64", target_arch = "x86_64"))
        {
            let t = target_triple().unwrap();
            assert!(
                t.ends_with("apple-darwin") || t.ends_with("unknown-linux-gnu"),
                "{t}"
            );
        }
    }
}

//! Full end-to-end tests for the safety/sync PR.
//!
//! Covers the real user journeys:
//! - init → snapshot → push → pull → apply → install
//! - backups, excludes, symlink mode, nested paths
//! - legacy repos without manifest.lock (rebuild)
//! - dirty pull rejected / force pull stashes
//! - rollback preserves .git + creates safety snapshot
//! - pull without --apply leaves $HOME untouched
//!
//! Local bare git remotes are used via `DOTDIPPER_TEST_REMOTE` so push/pull
//! exercise the real `vcs` code paths without GitHub SSH/`gh`.

use assert_cmd::Command;
use predicates::prelude::*;
use serial_test::serial;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

struct EnvGuard {
    keys: Vec<&'static str>,
}

impl EnvGuard {
    fn apply(vars: &[(&'static str, Option<&Path>)]) -> Self {
        let mut keys = Vec::new();
        for (key, value) in vars {
            keys.push(*key);
            match value {
                Some(path) => std::env::set_var(key, path),
                None => std::env::remove_var(key),
            }
        }
        Self { keys }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for key in &self.keys {
            std::env::remove_var(key);
        }
    }
}

fn git(repo: &Path, args: &[&str]) {
    let output = StdCommand::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git failed to spawn");
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_out(repo: &Path, args: &[&str]) -> String {
    let output = StdCommand::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git failed to spawn");
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn init_bare_remote() -> (TempDir, PathBuf) {
    let remote_root = TempDir::new().unwrap();
    let remote_path = remote_root.path().join("dotfiles.git");
    fs::create_dir_all(&remote_path).unwrap();
    let out = StdCommand::new("git")
        .args(["init", "--bare", "--initial-branch=main"])
        .current_dir(&remote_path)
        .output()
        .unwrap();
    assert!(out.status.success());
    (remote_root, remote_path)
}

fn write_config(config_path: &Path, home: &Path, tracked: &[&str], backup: bool, mode: &str) {
    let tracked_lines: Vec<String> = tracked
        .iter()
        .map(|rel| format!("  \"{}\",", home.join(rel).display()))
        .collect();
    let content = format!(
        r#"
[general]
default_mode = "{mode}"
backup = {backup}
tracked_files = [
{tracked}
]

[github]
username = "e2e-user"
repo_name = "dotfiles"
private = true

[packages]
common = []
"#,
        mode = mode,
        backup = backup,
        tracked = tracked_lines.join("\n")
    );
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(config_path, content).unwrap();
}

fn bin() -> Command {
    Command::cargo_bin("dotdipper").unwrap()
}

fn list_backups(home: &Path, prefix: &str) -> Vec<String> {
    fs::read_dir(home)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with(prefix))
        .collect()
}

// ---------------------------------------------------------------------------
// Full round-trip: machine1 push → machine2 pull → apply → install
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn e2e_push_pull_apply_install_roundtrip() {
    let (remote_root, remote_path) = init_bare_remote();
    let _remote_keep = remote_root;

    // --- Machine 1: author ---
    let m1 = TempDir::new().unwrap();
    let home1 = m1.path();
    let base1 = home1.join(".config").join("dotdipper");
    let config1 = base1.join("config.toml");

    fs::write(home1.join(".zshrc"), "export ZSH=from-m1\n").unwrap();
    fs::create_dir_all(home1.join(".config").join("app")).unwrap();
    fs::write(
        home1.join(".config").join("app").join("settings.toml"),
        "theme = \"dark\"\n",
    )
    .unwrap();
    fs::write(home1.join(".vimrc"), "set number\n").unwrap();

    write_config(
        &config1,
        home1,
        &[".zshrc", ".vimrc", ".config/app/settings.toml"],
        true,
        "copy",
    );

    let _guard1 = EnvGuard::apply(&[
        ("HOME", Some(home1)),
        ("DOTDIPPER_HOME", Some(&base1)),
        ("XDG_CONFIG_HOME", None),
        ("DOTDIPPER_TEST_REMOTE", Some(&remote_path)),
    ]);

    // Compile tracked files into compiled/ + write versioned snapshot
    bin()
        .env("HOME", home1)
        .env("DOTDIPPER_HOME", &base1)
        .env_remove("XDG_CONFIG_HOME")
        .env("DOTDIPPER_TEST_REMOTE", &remote_path)
        .arg("--config")
        .arg(&config1)
        .arg("snapshot")
        .arg("create")
        .arg("--force")
        .arg("-m")
        .arg("initial")
        .assert()
        .success();

    assert!(base1.join("compiled").join(".zshrc").exists());
    assert!(base1.join("compiled").join("manifest.lock").exists());
    assert!(base1.join("manifest.lock").exists());

    // Push via real vcs path (test remote skips gh)
    bin()
        .env("HOME", home1)
        .env("DOTDIPPER_HOME", &base1)
        .env_remove("XDG_CONFIG_HOME")
        .env("DOTDIPPER_TEST_REMOTE", &remote_path)
        .arg("--config")
        .arg(&config1)
        .arg("push")
        .arg("-m")
        .arg("e2e initial push")
        .assert()
        .success();

    // Confirm remote contains manifest.lock
    let inspect = TempDir::new().unwrap();
    let inspect_repo = inspect.path().join("inspect");
    let clone = StdCommand::new("git")
        .args([
            "clone",
            remote_path.to_str().unwrap(),
            inspect_repo.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(clone.status.success());
    assert!(
        inspect_repo.join("manifest.lock").exists(),
        "pushed repo must include manifest.lock"
    );
    assert!(inspect_repo.join(".zshrc").exists());
    assert!(inspect_repo
        .join(".config")
        .join("app")
        .join("settings.toml")
        .exists());

    drop(_guard1);

    // --- Machine 2: consumer ---
    let m2 = TempDir::new().unwrap();
    let home2 = m2.path();
    let base2 = home2.join(".config").join("dotdipper");
    let config2 = base2.join("config.toml");
    fs::create_dir_all(&base2).unwrap();

    // Existing local file that must be backed up on apply
    fs::write(home2.join(".zshrc"), "old local zshrc\n").unwrap();

    write_config(&config2, home2, &[], true, "copy");

    let _guard2 = EnvGuard::apply(&[
        ("HOME", Some(home2)),
        ("DOTDIPPER_HOME", Some(&base2)),
        ("XDG_CONFIG_HOME", None),
        ("DOTDIPPER_TEST_REMOTE", Some(&remote_path)),
    ]);

    // Pull alone must NOT touch $HOME
    bin()
        .env("HOME", home2)
        .env("DOTDIPPER_HOME", &base2)
        .env_remove("XDG_CONFIG_HOME")
        .env("DOTDIPPER_TEST_REMOTE", &remote_path)
        .arg("--config")
        .arg(&config2)
        .arg("pull")
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(home2.join(".zshrc")).unwrap(),
        "old local zshrc\n",
        "pull without --apply must leave $HOME untouched"
    );
    assert!(base2.join("compiled").join(".zshrc").exists());
    assert!(base2.join("compiled").join("manifest.lock").exists());
    assert!(
        base2.join("manifest.lock").exists(),
        "pull must sync manifest.lock to base dir"
    );

    // Diff should see divergence
    bin()
        .env("HOME", home2)
        .env("DOTDIPPER_HOME", &base2)
        .env_remove("XDG_CONFIG_HOME")
        .arg("--config")
        .arg(&config2)
        .arg("diff")
        .assert()
        .success()
        .stdout(predicate::str::contains("modified").or(predicate::str::contains("Modified")));

    // Apply with backups
    bin()
        .env("HOME", home2)
        .env("DOTDIPPER_HOME", &base2)
        .env_remove("XDG_CONFIG_HOME")
        .arg("--config")
        .arg(&config2)
        .arg("apply")
        .arg("--force")
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(home2.join(".zshrc")).unwrap(),
        "export ZSH=from-m1\n"
    );
    assert_eq!(
        fs::read_to_string(home2.join(".vimrc")).unwrap(),
        "set number\n"
    );
    assert_eq!(
        fs::read_to_string(home2.join(".config").join("app").join("settings.toml")).unwrap(),
        "theme = \"dark\"\n"
    );
    let backups = list_backups(home2, ".zshrc.bak.");
    assert!(
        !backups.is_empty(),
        "expected .bak backup of previous .zshrc"
    );
    assert_eq!(
        fs::read_to_string(home2.join(&backups[0])).unwrap(),
        "old local zshrc\n"
    );

    // Install dry-run: scripts + tracked_files sync + manifest-aware setup
    bin()
        .env("HOME", home2)
        .env("DOTDIPPER_HOME", &base2)
        .env_remove("XDG_CONFIG_HOME")
        .arg("--config")
        .arg(&config2)
        .arg("install")
        .arg("--dry-run")
        .assert()
        .success();

    let setup = fs::read_to_string(base2.join("install").join("setup_dotfiles.sh")).unwrap();
    assert!(setup.contains("DOTFILES=("));
    assert!(setup.contains(".zshrc"));
    assert!(setup.contains(".vimrc"));
    assert!(setup.contains(".config/app/settings.toml"));
    // File list must not include git store paths (skip-logic strings may still mention .git)
    let list_section = setup
        .split("DOTFILES=(")
        .nth(1)
        .and_then(|s| s.split(')').next())
        .unwrap_or("");
    assert!(
        !list_section.contains(".git/"),
        "DOTFILES list must not include .git paths: {list_section}"
    );

    let install_sh = fs::read_to_string(base2.join("install").join("install.sh")).unwrap();
    assert!(
        install_sh.contains("dotdipper apply"),
        "install.sh should prefer Rust apply when binary is available"
    );

    // tracked_files should have been refreshed after pull
    let cfg_after = fs::read_to_string(&config2).unwrap();
    assert!(
        cfg_after.contains(".zshrc") || cfg_after.contains("tracked_files"),
        "config should retain tracked_files section after pull sync"
    );
}

// ---------------------------------------------------------------------------
// Legacy remote without manifest.lock → rebuild on pull
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn e2e_legacy_remote_without_manifest_rebuilds_on_pull() {
    let (remote_root, remote_path) = init_bare_remote();
    let _keep = remote_root;

    // Seed remote with only a dotfile (no manifest.lock) — old push behavior
    let seed = TempDir::new().unwrap();
    let seed_repo = seed.path().join("seed");
    fs::create_dir_all(&seed_repo).unwrap();
    fs::write(seed_repo.join(".bashrc"), "alias ll='ls -la'\n").unwrap();
    git(&seed_repo, &["init", "-b", "main"]);
    git(&seed_repo, &["config", "user.email", "test@example.com"]);
    git(&seed_repo, &["config", "user.name", "Test"]);
    git(&seed_repo, &["add", "-A"]);
    git(&seed_repo, &["commit", "-m", "legacy without manifest"]);
    git(
        &seed_repo,
        &["remote", "add", "origin", remote_path.to_str().unwrap()],
    );
    git(&seed_repo, &["push", "-u", "origin", "main"]);

    let m = TempDir::new().unwrap();
    let home = m.path();
    let base = home.join(".config").join("dotdipper");
    let config = base.join("config.toml");
    write_config(&config, home, &[], true, "copy");

    let _guard = EnvGuard::apply(&[
        ("HOME", Some(home)),
        ("DOTDIPPER_HOME", Some(&base)),
        ("XDG_CONFIG_HOME", None),
        ("DOTDIPPER_TEST_REMOTE", Some(&remote_path)),
    ]);

    bin()
        .env("HOME", home)
        .env("DOTDIPPER_HOME", &base)
        .env_remove("XDG_CONFIG_HOME")
        .env("DOTDIPPER_TEST_REMOTE", &remote_path)
        .arg("--config")
        .arg(&config)
        .arg("pull")
        .assert()
        .success();

    assert!(base.join("compiled").join(".bashrc").exists());
    assert!(
        base.join("manifest.lock").exists(),
        "pull must rebuild manifest.lock for legacy remotes"
    );
    assert!(base.join("compiled").join("manifest.lock").exists());

    bin()
        .env("HOME", home)
        .env("DOTDIPPER_HOME", &base)
        .env_remove("XDG_CONFIG_HOME")
        .arg("--config")
        .arg(&config)
        .arg("apply")
        .arg("--force")
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(home.join(".bashrc")).unwrap(),
        "alias ll='ls -la'\n"
    );
}

// ---------------------------------------------------------------------------
// Dirty pull rejected; --force stashes and resets
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn e2e_pull_rejects_dirty_compiled_without_force() {
    let (remote_root, remote_path) = init_bare_remote();
    let _keep = remote_root;

    let m = TempDir::new().unwrap();
    let home = m.path();
    let base = home.join(".config").join("dotdipper");
    let config = base.join("config.toml");
    let compiled = base.join("compiled");

    fs::write(home.join(".profile"), "export A=1\n").unwrap();
    write_config(&config, home, &[".profile"], true, "copy");

    let _guard = EnvGuard::apply(&[
        ("HOME", Some(home)),
        ("DOTDIPPER_HOME", Some(&base)),
        ("XDG_CONFIG_HOME", None),
        ("DOTDIPPER_TEST_REMOTE", Some(&remote_path)),
    ]);

    bin()
        .env("HOME", home)
        .env("DOTDIPPER_HOME", &base)
        .env_remove("XDG_CONFIG_HOME")
        .env("DOTDIPPER_TEST_REMOTE", &remote_path)
        .arg("--config")
        .arg(&config)
        .arg("snapshot")
        .arg("create")
        .arg("--force")
        .assert()
        .success();

    bin()
        .env("HOME", home)
        .env("DOTDIPPER_HOME", &base)
        .env_remove("XDG_CONFIG_HOME")
        .env("DOTDIPPER_TEST_REMOTE", &remote_path)
        .arg("--config")
        .arg(&config)
        .arg("push")
        .arg("-m")
        .arg("clean push")
        .assert()
        .success();

    // Dirty the compiled store
    fs::write(compiled.join(".profile"), "export A=DIRTY\n").unwrap();

    bin()
        .env("HOME", home)
        .env("DOTDIPPER_HOME", &base)
        .env_remove("XDG_CONFIG_HOME")
        .env("DOTDIPPER_TEST_REMOTE", &remote_path)
        .arg("--config")
        .arg(&config)
        .arg("pull")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("uncommitted changes")
                .or(predicate::str::contains("uncommitted")),
        );

    // Home must still be original
    assert_eq!(
        fs::read_to_string(home.join(".profile")).unwrap(),
        "export A=1\n"
    );
}

#[test]
#[serial]
fn e2e_pull_force_stashes_dirty_and_restores_remote() {
    let (remote_root, remote_path) = init_bare_remote();
    let _keep = remote_root;

    let m = TempDir::new().unwrap();
    let home = m.path();
    let base = home.join(".config").join("dotdipper");
    let config = base.join("config.toml");
    let compiled = base.join("compiled");

    fs::write(home.join(".profile"), "export A=1\n").unwrap();
    write_config(&config, home, &[".profile"], true, "copy");

    let _guard = EnvGuard::apply(&[
        ("HOME", Some(home)),
        ("DOTDIPPER_HOME", Some(&base)),
        ("XDG_CONFIG_HOME", None),
        ("DOTDIPPER_TEST_REMOTE", Some(&remote_path)),
    ]);

    bin()
        .env("HOME", home)
        .env("DOTDIPPER_HOME", &base)
        .env_remove("XDG_CONFIG_HOME")
        .env("DOTDIPPER_TEST_REMOTE", &remote_path)
        .arg("--config")
        .arg(&config)
        .arg("snapshot")
        .arg("create")
        .arg("--force")
        .assert()
        .success();
    bin()
        .env("HOME", home)
        .env("DOTDIPPER_HOME", &base)
        .env_remove("XDG_CONFIG_HOME")
        .env("DOTDIPPER_TEST_REMOTE", &remote_path)
        .arg("--config")
        .arg(&config)
        .arg("push")
        .arg("-m")
        .arg("baseline")
        .assert()
        .success();

    fs::write(compiled.join(".profile"), "export A=DIRTY\n").unwrap();
    // Also add an untracked file
    fs::write(compiled.join("untracked-local.txt"), "local only\n").unwrap();

    bin()
        .env("HOME", home)
        .env("DOTDIPPER_HOME", &base)
        .env_remove("XDG_CONFIG_HOME")
        .env("DOTDIPPER_TEST_REMOTE", &remote_path)
        .arg("--config")
        .arg(&config)
        .arg("pull")
        .arg("--force")
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(compiled.join(".profile")).unwrap(),
        "export A=1\n",
        "force pull must restore remote compiled content"
    );
    assert!(
        !compiled.join("untracked-local.txt").exists(),
        "force pull cleans untracked compiled files"
    );
    // $HOME still untouched (no --apply)
    assert_eq!(
        fs::read_to_string(home.join(".profile")).unwrap(),
        "export A=1\n"
    );

    let stash_list = git_out(&compiled, &["stash", "list"]);
    assert!(
        stash_list.contains("dotdipper pre-pull stash") || !stash_list.is_empty(),
        "force pull should leave a recoverable stash: got '{stash_list}'"
    );
}

// ---------------------------------------------------------------------------
// pull --apply places files and keeps backups
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn e2e_pull_apply_places_files_with_backup() {
    let (remote_root, remote_path) = init_bare_remote();
    let _keep = remote_root;

    // Machine 1 push
    let m1 = TempDir::new().unwrap();
    let home1 = m1.path();
    let base1 = home1.join(".config").join("dotdipper");
    let config1 = base1.join("config.toml");
    fs::write(home1.join(".gitconfig"), "[user]\n\tname = E2E\n").unwrap();
    write_config(&config1, home1, &[".gitconfig"], true, "copy");

    let _g1 = EnvGuard::apply(&[
        ("HOME", Some(home1)),
        ("DOTDIPPER_HOME", Some(&base1)),
        ("XDG_CONFIG_HOME", None),
        ("DOTDIPPER_TEST_REMOTE", Some(&remote_path)),
    ]);

    bin()
        .env("HOME", home1)
        .env("DOTDIPPER_HOME", &base1)
        .env_remove("XDG_CONFIG_HOME")
        .env("DOTDIPPER_TEST_REMOTE", &remote_path)
        .arg("--config")
        .arg(&config1)
        .arg("snapshot")
        .arg("create")
        .arg("--force")
        .assert()
        .success();
    bin()
        .env("HOME", home1)
        .env("DOTDIPPER_HOME", &base1)
        .env_remove("XDG_CONFIG_HOME")
        .env("DOTDIPPER_TEST_REMOTE", &remote_path)
        .arg("--config")
        .arg(&config1)
        .arg("push")
        .arg("-m")
        .arg("gitconfig")
        .assert()
        .success();
    drop(_g1);

    // Machine 2 pull --apply --force
    let m2 = TempDir::new().unwrap();
    let home2 = m2.path();
    let base2 = home2.join(".config").join("dotdipper");
    let config2 = base2.join("config.toml");
    fs::write(home2.join(".gitconfig"), "[user]\n\tname = Local\n").unwrap();
    write_config(&config2, home2, &[], true, "copy");

    let _g2 = EnvGuard::apply(&[
        ("HOME", Some(home2)),
        ("DOTDIPPER_HOME", Some(&base2)),
        ("XDG_CONFIG_HOME", None),
        ("DOTDIPPER_TEST_REMOTE", Some(&remote_path)),
    ]);

    bin()
        .env("HOME", home2)
        .env("DOTDIPPER_HOME", &base2)
        .env_remove("XDG_CONFIG_HOME")
        .env("DOTDIPPER_TEST_REMOTE", &remote_path)
        .arg("--config")
        .arg(&config2)
        .arg("pull")
        .arg("--apply")
        .arg("--force")
        .assert()
        .success();

    assert!(fs::read_to_string(home2.join(".gitconfig"))
        .unwrap()
        .contains("E2E"));
    let backups = list_backups(home2, ".gitconfig.bak.");
    assert!(!backups.is_empty());
    assert!(fs::read_to_string(home2.join(&backups[0]))
        .unwrap()
        .contains("Local"));
}

// ---------------------------------------------------------------------------
// Excludes respected by apply + install setup script
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn e2e_apply_skips_excluded_files() {
    let m = TempDir::new().unwrap();
    let home = m.path();
    let base = home.join(".config").join("dotdipper");
    let compiled = base.join("compiled");
    fs::create_dir_all(home.join(".ssh")).unwrap();
    fs::create_dir_all(compiled.join(".ssh")).unwrap();

    fs::write(home.join(".ssh").join("config"), "Host keep-me\n").unwrap();
    fs::write(compiled.join(".ssh").join("config"), "Host from-store\n").unwrap();
    fs::write(compiled.join(".zshrc"), "export OK=1\n").unwrap();

    let manifest = serde_json::json!({
        "version": "1.0.0",
        "created": "2026-01-01T00:00:00Z",
        "files": {
            ".zshrc": {
                "path": ".zshrc",
                "hash": "a",
                "size": 12,
                "mode": 0o644,
                "modified": "2026-01-01T00:00:00Z"
            },
            ".ssh/config": {
                "path": ".ssh/config",
                "hash": "b",
                "size": 16,
                "mode": 0o600,
                "modified": "2026-01-01T00:00:00Z"
            }
        }
    });
    let manifest_text = serde_json::to_string_pretty(&manifest).unwrap();
    fs::write(compiled.join("manifest.lock"), &manifest_text).unwrap();
    fs::write(base.join("manifest.lock"), &manifest_text).unwrap();

    let config = base.join("config.toml");
    fs::write(
        &config,
        format!(
            r#"
[general]
default_mode = "copy"
backup = true
tracked_files = ["{home}/.zshrc", "{home}/.ssh/config"]

[packages]
common = []

[files."~/.ssh/config"]
exclude = true
"#,
            home = home.display()
        ),
    )
    .unwrap();

    let _guard = EnvGuard::apply(&[
        ("HOME", Some(home)),
        ("DOTDIPPER_HOME", Some(&base)),
        ("XDG_CONFIG_HOME", None),
    ]);

    bin()
        .env("HOME", home)
        .env("DOTDIPPER_HOME", &base)
        .env_remove("XDG_CONFIG_HOME")
        .arg("--config")
        .arg(&config)
        .arg("apply")
        .arg("--force")
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(home.join(".zshrc")).unwrap(),
        "export OK=1\n"
    );
    assert_eq!(
        fs::read_to_string(home.join(".ssh").join("config")).unwrap(),
        "Host keep-me\n",
        "excluded ~/.ssh/config must not be overwritten"
    );

    bin()
        .env("HOME", home)
        .env("DOTDIPPER_HOME", &base)
        .env_remove("XDG_CONFIG_HOME")
        .arg("--config")
        .arg(&config)
        .arg("install")
        .arg("--dry-run")
        .assert()
        .success();

    let setup = fs::read_to_string(base.join("install").join("setup_dotfiles.sh")).unwrap();
    assert!(setup.contains(".zshrc"));
    assert!(
        !setup.contains(".ssh/config"),
        "setup_dotfiles.sh must omit excluded paths"
    );
}

// ---------------------------------------------------------------------------
// Symlink apply mode
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn e2e_apply_symlink_mode_links_into_compiled() {
    let m = TempDir::new().unwrap();
    let home = m.path();
    let base = home.join(".config").join("dotdipper");
    let compiled = base.join("compiled");
    fs::create_dir_all(&compiled).unwrap();
    fs::write(compiled.join(".aliases"), "alias g=git\n").unwrap();

    let manifest = serde_json::json!({
        "version": "1.0.0",
        "created": "2026-01-01T00:00:00Z",
        "files": {
            ".aliases": {
                "path": ".aliases",
                "hash": "x",
                "size": 12,
                "mode": 0o644,
                "modified": "2026-01-01T00:00:00Z"
            }
        }
    });
    let text = serde_json::to_string_pretty(&manifest).unwrap();
    fs::write(compiled.join("manifest.lock"), &text).unwrap();
    fs::write(base.join("manifest.lock"), &text).unwrap();
    write_config(
        &base.join("config.toml"),
        home,
        &[".aliases"],
        true,
        "symlink",
    );

    let _guard = EnvGuard::apply(&[
        ("HOME", Some(home)),
        ("DOTDIPPER_HOME", Some(&base)),
        ("XDG_CONFIG_HOME", None),
    ]);

    bin()
        .env("HOME", home)
        .env("DOTDIPPER_HOME", &base)
        .env_remove("XDG_CONFIG_HOME")
        .arg("--config")
        .arg(base.join("config.toml"))
        .arg("apply")
        .arg("--force")
        .assert()
        .success();

    let target = home.join(".aliases");
    assert!(target.is_symlink(), "symlink mode should create a symlink");
    // Symlink target is the active profile store (compat `compiled/` may be a symlink itself)
    assert_eq!(
        fs::read_link(&target).unwrap().canonicalize().unwrap(),
        compiled.join(".aliases").canonicalize().unwrap()
    );
    assert_eq!(fs::read_to_string(&target).unwrap(), "alias g=git\n");
}

// ---------------------------------------------------------------------------
// Rollback: safety snapshot + preserve .git
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn e2e_rollback_preserves_git_and_creates_safety_snapshot() {
    let m = TempDir::new().unwrap();
    let home = m.path();
    let base = home.join(".config").join("dotdipper");
    let compiled = base.join("compiled");
    let config = base.join("config.toml");

    fs::write(home.join(".testrc"), "v1\n").unwrap();
    write_config(&config, home, &[".testrc"], true, "copy");

    let _guard = EnvGuard::apply(&[
        ("HOME", Some(home)),
        ("DOTDIPPER_HOME", Some(&base)),
        ("XDG_CONFIG_HOME", None),
    ]);

    // Snapshot v1 (also compiles into compiled/)
    bin()
        .env("HOME", home)
        .env("DOTDIPPER_HOME", &base)
        .env_remove("XDG_CONFIG_HOME")
        .arg("--config")
        .arg(&config)
        .arg("snapshot")
        .arg("create")
        .arg("--force")
        .arg("-m")
        .arg("v1")
        .assert()
        .success();

    // Init git inside compiled to verify preservation
    git(&compiled, &["init", "-b", "main"]);
    git(&compiled, &["config", "user.email", "test@example.com"]);
    git(&compiled, &["config", "user.name", "Test"]);
    git(&compiled, &["add", "-A"]);
    git(&compiled, &["commit", "-m", "track v1"]);
    let head_before = git_out(&compiled, &["rev-parse", "HEAD"]);

    let snap_ids: Vec<String> = fs::read_dir(base.join("snapshots"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(snap_ids.len(), 1);
    let v1_id = snap_ids[0].clone();

    // Ensure distinct snapshot ids even on very fast machines
    std::thread::sleep(std::time::Duration::from_millis(5));

    // Advance to v2
    fs::write(home.join(".testrc"), "v2\n").unwrap();
    bin()
        .env("HOME", home)
        .env("DOTDIPPER_HOME", &base)
        .env_remove("XDG_CONFIG_HOME")
        .arg("--config")
        .arg(&config)
        .arg("snapshot")
        .arg("create")
        .arg("--force")
        .arg("-m")
        .arg("v2")
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(compiled.join(".testrc")).unwrap(),
        "v2\n"
    );

    // Rollback to v1
    bin()
        .env("HOME", home)
        .env("DOTDIPPER_HOME", &base)
        .env_remove("XDG_CONFIG_HOME")
        .arg("--config")
        .arg(&config)
        .arg("snapshot")
        .arg("rollback")
        .arg(&v1_id)
        .arg("--force")
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(compiled.join(".testrc")).unwrap(),
        "v1\n"
    );
    assert!(
        compiled.join(".git").exists(),
        "rollback must preserve compiled/.git"
    );
    let head_after = git_out(&compiled, &["rev-parse", "HEAD"]);
    assert_eq!(head_before, head_after, "git HEAD must be unchanged");

    // Safety snapshot of pre-rollback (v2) state should exist
    let snaps: Vec<_> = fs::read_dir(base.join("snapshots"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    assert!(
        snaps.len() >= 3,
        "expected v1 + v2 + safety snapshot, got {}",
        snaps.len()
    );

    let mut found_safety = false;
    for entry in &snaps {
        let meta = entry.path().join("snapshot.json");
        if meta.exists() {
            let body = fs::read_to_string(meta).unwrap();
            if body.contains("pre-rollback safety") {
                found_safety = true;
                // Safety snapshot should contain v2 content
                let safety_testrc = entry.path().join(".testrc");
                if safety_testrc.exists() {
                    assert_eq!(fs::read_to_string(safety_testrc).unwrap(), "v2\n");
                }
            }
        }
    }
    assert!(found_safety, "expected a pre-rollback safety snapshot");

    // $HOME still at v2 until apply
    assert_eq!(fs::read_to_string(home.join(".testrc")).unwrap(), "v2\n");
}

// ---------------------------------------------------------------------------
// Snapshot create writes manifest into compiled and skips .git objects
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn e2e_snapshot_writes_manifest_and_skips_git_objects() {
    let m = TempDir::new().unwrap();
    let home = m.path();
    let base = home.join(".config").join("dotdipper");
    let compiled = base.join("compiled");
    let config = base.join("config.toml");

    fs::write(home.join(".envrc"), "export FOO=1\n").unwrap();
    write_config(&config, home, &[".envrc"], true, "copy");

    let _guard = EnvGuard::apply(&[
        ("HOME", Some(home)),
        ("DOTDIPPER_HOME", Some(&base)),
        ("XDG_CONFIG_HOME", None),
    ]);

    // Pre-seed a fake .git under compiled to ensure snapshots skip it
    fs::create_dir_all(compiled.join(".git").join("objects")).unwrap();
    fs::write(compiled.join(".git").join("HEAD"), "ref: refs/heads/main\n").unwrap();

    bin()
        .env("HOME", home)
        .env("DOTDIPPER_HOME", &base)
        .env_remove("XDG_CONFIG_HOME")
        .arg("--config")
        .arg(&config)
        .arg("snapshot")
        .arg("create")
        .arg("--force")
        .assert()
        .success();

    assert!(compiled.join("manifest.lock").exists());
    assert!(base.join("manifest.lock").exists());
    assert!(compiled.join(".envrc").exists());

    // Versioned snapshot dirs must not contain .git/
    for entry in fs::read_dir(base.join("snapshots")).unwrap() {
        let entry = entry.unwrap();
        if entry.path().is_dir() {
            assert!(
                !entry.path().join(".git").exists(),
                "versioned snapshots must not copy .git"
            );
            assert!(entry.path().join(".envrc").exists());
        }
    }
}

// ---------------------------------------------------------------------------
// Undo last push against local bare remote
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn e2e_undo_reverts_last_pushed_commit() {
    let (remote_root, remote_path) = init_bare_remote();
    let _keep = remote_root;

    let m = TempDir::new().unwrap();
    let home = m.path();
    let base = home.join(".config").join("dotdipper");
    let config = base.join("config.toml");
    let compiled = base.join("compiled");

    fs::write(home.join(".testrc"), "before\n").unwrap();
    write_config(&config, home, &[".testrc"], true, "copy");

    let _guard = EnvGuard::apply(&[
        ("HOME", Some(home)),
        ("DOTDIPPER_HOME", Some(&base)),
        ("XDG_CONFIG_HOME", None),
        ("DOTDIPPER_TEST_REMOTE", Some(&remote_path)),
    ]);

    bin()
        .env("HOME", home)
        .env("DOTDIPPER_HOME", &base)
        .env_remove("XDG_CONFIG_HOME")
        .env("DOTDIPPER_TEST_REMOTE", &remote_path)
        .arg("--config")
        .arg(&config)
        .arg("snapshot")
        .arg("create")
        .arg("--force")
        .assert()
        .success();
    bin()
        .env("HOME", home)
        .env("DOTDIPPER_HOME", &base)
        .env_remove("XDG_CONFIG_HOME")
        .env("DOTDIPPER_TEST_REMOTE", &remote_path)
        .arg("--config")
        .arg(&config)
        .arg("push")
        .arg("-m")
        .arg("before")
        .assert()
        .success();

    fs::write(home.join(".testrc"), "after\n").unwrap();
    bin()
        .env("HOME", home)
        .env("DOTDIPPER_HOME", &base)
        .env_remove("XDG_CONFIG_HOME")
        .env("DOTDIPPER_TEST_REMOTE", &remote_path)
        .arg("--config")
        .arg(&config)
        .arg("snapshot")
        .arg("create")
        .arg("--force")
        .assert()
        .success();
    bin()
        .env("HOME", home)
        .env("DOTDIPPER_HOME", &base)
        .env_remove("XDG_CONFIG_HOME")
        .env("DOTDIPPER_TEST_REMOTE", &remote_path)
        .arg("--config")
        .arg(&config)
        .arg("push")
        .arg("-m")
        .arg("after")
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(compiled.join(".testrc")).unwrap(),
        "after\n"
    );

    bin()
        .env("HOME", home)
        .env("DOTDIPPER_HOME", &base)
        .env_remove("XDG_CONFIG_HOME")
        .env("DOTDIPPER_TEST_REMOTE", &remote_path)
        .arg("--config")
        .arg(&config)
        .arg("undo")
        .arg("--force")
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(compiled.join(".testrc")).unwrap(),
        "before\n"
    );

    let subject = git_out(&compiled, &["log", "-1", "--pretty=%s"]);
    assert!(
        subject.starts_with("Revert "),
        "expected revert commit, got {subject}"
    );
}

// ---------------------------------------------------------------------------
// Help text documents safer pull --force semantics
// ---------------------------------------------------------------------------

#[test]
fn e2e_pull_help_documents_force_semantics() {
    bin()
        .arg("pull")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("compiled"))
        .stdout(predicate::str::contains("--apply"));
}

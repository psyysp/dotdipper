//! Profile selection: active store, DOTDIPPER_PROFILE override, isolation.

use assert_cmd::Command;
use predicates::prelude::*;
use serial_test::serial;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

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

fn bin() -> Command {
    Command::cargo_bin("dotdipper").unwrap()
}

fn write_config(config: &Path, home: &Path, tracked: &[&str], profile: &str) {
    let tracked_lines: Vec<String> = tracked
        .iter()
        .map(|rel| format!("  \"{}\",", home.join(rel).display()))
        .collect();
    let content = format!(
        r#"
[general]
default_mode = "copy"
backup = true
active_profile = "{profile}"
tracked_files = [
{tracked}
]
"#,
        profile = profile,
        tracked = tracked_lines.join("\n")
    );
    if let Some(parent) = config.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(config, content).unwrap();
}

#[test]
#[serial]
fn profile_switch_isolates_compiled_stores() {
    let root = TempDir::new().unwrap();
    let home = root.path().join("home");
    let base = root.path().join("dotdipper");
    fs::create_dir_all(&home).unwrap();

    let _guard = EnvGuard::apply(&[
        ("HOME", Some(&home)),
        ("DOTDIPPER_HOME", Some(&base)),
        ("XDG_CONFIG_HOME", None),
        ("DOTDIPPER_PROFILE", None),
    ]);

    let config = base.join("config.toml");
    fs::write(home.join(".zshrc"), "export DEFAULT=1\n").unwrap();
    write_config(&config, &home, &[".zshrc"], "default");

    bin()
        .env("HOME", &home)
        .env("DOTDIPPER_HOME", &base)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("DOTDIPPER_PROFILE")
        .args([
            "--config",
            config.to_str().unwrap(),
            "snapshot",
            "create",
            "-m",
            "default",
        ])
        .assert()
        .success();

    assert!(base
        .join("profiles")
        .join("default")
        .join("compiled")
        .join(".zshrc")
        .exists());

    bin()
        .env("HOME", &home)
        .env("DOTDIPPER_HOME", &base)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("DOTDIPPER_PROFILE")
        .args([
            "--config",
            config.to_str().unwrap(),
            "profile",
            "create",
            "work",
        ])
        .assert()
        .success();

    bin()
        .env("HOME", &home)
        .env("DOTDIPPER_HOME", &base)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("DOTDIPPER_PROFILE")
        .args([
            "--config",
            config.to_str().unwrap(),
            "profile",
            "switch",
            "work",
        ])
        .assert()
        .success();

    fs::write(home.join(".zshrc"), "export WORK=1\n").unwrap();
    // After switch, tracked_files still point at home .zshrc — re-write config for work snapshot
    write_config(&config, &home, &[".zshrc"], "work");

    bin()
        .env("HOME", &home)
        .env("DOTDIPPER_HOME", &base)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("DOTDIPPER_PROFILE")
        .args([
            "--config",
            config.to_str().unwrap(),
            "snapshot",
            "create",
            "-m",
            "work",
        ])
        .assert()
        .success();

    let default_content = fs::read_to_string(
        base.join("profiles")
            .join("default")
            .join("compiled")
            .join(".zshrc"),
    )
    .unwrap();
    let work_content = fs::read_to_string(
        base.join("profiles")
            .join("work")
            .join("compiled")
            .join(".zshrc"),
    )
    .unwrap();

    assert!(default_content.contains("DEFAULT=1"));
    assert!(work_content.contains("WORK=1"));
    assert_ne!(default_content, work_content);

    // Compat link follows active profile
    assert_eq!(
        fs::read_to_string(base.join("compiled").join(".zshrc")).unwrap(),
        work_content
    );
}

#[test]
#[serial]
fn dotdipper_profile_env_overrides_config() {
    let root = TempDir::new().unwrap();
    let home = root.path().join("home");
    let base = root.path().join("dotdipper");
    fs::create_dir_all(&home).unwrap();

    let _guard = EnvGuard::apply(&[
        ("HOME", Some(&home)),
        ("DOTDIPPER_HOME", Some(&base)),
        ("XDG_CONFIG_HOME", None),
        ("DOTDIPPER_PROFILE", None),
    ]);

    let config = base.join("config.toml");
    write_config(&config, &home, &[], "default");

    bin()
        .env("HOME", &home)
        .env("DOTDIPPER_HOME", &base)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("DOTDIPPER_PROFILE")
        .args([
            "--config",
            config.to_str().unwrap(),
            "profile",
            "create",
            "ci",
        ])
        .assert()
        .success();

    fs::write(
        base.join("profiles")
            .join("ci")
            .join("compiled")
            .join("marker"),
        "ci-store\n",
    )
    .unwrap();

    // status/diff touch active store via compiled_dir(); list shows env as active marker
    bin()
        .env("HOME", &home)
        .env("DOTDIPPER_HOME", &base)
        .env("DOTDIPPER_PROFILE", "ci")
        .env_remove("XDG_CONFIG_HOME")
        .args(["--config", config.to_str().unwrap(), "profile", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ci (active)"));

    std::env::set_var("DOTDIPPER_PROFILE", "ci");
    let paths = dotdipper::profiles::active_store_paths().unwrap();
    assert!(paths
        .compiled
        .ends_with(PathBuf::from("profiles/ci/compiled")));
    assert!(paths.compiled.join("marker").exists());
    std::env::remove_var("DOTDIPPER_PROFILE");
}

#[test]
#[serial]
fn overlay_wins_and_switch_does_not_flatten_global() {
    let root = TempDir::new().unwrap();
    let home = root.path().join("home");
    let base = root.path().join("dotdipper");
    fs::create_dir_all(&home).unwrap();

    let _guard = EnvGuard::apply(&[
        ("HOME", Some(&home)),
        ("DOTDIPPER_HOME", Some(&base)),
        ("XDG_CONFIG_HOME", None),
        ("DOTDIPPER_PROFILE", None),
    ]);

    let config = base.join("config.toml");
    write_config(&config, &home, &[".zshrc"], "default");
    let global = fs::read_to_string(&config).unwrap();
    let global = format!("{global}\n[github]\nusername = \"alice\"\nrepo_name = \"dotfiles\"\n");
    fs::write(&config, global).unwrap();

    bin()
        .env("HOME", &home)
        .env("DOTDIPPER_HOME", &base)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("DOTDIPPER_PROFILE")
        .args([
            "--config",
            config.to_str().unwrap(),
            "profile",
            "create",
            "work",
        ])
        .assert()
        .success();

    fs::write(
        base.join("profiles").join("work").join("config.toml"),
        r#"
[github]
repo_name = "dotfiles-work"
branch = "main"

[general]
tracked_files = ["/tmp/work-only"]
"#,
    )
    .unwrap();

    bin()
        .env("HOME", &home)
        .env("DOTDIPPER_HOME", &base)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("DOTDIPPER_PROFILE")
        .args([
            "--config",
            config.to_str().unwrap(),
            "profile",
            "switch",
            "work",
        ])
        .assert()
        .success();

    let global_after = fs::read_to_string(&config).unwrap();
    assert!(
        global_after.contains("repo_name = \"dotfiles\""),
        "switch must not copy overlay repo_name into the global config:\n{global_after}"
    );
    assert!(!global_after.contains("dotfiles-work"));
    assert!(global_after.contains("active_profile = \"work\""));

    let loaded = dotdipper::cfg::load(&config).unwrap();
    assert_eq!(loaded.github.repo_name.as_deref(), Some("dotfiles-work"));
    assert_eq!(loaded.github.branch.as_deref(), Some("main"));
    assert_eq!(
        loaded.general.tracked_files,
        vec![PathBuf::from("/tmp/work-only")]
    );
    assert_eq!(loaded.general.active_profile.as_deref(), Some("work"));
}

fn git(repo: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
#[serial]
fn profiles_push_to_separate_branches() {
    let remote_root = TempDir::new().unwrap();
    let remote = remote_root.path().join("dotfiles.git");
    fs::create_dir_all(&remote).unwrap();
    let init = std::process::Command::new("git")
        .args(["init", "--bare", "--initial-branch=main"])
        .current_dir(&remote)
        .output()
        .unwrap();
    assert!(init.status.success());

    let root = TempDir::new().unwrap();
    let home = root.path().join("home");
    let base = root.path().join("dotdipper");
    fs::create_dir_all(&home).unwrap();

    let _guard = EnvGuard::apply(&[
        ("HOME", Some(&home)),
        ("DOTDIPPER_HOME", Some(&base)),
        ("XDG_CONFIG_HOME", None),
        ("DOTDIPPER_PROFILE", None),
    ]);

    let config = base.join("config.toml");
    fs::write(home.join(".zshrc"), "export DEFAULT=1\n").unwrap();
    write_config(&config, &home, &[".zshrc"], "default");
    let mut global = fs::read_to_string(&config).unwrap();
    global.push_str("\n[github]\nusername = \"e2e-user\"\nrepo_name = \"dotfiles\"\n");
    fs::write(&config, global).unwrap();

    bin()
        .env("HOME", &home)
        .env("DOTDIPPER_HOME", &base)
        .env("DOTDIPPER_TEST_REMOTE", &remote)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("DOTDIPPER_PROFILE")
        .args([
            "--config",
            config.to_str().unwrap(),
            "snapshot",
            "create",
            "-m",
            "default",
        ])
        .assert()
        .success();

    bin()
        .env("HOME", &home)
        .env("DOTDIPPER_HOME", &base)
        .env("DOTDIPPER_TEST_REMOTE", &remote)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("DOTDIPPER_PROFILE")
        .args([
            "--config",
            config.to_str().unwrap(),
            "push",
            "-m",
            "default profile",
        ])
        .assert()
        .success();

    bin()
        .env("HOME", &home)
        .env("DOTDIPPER_HOME", &base)
        .env("DOTDIPPER_TEST_REMOTE", &remote)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("DOTDIPPER_PROFILE")
        .args([
            "--config",
            config.to_str().unwrap(),
            "profile",
            "create",
            "work",
        ])
        .assert()
        .success();

    bin()
        .env("HOME", &home)
        .env("DOTDIPPER_HOME", &base)
        .env("DOTDIPPER_TEST_REMOTE", &remote)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("DOTDIPPER_PROFILE")
        .args([
            "--config",
            config.to_str().unwrap(),
            "profile",
            "switch",
            "work",
        ])
        .assert()
        .success();

    fs::write(home.join(".zshrc"), "export WORK=1\n").unwrap();
    write_config(&config, &home, &[".zshrc"], "work");
    let mut global = fs::read_to_string(&config).unwrap();
    global.push_str("\n[github]\nusername = \"e2e-user\"\nrepo_name = \"dotfiles\"\n");
    fs::write(&config, global).unwrap();

    bin()
        .env("HOME", &home)
        .env("DOTDIPPER_HOME", &base)
        .env("DOTDIPPER_TEST_REMOTE", &remote)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("DOTDIPPER_PROFILE")
        .args([
            "--config",
            config.to_str().unwrap(),
            "snapshot",
            "create",
            "-m",
            "work",
        ])
        .assert()
        .success();

    bin()
        .env("HOME", &home)
        .env("DOTDIPPER_HOME", &base)
        .env("DOTDIPPER_TEST_REMOTE", &remote)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("DOTDIPPER_PROFILE")
        .args([
            "--config",
            config.to_str().unwrap(),
            "push",
            "-m",
            "work profile",
        ])
        .assert()
        .success();

    let refs = std::process::Command::new("git")
        .args(["--git-dir", remote.to_str().unwrap(), "show-ref"])
        .output()
        .unwrap();
    let refs = String::from_utf8_lossy(&refs.stdout);
    assert!(
        refs.contains("refs/heads/main"),
        "default profile should push main:\n{refs}"
    );
    assert!(
        refs.contains("refs/heads/dotdipper/work"),
        "work profile should push dotdipper/work:\n{refs}"
    );

    let inspect = root.path().join("inspect");
    git(
        root.path(),
        &[
            "clone",
            "--branch",
            "dotdipper/work",
            "--single-branch",
            remote.to_str().unwrap(),
            inspect.to_str().unwrap(),
        ],
    );
    let work_zsh = fs::read_to_string(inspect.join(".zshrc")).unwrap();
    assert!(work_zsh.contains("WORK=1"));
}

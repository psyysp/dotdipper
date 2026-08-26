//! Safety and sync integration tests:
//! - manifest.lock synced from compiled/ enables apply after pull
//! - apply creates .bak backups for existing home files
//! - install --dry-run generates a manifest-aware setup script
use assert_cmd::Command;
use std::fs;
use std::process::Command as StdCommand;
use tempfile::TempDir;

fn git(repo: &std::path::Path, args: &[&str]) {
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

#[test]
fn pull_style_clone_then_apply_uses_compiled_manifest() {
    let remote = TempDir::new().unwrap();
    let remote_path = remote.path().join("dotfiles.git");
    fs::create_dir_all(&remote_path).unwrap();
    let init = StdCommand::new("git")
        .args(["init", "--bare", "--initial-branch=main"])
        .current_dir(&remote_path)
        .output()
        .unwrap();
    assert!(init.status.success());

    // Machine 1: compiled store with dotfile + manifest.lock (as push now writes)
    let machine1 = TempDir::new().unwrap();
    let compiled1 = machine1.path().join("compiled");
    fs::create_dir_all(&compiled1).unwrap();
    fs::write(compiled1.join(".testrc"), "export HELLO=1\n").unwrap();

    let manifest = serde_json::json!({
        "version": "1.0.0",
        "created": "2026-01-01T00:00:00Z",
        "files": {
            ".testrc": {
                "path": ".testrc",
                "hash": "placeholder",
                "size": 15,
                "mode": 0o644,
                "modified": "2026-01-01T00:00:00Z"
            }
        }
    });
    fs::write(
        compiled1.join("manifest.lock"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    git(&compiled1, &["init", "-b", "main"]);
    git(&compiled1, &["config", "user.email", "test@example.com"]);
    git(&compiled1, &["config", "user.name", "Test"]);
    git(&compiled1, &["add", "-A"]);
    git(&compiled1, &["commit", "-m", "initial with manifest"]);
    git(
        &compiled1,
        &["remote", "add", "origin", remote_path.to_str().unwrap()],
    );
    git(&compiled1, &["push", "-u", "origin", "main"]);

    // Machine 2: fresh home, clone remote into compiled (simulates pull)
    let machine2 = TempDir::new().unwrap();
    let home2 = machine2.path();
    let config_dir2 = home2.join(".config").join("dotdipper");
    fs::create_dir_all(&config_dir2).unwrap();
    let compiled2 = config_dir2.join("compiled");
    let clone = StdCommand::new("git")
        .args([
            "clone",
            remote_path.to_str().unwrap(),
            compiled2.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(clone.status.success());
    assert!(compiled2.join(".testrc").exists());
    assert!(compiled2.join("manifest.lock").exists());

    let config_path2 = config_dir2.join("config.toml");
    fs::write(
        &config_path2,
        r#"
[general]
default_mode = "copy"
backup = true
tracked_files = []

[packages]
common = []
"#,
    )
    .unwrap();

    let mut apply = Command::cargo_bin("dotdipper").unwrap();
    apply
        .env("HOME", home2)
        .env("DOTDIPPER_HOME", &config_dir2)
        .env_remove("XDG_CONFIG_HOME")
        .arg("--config")
        .arg(&config_path2)
        .arg("apply")
        .arg("--force");
    apply.assert().success();

    assert_eq!(
        fs::read_to_string(home2.join(".testrc")).unwrap(),
        "export HELLO=1\n"
    );
    // Base manifest synced from compiled by load_manifest()
    assert!(config_dir2.join("manifest.lock").exists());
}

#[test]
fn apply_backs_up_existing_home_files() {
    let temp = TempDir::new().unwrap();
    let home = temp.path();
    let config_dir = home.join(".config").join("dotdipper");
    let compiled = config_dir.join("compiled");
    fs::create_dir_all(&compiled).unwrap();

    fs::write(home.join(".zshrc"), "old config\n").unwrap();
    fs::write(compiled.join(".zshrc"), "new config\n").unwrap();

    let manifest = serde_json::json!({
        "version": "1.0.0",
        "created": "2026-01-01T00:00:00Z",
        "files": {
            ".zshrc": {
                "path": ".zshrc",
                "hash": "abc",
                "size": 11,
                "mode": 0o644,
                "modified": "2026-01-01T00:00:00Z"
            }
        }
    });
    fs::write(
        compiled.join("manifest.lock"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    fs::write(
        config_dir.join("manifest.lock"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let config_path = config_dir.join("config.toml");
    fs::write(
        &config_path,
        format!(
            r#"
[general]
default_mode = "copy"
backup = true
tracked_files = ["{}"]

[packages]
common = []
"#,
            home.join(".zshrc").display()
        ),
    )
    .unwrap();

    let mut apply = Command::cargo_bin("dotdipper").unwrap();
    apply
        .env("HOME", home)
        .env("DOTDIPPER_HOME", &config_dir)
        .env_remove("XDG_CONFIG_HOME")
        .arg("--config")
        .arg(&config_path)
        .arg("apply")
        .arg("--force");
    apply.assert().success();

    assert_eq!(
        fs::read_to_string(home.join(".zshrc")).unwrap(),
        "new config\n"
    );

    let backups: Vec<_> = fs::read_dir(home)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with(".zshrc.bak."))
        .collect();
    assert!(
        !backups.is_empty(),
        "expected a .bak backup of the previous .zshrc"
    );
    assert_eq!(
        fs::read_to_string(home.join(&backups[0])).unwrap(),
        "old config\n"
    );
}

#[test]
fn install_dry_run_generates_manifest_aware_setup_script() {
    let temp = TempDir::new().unwrap();
    let home = temp.path();
    let config_dir = home.join(".config").join("dotdipper");
    let compiled = config_dir.join("compiled");
    fs::create_dir_all(&compiled).unwrap();
    fs::write(compiled.join(".vimrc"), "set number\n").unwrap();

    let manifest = serde_json::json!({
        "version": "1.0.0",
        "created": "2026-01-01T00:00:00Z",
        "files": {
            ".vimrc": {
                "path": ".vimrc",
                "hash": "abc",
                "size": 11,
                "mode": 0o644,
                "modified": "2026-01-01T00:00:00Z"
            }
        }
    });
    fs::write(
        compiled.join("manifest.lock"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    fs::write(
        config_dir.join("manifest.lock"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let config_path = config_dir.join("config.toml");
    fs::write(
        &config_path,
        r#"
[general]
default_mode = "symlink"
backup = true
tracked_files = []

[packages]
common = ["git"]

[files."~/.ssh/config"]
exclude = true
"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", home)
        .env("DOTDIPPER_HOME", &config_dir)
        .env_remove("XDG_CONFIG_HOME")
        .arg("--config")
        .arg(&config_path)
        .arg("install")
        .arg("--dry-run");
    cmd.assert().success();

    let setup = fs::read_to_string(config_dir.join("install").join("setup_dotfiles.sh")).unwrap();
    assert!(
        setup.contains("DOTFILE_COUNT=1"),
        "setup script should embed an explicit file list"
    );
    assert!(setup.contains("apply_symlink '.vimrc'"));
    assert!(
        !setup.contains(r#"find "$COMPILED_DIR" -type f"#),
        "manifest-backed setup should not fall back to runtime find"
    );
}

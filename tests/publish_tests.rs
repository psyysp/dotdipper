//! End-to-end checks for `dotdipper publish`.
//!
//! The unit tests in `src/publish` cover the redaction and scanning rules. What
//! matters here is the command's contract: a dirty tree must fail the command
//! and write nothing, and a clean one must produce a sanitized tree.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Builds an isolated dotdipper home with a compiled store, so the test never
/// touches the invoking user's real `~/.config/dotdipper`.
fn isolated_store(home: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let base = home.join(".config").join("dotdipper");
    let compiled = base.join("profiles").join("default").join("compiled");
    fs::create_dir_all(&compiled).unwrap();

    let config_path = base.join("config.toml");
    fs::write(
        &config_path,
        r#"
[general]
tracked_files = []

[github]
username = "someuser"
repo_name = "dotfiles-private"
public_repo_name = "dotfiles-public"
"#,
    )
    .unwrap();

    (config_path, compiled)
}

/// Generates the allowlist, the way a user runs `--review` before their first
/// publish. Returns the allowlist path so tests can inspect or edit it.
fn review(home: &Path, config_path: &Path) -> std::path::PathBuf {
    publish(home, config_path)
        .arg("--review")
        .assert()
        .success();
    home.join(".config")
        .join("dotdipper")
        .join("public-allowlist.toml")
}

fn publish(home: &Path, config_path: &Path) -> Command {
    let mut cmd = Command::cargo_bin("dotdipper").unwrap();
    cmd.env("HOME", home)
        .env_remove("XDG_CONFIG_HOME")
        .env("DOTDIPPER_HOME", home.join(".config").join("dotdipper"))
        .env_remove("DOTDIPPER_PROFILE")
        .arg("--config")
        .arg(config_path)
        .arg("publish");
    cmd
}

#[test]
fn publish_redacts_identity_and_withholds_excluded_files() {
    let temp = TempDir::new().unwrap();
    let home = temp.path();
    let (config_path, compiled) = isolated_store(home);
    let out = home.join("public-out");

    fs::create_dir_all(compiled.join(".ssh")).unwrap();
    fs::write(
        compiled.join(".ssh/config"),
        "Host box\n  HostName box.tail1234ab.ts.net\n",
    )
    .unwrap();
    fs::write(
        compiled.join(".gitconfig"),
        "[user]\n\tname = Real Person\n\temail = real@person.tld\n",
    )
    .unwrap();
    fs::write(compiled.join(".vimrc"), "set number\n").unwrap();
    fs::write(compiled.join("manifest.lock"), "{}\n").unwrap();

    review(home, &config_path);

    publish(home, &config_path)
        .arg("--out")
        .arg(&out)
        .assert()
        .success()
        .stdout(predicate::str::contains("Scan clean"));

    let gitconfig = fs::read_to_string(out.join(".gitconfig")).unwrap();
    assert!(!gitconfig.contains("Real Person"));
    assert!(!gitconfig.contains("real@person.tld"));

    assert!(out.join(".vimrc").exists());
    assert!(!out.join(".ssh").exists(), "ssh config must be withheld");
    assert!(!out.join("manifest.lock").exists());
}

#[test]
fn publish_fails_closed_and_writes_nothing_when_a_secret_survives() {
    let temp = TempDir::new().unwrap();
    let home = temp.path();
    let (config_path, compiled) = isolated_store(home);
    let out = home.join("public-out");
    let list_path = home
        .join(".config")
        .join("dotdipper")
        .join("public-allowlist.toml");

    fs::write(compiled.join(".vimrc"), "set number\n").unwrap();
    // No redaction rule covers this, which is exactly the case that must abort.
    fs::write(
        compiled.join("app.conf"),
        "token = ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789\n",
    )
    .unwrap();

    // The scan runs before the allowlist is written, so a dirty tree cannot
    // even be approved: --review reports the finding and writes nothing.
    publish(home, &config_path)
        .arg("--review")
        .assert()
        .failure()
        .stdout(predicate::str::contains("github-token"))
        .stderr(predicate::str::contains("Publish aborted"));

    assert!(
        !list_path.exists(),
        "a dirty tree must not produce an allowlist"
    );

    // And with no allowlist, publish refuses outright rather than shipping.
    publish(home, &config_path)
        .arg("--out")
        .arg(&out)
        .assert()
        .failure();

    assert!(
        !out.exists(),
        "a blocked publish must not leave a partial tree behind"
    );
}

#[test]
fn dry_run_reports_the_plan_without_writing() {
    let temp = TempDir::new().unwrap();
    let home = temp.path();
    let (config_path, compiled) = isolated_store(home);
    let out = home.join("public-out");

    fs::write(
        compiled.join(".gitconfig"),
        "[user]\n\temail = real@person.tld\n",
    )
    .unwrap();

    review(home, &config_path);

    publish(home, &config_path)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("Dry run"))
        .stdout(predicate::str::contains("Scan clean"));

    assert!(!out.exists());
}

#[test]
fn publish_refuses_when_public_repo_matches_the_private_one() {
    let temp = TempDir::new().unwrap();
    let home = temp.path();
    let base = home.join(".config").join("dotdipper");
    let compiled = base.join("profiles").join("default").join("compiled");
    fs::create_dir_all(&compiled).unwrap();
    fs::write(compiled.join(".vimrc"), "set number\n").unwrap();

    let config_path = base.join("config.toml");
    fs::write(
        &config_path,
        r#"
[general]
tracked_files = []

[github]
username = "someuser"
repo_name = "dotfiles"
public_repo_name = "dotfiles"
"#,
    )
    .unwrap();

    // Publishing a sanitized copy into the private repo would expose its whole
    // history, since visibility is a property of the repository.
    publish(home, &config_path)
        .assert()
        .failure()
        .stderr(predicate::str::contains("must differ from"));
}

#[test]
fn publish_reports_a_missing_public_repo_clearly() {
    let temp = TempDir::new().unwrap();
    let home = temp.path();
    let base = home.join(".config").join("dotdipper");
    let compiled = base.join("profiles").join("default").join("compiled");
    fs::create_dir_all(&compiled).unwrap();
    fs::write(compiled.join(".vimrc"), "set number\n").unwrap();

    let config_path = base.join("config.toml");
    fs::write(
        &config_path,
        "[general]\ntracked_files = []\n\n[github]\nusername = \"someuser\"\n",
    )
    .unwrap();

    publish(home, &config_path)
        .assert()
        .failure()
        .stderr(predicate::str::contains("No public repository configured"));
}

#[test]
fn publish_refuses_before_the_allowlist_has_been_reviewed() {
    let temp = TempDir::new().unwrap();
    let home = temp.path();
    let (config_path, compiled) = isolated_store(home);
    fs::write(compiled.join(".vimrc"), "set number\n").unwrap();

    // Nothing may publish until the user has seen what would publish.
    publish(home, &config_path)
        .assert()
        .failure()
        .stderr(predicate::str::contains("No allowlist"));
}

#[test]
fn a_file_added_after_review_is_withheld_until_reviewed_again() {
    let temp = TempDir::new().unwrap();
    let home = temp.path();
    let (config_path, compiled) = isolated_store(home);
    let out = home.join("public-out");

    fs::write(compiled.join(".vimrc"), "set number\n").unwrap();
    let list_path = review(home, &config_path);
    assert!(fs::read_to_string(&list_path).unwrap().contains(".vimrc"));

    // A new dotfile lands in the store after the review. It scans clean, so
    // only the allowlist stands between it and the public repo.
    fs::write(compiled.join("newly-captured.conf"), "setting = 1\n").unwrap();

    publish(home, &config_path)
        .arg("--out")
        .arg(&out)
        .assert()
        .success()
        .stdout(predicate::str::contains("awaiting review"));

    assert!(out.join(".vimrc").exists());
    assert!(
        !out.join("newly-captured.conf").exists(),
        "a file added after review must not publish itself"
    );

    // After a second review it is listed, and then it publishes.
    review(home, &config_path);
    publish(home, &config_path)
        .arg("--out")
        .arg(&out)
        .assert()
        .success();
    assert!(out.join("newly-captured.conf").exists());
}

#[test]
fn the_allowlist_shows_what_was_redacted_in_each_file() {
    let temp = TempDir::new().unwrap();
    let home = temp.path();
    let (config_path, compiled) = isolated_store(home);

    fs::write(
        compiled.join(".gitconfig"),
        "[user]\n\tname = Real Person\n\temail = real@person.tld\n",
    )
    .unwrap();
    fs::write(compiled.join(".vimrc"), "set number\n").unwrap();

    let list_path = review(home, &config_path);
    let text = fs::read_to_string(&list_path).unwrap();

    // The point of the allowlist is that the user can read it and see both
    // what ships and what was scrubbed on the way out.
    assert!(text.contains(".gitconfig"));
    assert!(text.contains("gitconfig-identity"));
    assert!(text.contains(".vimrc"));
    assert!(text.contains("withheld") || text.contains("manifest.lock"));
}

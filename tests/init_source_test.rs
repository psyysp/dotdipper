//! `dotdipper init <source>`: from an installed binary to a set-up home
//! directory in one command, from a repository the user may not control.
//!
//! A local bare repository stands in for the remote through
//! `DOTDIPPER_TEST_REMOTE`, so the real clone, layout and apply paths run
//! without reaching the network. Each test gets its own `$HOME`.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use tempfile::TempDir;

struct Fixture {
    _root: TempDir,
    work: PathBuf,
    remote: PathBuf,
    home: PathBuf,
}

impl Fixture {
    /// A repository on `master` (not `main`) holding `files`.
    fn new(files: &[(&str, &str)]) -> Self {
        let root = TempDir::new().unwrap();
        let work = root.path().join("work");
        let remote = root.path().join("remote.git");
        let home = root.path().join("home");
        fs::create_dir_all(&work).unwrap();
        fs::create_dir_all(&home).unwrap();
        git(&work, &["init", "-q", "-b", "master"]);
        let fx = Fixture {
            _root: root,
            work,
            remote,
            home,
        };
        fx.commit(files, "initial");
        git(
            fx.work.parent().unwrap(),
            &[
                "clone",
                "-q",
                "--bare",
                fx.work.to_str().unwrap(),
                fx.remote.to_str().unwrap(),
            ],
        );
        fx
    }

    fn commit(&self, files: &[(&str, &str)], message: &str) -> String {
        for (path, content) in files {
            let p = self.work.join(path);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, content).unwrap();
        }
        git(&self.work, &["add", "-A"]);
        git(
            &self.work,
            &[
                "-c",
                "user.email=t@example.com",
                "-c",
                "user.name=t",
                "commit",
                "-qm",
                message,
            ],
        );
        if self.remote.exists() {
            git(
                &self.work,
                &["push", "-q", self.remote.to_str().unwrap(), "master"],
            );
        }
        git_out(&self.work, &["rev-parse", "HEAD"])
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::cargo_bin("dotdipper").unwrap();
        cmd.env("HOME", &self.home)
            .env("DOTDIPPER_HOME", self.home.join(".config/dotdipper"))
            .env("DOTDIPPER_TEST_REMOTE", &self.remote)
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("DOTDIPPER_PROFILE");
        cmd
    }

    fn home_file(&self, rel: &str) -> Option<String> {
        fs::read_to_string(self.home.join(rel)).ok()
    }

    fn config(&self) -> String {
        fs::read_to_string(self.home.join(".config/dotdipper/config.toml")).unwrap()
    }
}

fn git(dir: &Path, args: &[&str]) {
    let out = StdCommand::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {}: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn git_out(dir: &Path, args: &[&str]) -> String {
    let out = StdCommand::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

const PLAIN_REPO: &[(&str, &str)] = &[
    (".zshrc", "alias ll='ls -l'\n"),
    (".config/nvim/init.lua", "vim.o.number = true\n"),
    ("README.md", "# my dotfiles\n"),
    ("install.sh", "#!/bin/sh\necho installing\n"),
    (".github/workflows/ci.yml", "on: push\n"),
];

#[test]
fn a_plain_repository_is_applied_in_one_command() {
    let fx = Fixture::new(PLAIN_REPO);
    fs::write(fx.home.join(".zshrc"), "mine\n").unwrap();

    fx.cmd()
        .args(["init", "someone/dotfiles", "--apply", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("laid out like $HOME"));

    assert_eq!(fx.home_file(".zshrc").unwrap(), "alias ll='ls -l'\n");
    assert!(fx.home_file(".config/nvim/init.lua").is_some());

    // Files about the repository stay out of $HOME.
    for rel in ["README.md", "install.sh", ".github/workflows/ci.yml"] {
        assert!(fx.home_file(rel).is_none(), "{rel} was applied");
    }

    // The file it replaced was backed up, not lost.
    let backed_up = fs::read_dir(&fx.home)
        .unwrap()
        .filter_map(|e| e.ok())
        .any(|e| e.file_name().to_string_lossy().starts_with(".zshrc.bak."));
    assert!(backed_up, "no backup of the replaced .zshrc");

    // The repository's default branch was followed, and copies were used so
    // a later pull of someone else's repository cannot change live files.
    let config = fx.config();
    assert!(config.contains("branch = \"master\""), "{config}");
    assert!(
        config.contains("url = \"https://github.com/someone/dotfiles.git\""),
        "{config}"
    );
    assert!(config.contains("default_mode = \"copy\""), "{config}");
}

#[test]
fn without_apply_home_is_untouched_and_the_preview_flags_risky_files() {
    let fx = Fixture::new(&[
        (".zshrc", "x\n"),
        (".ssh/authorized_keys", "ssh-ed25519 AAAA someone\n"),
    ]);

    fx.cmd()
        .args(["init", "someone/dotfiles"])
        .assert()
        .success()
        .stdout(predicate::str::contains("runs at every shell start"))
        .stdout(predicate::str::contains("controls SSH access"));

    assert!(fx.home_file(".zshrc").is_none());
    assert!(fx.home_file(".ssh/authorized_keys").is_none());
}

#[test]
fn applying_without_a_terminal_requires_an_explicit_yes() {
    let fx = Fixture::new(PLAIN_REPO);

    fx.cmd()
        .args(["init", "someone/dotfiles", "--apply"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--yes"));

    assert!(fx.home_file(".zshrc").is_none());
}

#[test]
fn a_stow_repository_is_reported_not_applied_by_position() {
    let fx = Fixture::new(&[
        ("zsh/.zshrc", "x\n"),
        ("git/.gitconfig", "x\n"),
        ("install.sh", "#!/bin/sh\nstow */\n"),
    ]);

    fx.cmd()
        .args(["init", "someone/dotfiles", "--apply", "--yes"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("GNU Stow"))
        .stderr(predicate::str::contains("Nothing was applied"));

    assert!(fx.home_file("zsh/.zshrc").is_none());
    assert!(fx.home_file(".zshrc").is_none());
}

#[test]
fn sources_that_git_would_execute_are_refused_before_any_clone() {
    let fx = Fixture::new(PLAIN_REPO);
    for bad in ["ext::sh", "file:///etc", "http://github.com/a/b"] {
        fx.cmd().args(["init", bad]).assert().failure();
    }
    fx.cmd()
        .args(["init", "--", "--upload-pack=touch /tmp/x"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("option"));
    assert!(!fx.home.join(".config/dotdipper/config.toml").exists());
}

#[test]
fn a_pinned_commit_is_what_gets_applied() {
    let fx = Fixture::new(&[(".zshrc", "reviewed\n")]);
    let reviewed = git_out(&fx.work, &["rev-parse", "HEAD"]);
    fx.commit(&[(".zshrc", "changed after review\n")], "later");

    fx.cmd()
        .args([
            "init",
            "someone/dotfiles",
            "--ref",
            &reviewed,
            "--apply",
            "--yes",
        ])
        .assert()
        .success();

    assert_eq!(fx.home_file(".zshrc").unwrap(), "reviewed\n");
}

#[test]
fn a_later_pull_picks_up_files_the_repository_added() {
    let fx = Fixture::new(PLAIN_REPO);
    fx.cmd()
        .args(["init", "someone/dotfiles", "--apply", "--yes"])
        .assert()
        .success();

    fx.commit(&[(".vimrc", "set nu\n")], "add vimrc");

    fx.cmd().args(["pull", "--apply"]).assert().success();
    assert_eq!(fx.home_file(".vimrc").unwrap(), "set nu\n");
    assert!(fx.home_file("README.md").is_none());
}

/// A repository that ships dotdipper's own config, with a hook that would
/// leave a marker in $HOME if it ever ran.
const REPO_WITH_SETTINGS: &[(&str, &str)] = &[
    (".zshrc", "export FROM_REPO=1\n"),
    (
        ".config/dotdipper/config.toml",
        "[general]\ndefault_mode = \"symlink\"\n\n[hooks]\npost_apply = ['touch \"$HOME/pwned\"']\n",
    ),
];

#[test]
fn a_repository_s_dotdipper_settings_and_hooks_are_held_back() {
    let fx = Fixture::new(REPO_WITH_SETTINGS);

    fx.cmd()
        .args(["init", "someone/dotfiles", "--apply", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("touch \"$HOME/pwned\""))
        .stdout(predicate::str::contains(
            "Not applying this repository's dotdipper settings",
        ));

    // The ordinary dotfile arrived; the settings did not replace ours.
    assert_eq!(fx.home_file(".zshrc").unwrap(), "export FROM_REPO=1\n");
    let config = fx.config();
    assert!(!config.contains("pwned"), "{config}");
    assert!(config.contains("default_mode = \"copy\""), "{config}");

    // The exclusion persists, so a later apply cannot bring the hook in
    // and then run it.
    for _ in 0..2 {
        fx.cmd().args(["apply", "--force"]).assert().success();
    }
    assert!(!fx.config().contains("pwned"));
    assert!(!fx.home.join("pwned").exists(), "the repository's hook ran");
}

#[test]
fn adopt_config_applies_the_settings_of_a_repository_you_own() {
    let fx = Fixture::new(REPO_WITH_SETTINGS);

    fx.cmd()
        .args([
            "init",
            "someone/dotfiles",
            "--apply",
            "--yes",
            "--adopt-config",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Adopting this repository"));

    assert!(fx.config().contains("pwned"), "settings were not adopted");
    // Adopting installs the hook; it runs on the next apply, not during init.
    assert!(!fx.home.join("pwned").exists());
}

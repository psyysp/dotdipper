//! What kind of repository was cloned, and which of its files belong in `$HOME`.
//!
//! `dotdipper init <source>` accepts any dotfiles repository, not only ones
//! dotdipper wrote. Most follow one of a few conventions, and only some of
//! them map onto `$HOME` by position:
//!
//! - a dotdipper store or public mirror, where every file is a home path;
//! - a plain repository laid out like `$HOME` (`.zshrc`, `.config/...`),
//!   where the dotfiles map by position but `README.md`, `LICENSE` and an
//!   `install.sh` at the top level are about the repository, not for `$HOME`;
//! - a GNU Stow tree (`zsh/.zshrc`), a chezmoi source (`dot_zshrc`), or a
//!   repository driven by its own install script, none of which map by
//!   position. Applying those by position would litter `$HOME` with
//!   `zsh/.zshrc` and `dot_zshrc`, so they are reported, not applied.
//!
//! This module also names the paths in a repository that deserve a second
//! look before they are written: files that run code at shell start, at
//! login or during git operations, or that grant access to the machine.

use std::fs;
use std::path::Path;

/// The layout of a cloned repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Written by dotdipper (a private store or a public mirror).
    Dotdipper,
    /// A plain repository whose top-level dotfiles mirror `$HOME`.
    HomeMirror,
    /// A GNU Stow tree: top-level package directories holding dotfiles.
    Stow,
    /// A chezmoi source directory.
    Chezmoi,
    /// Nothing recognisable maps onto `$HOME` (typically a repository whose
    /// own script symlinks undotted files into place).
    Unknown,
}

impl Layout {
    /// Whether files can be applied by position.
    pub fn is_applicable(self) -> bool {
        matches!(self, Layout::Dotdipper | Layout::HomeMirror)
    }

    pub fn describe(self) -> &'static str {
        match self {
            Layout::Dotdipper => "dotdipper repository",
            Layout::HomeMirror => "dotfiles repository laid out like $HOME",
            Layout::Stow => "GNU Stow repository",
            Layout::Chezmoi => "chezmoi source repository",
            Layout::Unknown => "repository with no files that map onto $HOME",
        }
    }
}

/// Files only dotdipper writes. Any one of them marks the repository as
/// dotdipper's, where every file, top-level or not, is a home path.
const DOTDIPPER_MARKERS: &[&str] = &[
    "manifest.lock",
    ".dotdipper",
    "Brewfile",
    "apps_manifest.toml",
    "install-apps.sh",
];

/// Dot-named entries that describe the repository rather than a home
/// directory. A `.github/` workflow applied to `$HOME` would do nothing
/// useful, and `.gitignore` there would change what git ignores everywhere.
const REPO_METADATA: &[&str] = &[
    ".git",
    ".github",
    ".gitlab",
    ".gitlab-ci.yml",
    ".gitignore",
    ".gitattributes",
    ".gitmodules",
    ".editorconfig",
    ".pre-commit-config.yaml",
    ".circleci",
    ".travis.yml",
    ".devcontainer",
    ".vscode",
    ".idea",
    ".DS_Store",
];

/// Names an install-script-driven repository usually runs.
const INSTALL_SCRIPTS: &[&str] = &[
    "install.sh",
    "install",
    "bootstrap.sh",
    "bootstrap",
    "setup.sh",
    "Makefile",
    "script",
];

/// Classifies the repository at `root`.
pub fn detect(root: &Path) -> Layout {
    detect_with(root, false)
}

/// Classifies the repository at `root`. `manifest_generated` says the
/// `manifest.lock` there was rebuilt on this machine rather than pulled, so
/// it says nothing about who wrote the repository.
pub fn detect_with(root: &Path, manifest_generated: bool) -> Layout {
    let Ok(entries) = fs::read_dir(root) else {
        return Layout::Unknown;
    };
    let names: Vec<(String, bool)> = entries
        .filter_map(|e| e.ok())
        .map(|e| {
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            (e.file_name().to_string_lossy().into_owned(), is_dir)
        })
        .collect();

    if names.iter().any(|(n, _)| {
        DOTDIPPER_MARKERS.contains(&n.as_str()) && !(manifest_generated && n == "manifest.lock")
    }) {
        return Layout::Dotdipper;
    }
    if names
        .iter()
        .any(|(n, _)| n.starts_with(".chezmoi") || n.starts_with("dot_"))
    {
        return Layout::Chezmoi;
    }
    if names.iter().any(|(n, _)| is_home_dotfile(n)) {
        return Layout::HomeMirror;
    }
    let stow_packages = names
        .iter()
        .filter(|(n, is_dir)| *is_dir && !n.starts_with('.'))
        .filter(|(n, _)| dir_has_home_dotfile(&root.join(n)))
        .count();
    if stow_packages > 0 {
        return Layout::Stow;
    }
    Layout::Unknown
}

/// Whether the repository ships a script of its own. dotdipper never runs
/// it; this is only so the message can say what the author intended.
pub fn install_script(root: &Path) -> Option<&'static str> {
    INSTALL_SCRIPTS
        .iter()
        .copied()
        .find(|name| root.join(name).is_file())
}

/// Whether a repository-relative path belongs in `$HOME` under `layout`.
///
/// For a plain repository only dot-named top-level entries do; `README.md`
/// and friends are about the repository. A dotdipper repository is written
/// from home paths, so everything in it qualifies.
pub fn belongs_in_home(layout: Layout, rel: &Path) -> bool {
    match layout {
        Layout::Dotdipper => true,
        Layout::HomeMirror => rel
            .components()
            .next()
            .map(|c| is_home_dotfile(&c.as_os_str().to_string_lossy()))
            .unwrap_or(false),
        Layout::Stow | Layout::Chezmoi | Layout::Unknown => false,
    }
}

fn is_home_dotfile(name: &str) -> bool {
    name.starts_with('.') && name != "." && name != ".." && !REPO_METADATA.contains(&name)
}

fn dir_has_home_dotfile(dir: &Path) -> bool {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .any(|e| is_home_dotfile(&e.file_name().to_string_lossy()))
        })
        .unwrap_or(false)
}

/// Why a home path deserves a second look before a repository someone else
/// wrote is applied, or `None` for an ordinary configuration file.
///
/// The list targets what an attacker would plant: code that runs on its own
/// (shell startup, login items, git hooks and helpers) and files that grant
/// access (SSH, cloud and registry credentials).
pub fn risk(rel: &Path) -> Option<&'static str> {
    let s = rel.to_string_lossy();
    let first = rel
        .components()
        .next()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .unwrap_or_default();

    const SHELL_STARTUP: &[&str] = &[
        ".bashrc",
        ".bash_profile",
        ".bash_login",
        ".bash_logout",
        ".profile",
        ".zshrc",
        ".zshenv",
        ".zprofile",
        ".zlogin",
        ".zlogout",
        ".inputrc",
    ];
    if s.starts_with(".config/dotdipper/") {
        return Some("dotdipper's own settings, whose hooks run commands");
    }
    if SHELL_STARTUP.contains(&s.as_ref())
        || s.starts_with(".config/fish/")
        || s.starts_with(".oh-my-zsh/")
        || s.starts_with(".config/zsh/")
    {
        return Some("runs at every shell start");
    }
    if first == ".ssh" {
        return Some("controls SSH access and keys");
    }
    if s.starts_with("Library/LaunchAgents/")
        || s.starts_with(".config/autostart/")
        || s.starts_with(".config/systemd/")
    {
        return Some("starts a program at login");
    }
    if s == ".gitconfig" || s.starts_with(".config/git/") || s.contains("/hooks/") {
        return Some("can run commands during git operations");
    }
    if matches!(
        first.as_str(),
        ".aws" | ".kube" | ".gnupg" | ".docker" | ".azure"
    ) || matches!(s.as_ref(), ".netrc" | ".npmrc" | ".pypirc" | ".pgpass")
        || s.starts_with(".config/gcloud/")
        || s.starts_with(".config/gh/")
    {
        return Some("holds credentials or package-registry settings");
    }
    if first == "bin" || s.starts_with(".local/bin/") {
        return Some("installs a program on PATH");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn tree(files: &[&str]) -> TempDir {
        let dir = TempDir::new().unwrap();
        for f in files {
            let p = dir.path().join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, "x").unwrap();
        }
        dir
    }

    #[test]
    fn layouts_are_told_apart() {
        let cases: &[(&[&str], Layout)] = &[
            (&["manifest.lock", ".zshrc"], Layout::Dotdipper),
            (
                &["install-apps.sh", ".zshrc", "README.md"],
                Layout::Dotdipper,
            ),
            (
                &[".zshrc", ".config/nvim/init.lua", "README.md"],
                Layout::HomeMirror,
            ),
            (&["zsh/.zshrc", "git/.gitconfig", "README.md"], Layout::Stow),
            (&["dot_zshrc", ".chezmoiignore"], Layout::Chezmoi),
            (&["zshrc", "vimrc", "install.sh"], Layout::Unknown),
            // Repository metadata alone is not a home directory.
            (
                &[".github/workflows/ci.yml", ".gitignore", "zshrc"],
                Layout::Unknown,
            ),
        ];
        for (files, expected) in cases {
            let dir = tree(files);
            assert_eq!(detect(dir.path()), *expected, "{files:?}");
        }
    }

    #[test]
    fn a_manifest_rebuilt_here_does_not_make_a_plain_repository_dotdipper_s() {
        let dir = tree(&["manifest.lock", ".zshrc", "README.md"]);
        assert_eq!(detect_with(dir.path(), false), Layout::Dotdipper);
        assert_eq!(detect_with(dir.path(), true), Layout::HomeMirror);
    }

    #[test]
    fn a_plain_repository_keeps_its_readme_out_of_home() {
        let l = Layout::HomeMirror;
        assert!(belongs_in_home(l, Path::new(".zshrc")));
        assert!(belongs_in_home(l, Path::new(".config/nvim/init.lua")));
        assert!(!belongs_in_home(l, Path::new("README.md")));
        assert!(!belongs_in_home(l, Path::new("install.sh")));
        assert!(!belongs_in_home(l, Path::new(".github/workflows/ci.yml")));
        assert!(!belongs_in_home(l, Path::new(".gitignore")));
        // A dotdipper mirror places its install script in $HOME on purpose.
        assert!(belongs_in_home(
            Layout::Dotdipper,
            Path::new("install-apps.sh")
        ));
        assert!(!belongs_in_home(Layout::Stow, Path::new("zsh/.zshrc")));
    }

    #[test]
    fn paths_that_run_code_or_grant_access_are_flagged() {
        for p in [
            ".zshrc",
            ".ssh/authorized_keys",
            ".ssh/config",
            "Library/LaunchAgents/com.example.plist",
            ".gitconfig",
            ".config/git/config",
            ".aws/credentials",
            ".local/bin/tool",
            ".config/dotdipper/config.toml",
        ] {
            assert!(risk(Path::new(p)).is_some(), "{p} not flagged");
        }
        for p in [
            ".config/nvim/init.lua",
            ".tmux.conf",
            ".config/kitty/kitty.conf",
        ] {
            assert!(risk(Path::new(p)).is_none(), "{p} flagged");
        }
    }
}

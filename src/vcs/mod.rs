use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

use crate::cfg::Config;
use crate::ui;

const BASE_GITIGNORE: &str = r#"# Temporary files
*.tmp
*.swp
*.swo
*~

# OS files
.DS_Store
Thumbs.db

# Backup files
*.bak
*.backup
"#;

pub fn check_git() -> Result<()> {
    let output = Command::new("git")
        .arg("--version")
        .output()
        .context("Git not found")?;

    if !output.status.success() {
        anyhow::bail!("Git command failed");
    }

    Ok(())
}

pub fn check_gh() -> Result<()> {
    let output = Command::new("gh")
        .arg("--version")
        .output()
        .context("GitHub CLI (gh) not found")?;

    if !output.status.success() {
        anyhow::bail!("GitHub CLI command failed");
    }

    Ok(())
}

pub fn init_repo(repo_path: &Path, branch: &str) -> Result<()> {
    if repo_path.join(".git").exists() {
        return Ok(());
    }

    std::fs::create_dir_all(repo_path).context("Failed to create compiled repository directory")?;

    let output = Command::new("git")
        .args(["init", "-b", branch])
        .current_dir(repo_path)
        .output()
        .context("Failed to initialize git repository")?;

    if !output.status.success() {
        let fallback = Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .context("Failed to initialize git repository")?;
        if !fallback.status.success() {
            anyhow::bail!(
                "Failed to initialize git repository: {}",
                String::from_utf8_lossy(&fallback.stderr)
            );
        }
        let checkout = Command::new("git")
            .args(["checkout", "-b", branch])
            .current_dir(repo_path)
            .output()
            .context("Failed to create initial branch")?;
        if !checkout.status.success() {
            anyhow::bail!(
                "Failed to create branch '{}': {}",
                branch,
                String::from_utf8_lossy(&checkout.stderr)
            );
        }
    }

    std::fs::write(repo_path.join(".gitignore"), BASE_GITIGNORE)?;
    ensure_commit_identity(repo_path)?;

    Ok(())
}

/// Ensure the compiled git repo can commit even when HOME has no global git identity.
fn ensure_commit_identity(repo_path: &Path) -> Result<()> {
    let name_ok = Command::new("git")
        .args(["config", "--get", "user.name"])
        .current_dir(repo_path)
        .output()
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false);
    let email_ok = Command::new("git")
        .args(["config", "--get", "user.email"])
        .current_dir(repo_path)
        .output()
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false);

    if !name_ok {
        let output = Command::new("git")
            .args(["config", "user.name", "dotdipper"])
            .current_dir(repo_path)
            .output()
            .context("Failed to set local git user.name")?;
        if !output.status.success() {
            anyhow::bail!(
                "Failed to set git user.name: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    if !email_ok {
        let output = Command::new("git")
            .args(["config", "user.email", "dotdipper@localhost"])
            .current_dir(repo_path)
            .output()
            .context("Failed to set local git user.email")?;
        if !output.status.success() {
            anyhow::bail!(
                "Failed to set git user.email: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    Ok(())
}

/// Resolved GitHub push/pull target. Branch and repo are independent:
/// overlay/global `[github].repo_name` can point at a dedicated repo while
/// the branch still defaults to `main` / `dotdipper/<profile>`.
#[derive(Debug, Clone)]
pub struct GitTarget {
    pub username: String,
    pub repo_name: String,
    pub branch: String,
}

impl GitTarget {
    pub fn origin_ref(&self) -> String {
        format!("origin/{}", self.branch)
    }
}

pub fn default_branch_for_profile(profile: &str) -> String {
    if profile == "default" {
        "main".to_string()
    } else {
        format!("dotdipper/{profile}")
    }
}

pub fn resolve_git_target(config: &Config, repo_override: Option<&str>) -> Result<GitTarget> {
    let username = resolve_github_username(config)?;
    let repo_name = resolve_repo_name(config, repo_override);
    let profile =
        crate::profiles::resolve_active_profile_name().unwrap_or_else(|_| "default".into());
    let branch = config
        .github
        .branch
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| default_branch_for_profile(&profile));
    Ok(GitTarget {
        username,
        repo_name,
        branch,
    })
}

pub fn push(
    config: &Config,
    message: Option<String>,
    force: bool,
    repo_override: Option<&str>,
) -> Result<String> {
    let repo_path = crate::paths::compiled_dir()?;
    let target = resolve_git_target(config, repo_override)?;

    // Ensure git is initialized on the profile branch
    init_repo(&repo_path, &target.branch)?;
    write_push_gitignore(&repo_path, config)?;
    checkout_or_create_branch(&repo_path, &target.branch)?;

    ui::info(&format!(
        "Pushing to {}/{} ({})",
        target.username, target.repo_name, target.branch
    ));

    // Add all files
    let output = Command::new("git")
        .args(["add", "-A"])
        .current_dir(&repo_path)
        .output()
        .context("Failed to add files to git")?;

    if !output.status.success() {
        anyhow::bail!(
            "Failed to add files: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Check if there are changes to commit
    let status_output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&repo_path)
        .output()
        .context("Failed to check git status")?;

    if status_output.stdout.is_empty() {
        ui::info("No changes to commit");
    } else {
        ensure_commit_identity(&repo_path)?;

        // Commit changes
        let commit_message = message.unwrap_or_else(|| {
            format!(
                "Update dotfiles - {}",
                chrono::Utc::now().format("%Y-%m-%d %H:%M:%S")
            )
        });

        let output = Command::new("git")
            .args(["commit", "-m", commit_message.as_str()])
            .current_dir(&repo_path)
            .output()
            .context("Failed to commit changes")?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to commit: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        ui::success("Changes committed");
    }

    if let Err(e) = ensure_github_repo(config, &repo_path, &target.username, &target.repo_name) {
        ui::warn(&format!("Could not create GitHub repo: {}", e));
        ui::hint("Create a GitHub repository manually and add it as a remote");
        return Ok(target.repo_name);
    }

    // Push to remote
    let mut push_args = vec!["push", "origin", target.branch.as_str()];
    if force {
        push_args.push("--force");
    }

    let output = Command::new("git")
        .args(&push_args)
        .current_dir(&repo_path)
        .output()
        .context("Failed to push to GitHub")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let need_fetch = stderr.contains("fetch first")
            || stderr.contains("Updates were rejected")
            || stderr.contains("integrate the remote changes");

        if need_fetch {
            // Remote has commits we don't have (e.g. repo created with README). Fetch, rebase, retry.
            ui::info("Remote has commits you don't have locally. Syncing and retrying push...");
            let fetch_out = Command::new("git")
                .args(["fetch", "origin", &target.branch])
                .current_dir(&repo_path)
                .output()
                .context("Failed to fetch from origin")?;
            if !fetch_out.status.success() {
                anyhow::bail!(
                    "Failed to fetch: {}. Run 'dotdipper pull' first, then 'dotdipper push' again.",
                    String::from_utf8_lossy(&fetch_out.stderr)
                );
            }
            let rebase_out = Command::new("git")
                .args(["rebase", &target.origin_ref()])
                .current_dir(&repo_path)
                .output()
                .context("Failed to rebase onto origin")?;
            if !rebase_out.status.success() {
                anyhow::bail!(
                    "Rebase failed (remote and local both have changes): {}\n\
                     Resolve conflicts in {:?} (e.g. git rebase --abort or fix and git rebase --continue), then run 'dotdipper push' again.",
                    String::from_utf8_lossy(&rebase_out.stderr),
                    repo_path
                );
            }
            let retry_out = Command::new("git")
                .args(&push_args)
                .current_dir(&repo_path)
                .output()
                .context("Failed to push after rebase")?;
            if !retry_out.status.success() {
                anyhow::bail!(
                    "Failed to push: {}",
                    String::from_utf8_lossy(&retry_out.stderr)
                );
            }
        } else if stderr.contains("failed to push") || stderr.contains("rejected") {
            // No upstream set; try set-upstream and push again
            let output = Command::new("git")
                .args(["push", "--set-upstream", "origin", &target.branch])
                .current_dir(&repo_path)
                .output()
                .context("Failed to set upstream branch")?;

            if !output.status.success() {
                anyhow::bail!(
                    "Failed to push: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        } else {
            anyhow::bail!("Failed to push: {}", stderr);
        }
    }

    Ok(target.repo_name)
}

pub fn pull(config: &Config, force: bool, repo_override: Option<&str>) -> Result<String> {
    let repo_path = crate::paths::compiled_dir()?;
    let target = resolve_git_target(config, repo_override)?;
    let origin_ref = target.origin_ref();

    ui::info(&format!(
        "Pulling from {}/{} ({})",
        target.username, target.repo_name, target.branch
    ));

    // If repo doesn't exist, clone it
    if !repo_path.join(".git").exists() {
        clone_repo(
            &target.username,
            &target.repo_name,
            &target.branch,
            &repo_path,
        )?;
    } else {
        // Ensure current origin points at the selected repo
        add_remote(&target.username, &target.repo_name, &repo_path)?;

        let dirty = has_uncommitted_changes(&repo_path)?;
        if dirty && !force {
            anyhow::bail!(
                "Local compiled store has uncommitted changes. \
                 Run 'dotdipper push' to save them, or re-run with --force to discard local compiled changes \
                 (a git stash is created first). Your live $HOME files are not modified until you pass --apply."
            );
        }

        if dirty && force {
            ui::warn("Local compiled changes detected; stashing before forced pull...");
            stash_local_changes(&repo_path)?;
        }

        // Fetch first so force reset / pull both see latest remote
        fetch_origin_branch(&repo_path, &target.branch)?;
        checkout_tracking_branch(&repo_path, &target.branch)?;

        if force {
            // Overwrite local compiled git state with remote (HOME untouched)
            let reset_output = Command::new("git")
                .args(["reset", "--hard", origin_ref.as_str()])
                .current_dir(&repo_path)
                .output()
                .context("Failed to reset to origin")?;

            if !reset_output.status.success() {
                anyhow::bail!(
                    "Failed to force-sync compiled store: {}",
                    String::from_utf8_lossy(&reset_output.stderr)
                );
            }

            let clean_output = Command::new("git")
                .args(["clean", "-fd"])
                .current_dir(&repo_path)
                .output()
                .context("Failed to clean untracked files after force pull")?;

            if !clean_output.status.success() {
                ui::warn(&format!(
                    "git clean reported issues: {}",
                    String::from_utf8_lossy(&clean_output.stderr)
                ));
            }
        } else {
            // Pull changes
            let output = Command::new("git")
                .args(["pull", "origin", target.branch.as_str()])
                .current_dir(&repo_path)
                .output()
                .context("Failed to pull from GitHub")?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if stderr.contains("no tracking information") {
                    // Set tracking branch
                    let upstream = format!("origin/{}", target.branch);
                    let output = Command::new("git")
                        .args([
                            "branch",
                            &format!("--set-upstream-to={upstream}"),
                            &target.branch,
                        ])
                        .current_dir(&repo_path)
                        .output()
                        .context("Failed to set tracking branch")?;

                    if output.status.success() {
                        // Try pull again
                        let output = Command::new("git")
                            .args(["pull", "origin", target.branch.as_str()])
                            .current_dir(&repo_path)
                            .output()
                            .context("Failed to pull from GitHub")?;

                        if !output.status.success() {
                            anyhow::bail!(
                                "Failed to pull: {}",
                                String::from_utf8_lossy(&output.stderr)
                            );
                        }
                    } else {
                        anyhow::bail!(
                            "Failed to set tracking branch: {}",
                            String::from_utf8_lossy(&output.stderr)
                        );
                    }
                } else {
                    anyhow::bail!("Failed to pull: {}", stderr);
                }
            }
        }
    }

    // Make sure apply/install can find the manifest after pull
    if let Err(e) = crate::repo::sync_manifest_from_compiled() {
        ui::warn(&format!("Could not sync manifest after pull: {}", e));
        ui::hint("You can rebuild it with the files under compiled/ once they exist.");
    }

    Ok(target.repo_name)
}

fn has_uncommitted_changes(repo_path: &Path) -> Result<bool> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo_path)
        .output()
        .context("Failed to check git status")?;

    if !output.status.success() {
        anyhow::bail!(
            "Failed to check git status: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(!output.stdout.is_empty())
}

fn stash_local_changes(repo_path: &Path) -> Result<()> {
    let message = format!(
        "dotdipper pre-pull stash {}",
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S")
    );
    let output = Command::new("git")
        .args(["stash", "push", "-u", "-m", message.as_str()])
        .current_dir(repo_path)
        .output()
        .context("Failed to stash local compiled changes")?;

    if !output.status.success() {
        anyhow::bail!(
            "Failed to stash local changes before force pull: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    ui::info("Stashed local compiled changes (recover with: git -C ~/.config/dotdipper/compiled stash pop)");
    Ok(())
}

pub fn undo_last_push(config: &Config, force: bool, repo_override: Option<&str>) -> Result<String> {
    let repo_path = crate::paths::compiled_dir()?;
    let target = resolve_git_target(config, repo_override)?;
    let origin_ref = target.origin_ref();

    if !repo_path.join(".git").exists() {
        clone_repo(
            &target.username,
            &target.repo_name,
            &target.branch,
            &repo_path,
        )?;
    } else {
        add_remote(&target.username, &target.repo_name, &repo_path)?;
    }

    ensure_clean_worktree(&repo_path)?;
    fetch_origin_branch(&repo_path, &target.branch)?;
    checkout_tracking_branch(&repo_path, &target.branch)?;
    fast_forward_to_origin(&repo_path, &target.branch)?;
    ensure_head_matches_ref(&repo_path, &origin_ref)?;
    ensure_head_is_not_merge_commit(&repo_path)?;

    let commit_summary = git_stdout(&repo_path, &["log", "-1", "--pretty=%h %s", "HEAD"])?;

    if !force
        && !ui::prompt_confirm(
            &format!(
                "Undo last pushed commit '{}' by creating a new revert commit?",
                commit_summary
            ),
            false,
        )
    {
        ui::info("Undo cancelled");
        return Ok(target.repo_name);
    }

    revert_head_commit(&repo_path)?;
    push_branch(&repo_path, &target.branch)?;

    ui::success(&format!(
        "Created and pushed a revert for {}",
        commit_summary
    ));
    Ok(target.repo_name)
}

fn ensure_clean_worktree(repo_path: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo_path)
        .output()
        .context("Failed to check git status")?;

    if !output.status.success() {
        anyhow::bail!(
            "Failed to check git status: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    if !output.stdout.is_empty() {
        anyhow::bail!(
            "Local repository has uncommitted changes. Commit, stash, or discard them before running undo."
        );
    }

    Ok(())
}

fn fetch_origin_branch(repo_path: &Path, branch: &str) -> Result<()> {
    let output = Command::new("git")
        .args(["fetch", "origin", branch])
        .current_dir(repo_path)
        .output()
        .with_context(|| format!("Failed to fetch origin/{branch}"))?;

    if !output.status.success() {
        anyhow::bail!(
            "Failed to fetch origin/{branch}: {}. Push this profile first, or set [github].branch.",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}

/// Keep the working tree: create or reset the local branch to the current HEAD.
fn checkout_or_create_branch(repo_path: &Path, branch: &str) -> Result<()> {
    let current = git_stdout(repo_path, &["branch", "--show-current"]).unwrap_or_default();
    if current == branch {
        return Ok(());
    }

    let output = Command::new("git")
        .args(["checkout", "-B", branch])
        .current_dir(repo_path)
        .output()
        .with_context(|| format!("Failed to switch to branch {branch}"))?;

    if !output.status.success() {
        anyhow::bail!(
            "Failed to switch to branch '{branch}': {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}

/// Check out a branch that tracks origin/<branch> (used by pull/undo).
fn checkout_tracking_branch(repo_path: &Path, branch: &str) -> Result<()> {
    let current = git_stdout(repo_path, &["branch", "--show-current"]).unwrap_or_default();
    if current == branch {
        return Ok(());
    }

    let local_ref = format!("refs/heads/{branch}");
    let origin_ref = format!("origin/{branch}");
    let args: Vec<&str> = if git_ref_exists(repo_path, &local_ref)? {
        vec!["checkout", branch]
    } else {
        vec!["checkout", "-B", branch, origin_ref.as_str()]
    };

    let output = Command::new("git")
        .args(&args)
        .current_dir(repo_path)
        .output()
        .with_context(|| format!("Failed to switch to branch {branch}"))?;

    if !output.status.success() {
        anyhow::bail!(
            "Failed to switch to branch '{branch}': {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}

fn git_ref_exists(repo_path: &Path, git_ref: &str) -> Result<bool> {
    let output = Command::new("git")
        .args(["show-ref", "--verify", "--quiet", git_ref])
        .current_dir(repo_path)
        .output()
        .with_context(|| format!("Failed to verify git ref {}", git_ref))?;

    Ok(output.status.success())
}

fn fast_forward_to_origin(repo_path: &Path, branch: &str) -> Result<()> {
    let origin_ref = format!("origin/{branch}");
    let output = Command::new("git")
        .args(["merge", "--ff-only", origin_ref.as_str()])
        .current_dir(repo_path)
        .output()
        .with_context(|| format!("Failed to fast-forward local {branch} branch"))?;

    if !output.status.success() {
        anyhow::bail!(
            "Failed to fast-forward local {branch} branch: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}

fn ensure_head_matches_ref(repo_path: &Path, git_ref: &str) -> Result<()> {
    let head = git_stdout(repo_path, &["rev-parse", "HEAD"])?;
    let target = git_stdout(repo_path, &["rev-parse", git_ref])?;

    if head != target {
        anyhow::bail!(
            "Local repository is not aligned with {}. Run 'dotdipper pull' or clean up local commits before undoing the last push.",
            git_ref
        );
    }

    Ok(())
}

fn ensure_head_is_not_merge_commit(repo_path: &Path) -> Result<()> {
    let parents = git_stdout(repo_path, &["rev-list", "--parents", "-n", "1", "HEAD"])?;
    if parents.split_whitespace().count() > 2 {
        anyhow::bail!(
            "Undo does not support reverting a merge commit automatically. Revert it manually with git revert -m."
        );
    }

    Ok(())
}

fn revert_head_commit(repo_path: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["revert", "--no-edit", "HEAD"])
        .current_dir(repo_path)
        .output()
        .context("Failed to create revert commit")?;

    if !output.status.success() {
        let _ = Command::new("git")
            .args(["revert", "--abort"])
            .current_dir(repo_path)
            .output();
        anyhow::bail!(
            "Failed to create revert commit: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}

fn push_branch(repo_path: &Path, branch: &str) -> Result<()> {
    let output = Command::new("git")
        .args(["push", "origin", branch])
        .current_dir(repo_path)
        .output()
        .context("Failed to push revert commit")?;

    if !output.status.success() {
        anyhow::bail!(
            "Failed to push revert commit: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}

fn git_stdout(repo_path: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .output()
        .with_context(|| format!("Failed to run git {}", args.join(" ")))?;

    if !output.status.success() {
        anyhow::bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn ensure_github_repo(
    config: &Config,
    repo_path: &Path,
    username: &str,
    repo_name: &str,
) -> Result<()> {
    // Tests can point at a local bare repo via DOTDIPPER_TEST_REMOTE and skip gh.
    if std::env::var_os("DOTDIPPER_TEST_REMOTE").is_some() {
        add_remote(username, repo_name, repo_path)?;
        return Ok(());
    }

    check_gh()?;

    ui::info(&format!(
        "Creating GitHub repository: {}/{}",
        username, repo_name
    ));

    // Check if repo already exists
    let check_output = Command::new("gh")
        .args(["repo", "view", &format!("{}/{}", username, repo_name)])
        .output();

    if check_output.is_ok() && check_output.unwrap().status.success() {
        ui::info("Repository already exists on GitHub");
    } else {
        // Prompt to create repo
        if ui::prompt_confirm(
            &format!("Create private GitHub repository '{}'?", repo_name),
            true,
        ) {
            let mut create_args = vec!["repo", "create", repo_name];

            if config.github.private {
                create_args.push("--private");
            } else {
                create_args.push("--public");
            }

            create_args.push("--source");
            create_args.push(".");

            let output = Command::new("gh")
                .args(&create_args)
                .current_dir(repo_path)
                .output()
                .context("Failed to create GitHub repository")?;

            if !output.status.success() {
                anyhow::bail!(
                    "Failed to create repo: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }

            ui::success(&format!(
                "Created GitHub repository: {}/{}",
                username, repo_name
            ));
        } else {
            anyhow::bail!("Repository creation cancelled");
        }
    }

    // Always ensure remote URL matches selected repo
    add_remote(username, repo_name, repo_path)?;

    Ok(())
}

fn resolve_remote_url(username: &str, repo_name: &str) -> String {
    if let Ok(url) = std::env::var("DOTDIPPER_TEST_REMOTE") {
        let url = url.trim();
        if !url.is_empty() {
            return url.to_string();
        }
    }
    format!("git@github.com:{}/{}.git", username, repo_name)
}

fn add_remote(username: &str, repo_name: &str, repo_path: &Path) -> Result<()> {
    let remote_url = resolve_remote_url(username, repo_name);

    let output = Command::new("git")
        .args(["remote", "add", "origin", remote_url.as_str()])
        .current_dir(repo_path)
        .output()
        .context("Failed to add remote")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("already exists") {
            // Update existing remote
            let output = Command::new("git")
                .args(["remote", "set-url", "origin", remote_url.as_str()])
                .current_dir(repo_path)
                .output()
                .context("Failed to update remote")?;

            if !output.status.success() {
                anyhow::bail!(
                    "Failed to update remote: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        } else {
            anyhow::bail!("Failed to add remote: {}", stderr);
        }
    }

    Ok(())
}

fn clone_repo(username: &str, repo_name: &str, branch: &str, dest_path: &Path) -> Result<()> {
    let repo_url = resolve_remote_url(username, repo_name);

    ui::info(&format!(
        "Cloning repository from {} ({})",
        repo_url, branch
    ));

    // Create parent directory
    if let Some(parent) = dest_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let dest = dest_path
        .to_str()
        .context("Repository path is not valid UTF-8")?;
    let mut output = Command::new("git")
        .args([
            "clone",
            "--branch",
            branch,
            "--single-branch",
            repo_url.as_str(),
            dest,
        ])
        .output()
        .context("Failed to clone repository")?;

    if !output.status.success() {
        // Branch may not exist yet on a shared repo; clone default then create it.
        ui::info(&format!(
            "Branch '{}' not found on remote; cloning default branch",
            branch
        ));
        output = Command::new("git")
            .args(["clone", repo_url.as_str(), dest])
            .output()
            .context("Failed to clone repository")?;
        if !output.status.success() {
            anyhow::bail!(
                "Failed to clone: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        checkout_or_create_branch(dest_path, branch)?;
    }

    ui::success("Repository cloned successfully");
    Ok(())
}

fn get_github_username() -> Result<String> {
    let output = Command::new("gh")
        .args(["api", "user", "--jq", ".login"])
        .output()
        .context("Failed to get GitHub username")?;

    if !output.status.success() {
        anyhow::bail!("Failed to get GitHub username");
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn resolve_repo_name(config: &Config, repo_override: Option<&str>) -> String {
    repo_override
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| config.github.repo_name.clone())
        .unwrap_or_else(|| "dotfiles".to_string())
}

fn resolve_github_username(config: &Config) -> Result<String> {
    if let Some(username) = config.github.username.as_deref() {
        if !username.trim().is_empty() {
            return Ok(username.trim().to_string());
        }
    }

    if let Ok(username) = get_github_username() {
        if !username.trim().is_empty() {
            return Ok(username.trim().to_string());
        }
    }

    let username = ui::prompt_text("Enter your GitHub username:", None);
    if username.trim().is_empty() {
        anyhow::bail!("GitHub username is required");
    }

    Ok(username.trim().to_string())
}

fn write_push_gitignore(repo_path: &Path, config: &Config) -> Result<()> {
    let mut content = BASE_GITIGNORE.trim_end().to_string();
    let ignored = crate::cfg::resolve_push_ignored_paths(config)?;

    if !ignored.is_empty() {
        content.push_str("\n\n# Dotdipper push-ignore\n");
        for pattern in ignored {
            content.push_str(&pattern);
            content.push('\n');
        }
    } else {
        content.push('\n');
    }

    std::fs::write(repo_path.join(".gitignore"), content).context("Failed to update .gitignore")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn git(repo_path: &Path, args: &[&str]) -> std::process::Output {
        Command::new("git")
            .args(args)
            .current_dir(repo_path)
            .output()
            .unwrap()
    }

    fn git_ok(repo_path: &Path, args: &[&str]) {
        let output = git(repo_path, args);
        assert!(
            output.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn init_repo(repo_path: &Path) {
        git_ok(repo_path, &["init", "-b", "main"]);
        git_ok(repo_path, &["config", "user.email", "test@example.com"]);
        git_ok(repo_path, &["config", "user.name", "Dotdipper Tests"]);
    }

    #[test]
    fn revert_head_commit_restores_previous_contents() {
        if which::which("git").is_err() {
            return;
        }

        let temp_dir = TempDir::new().unwrap();
        init_repo(temp_dir.path());

        let tracked_file = temp_dir.path().join("dotfile.txt");
        fs::write(&tracked_file, "before\n").unwrap();
        git_ok(temp_dir.path(), &["add", "-A"]);
        git_ok(temp_dir.path(), &["commit", "-m", "Initial state"]);

        fs::write(&tracked_file, "after\n").unwrap();
        git_ok(temp_dir.path(), &["add", "-A"]);
        git_ok(temp_dir.path(), &["commit", "-m", "Update state"]);

        revert_head_commit(temp_dir.path()).unwrap();

        assert_eq!(fs::read_to_string(&tracked_file).unwrap(), "before\n");
        let subject = git_stdout(temp_dir.path(), &["log", "-1", "--pretty=%s"]).unwrap();
        assert!(subject.starts_with("Revert "));
    }

    #[test]
    fn ensure_clean_worktree_rejects_dirty_repo() {
        if which::which("git").is_err() {
            return;
        }

        let temp_dir = TempDir::new().unwrap();
        init_repo(temp_dir.path());

        let tracked_file = temp_dir.path().join("dirty.txt");
        fs::write(&tracked_file, "tracked\n").unwrap();
        git_ok(temp_dir.path(), &["add", "-A"]);
        git_ok(temp_dir.path(), &["commit", "-m", "Track file"]);

        fs::write(&tracked_file, "modified\n").unwrap();

        let err = ensure_clean_worktree(temp_dir.path()).unwrap_err();
        assert!(err.to_string().contains("uncommitted changes"));
    }

    #[test]
    fn ensure_head_matches_ref_detects_local_ahead_state() {
        if which::which("git").is_err() {
            return;
        }

        let remote_dir = TempDir::new().unwrap();
        let remote_output = Command::new("git")
            .args(["init", "--bare", "--initial-branch=main"])
            .current_dir(remote_dir.path())
            .output()
            .unwrap();
        assert!(remote_output.status.success());

        let local_dir = TempDir::new().unwrap();
        init_repo(local_dir.path());

        let tracked_file = local_dir.path().join("tracked.txt");
        fs::write(&tracked_file, "one\n").unwrap();
        git_ok(local_dir.path(), &["add", "-A"]);
        git_ok(local_dir.path(), &["commit", "-m", "Initial"]);
        git_ok(
            local_dir.path(),
            &[
                "remote",
                "add",
                "origin",
                remote_dir.path().to_str().unwrap(),
            ],
        );
        git_ok(local_dir.path(), &["push", "-u", "origin", "main"]);

        fs::write(&tracked_file, "two\n").unwrap();
        git_ok(local_dir.path(), &["add", "-A"]);
        git_ok(local_dir.path(), &["commit", "-m", "Ahead locally"]);

        let err = ensure_head_matches_ref(local_dir.path(), "origin/main").unwrap_err();
        assert!(err.to_string().contains("not aligned"));
    }

    #[test]
    fn undo_sequence_reverts_last_remote_commit_and_pushes_revert() {
        if which::which("git").is_err() {
            return;
        }

        let remote_dir = TempDir::new().unwrap();
        let remote_output = Command::new("git")
            .args(["init", "--bare", "--initial-branch=main"])
            .current_dir(remote_dir.path())
            .output()
            .unwrap();
        assert!(remote_output.status.success());

        let local_dir = TempDir::new().unwrap();
        init_repo(local_dir.path());

        let tracked_file = local_dir.path().join("tracked.txt");
        fs::write(&tracked_file, "before\n").unwrap();
        git_ok(local_dir.path(), &["add", "-A"]);
        git_ok(local_dir.path(), &["commit", "-m", "Initial"]);
        git_ok(
            local_dir.path(),
            &[
                "remote",
                "add",
                "origin",
                remote_dir.path().to_str().unwrap(),
            ],
        );
        git_ok(local_dir.path(), &["push", "-u", "origin", "main"]);

        fs::write(&tracked_file, "after\n").unwrap();
        git_ok(local_dir.path(), &["add", "-A"]);
        git_ok(local_dir.path(), &["commit", "-m", "Update"]);
        git_ok(local_dir.path(), &["push", "origin", "main"]);

        ensure_clean_worktree(local_dir.path()).unwrap();
        fetch_origin_branch(local_dir.path(), "main").unwrap();
        checkout_tracking_branch(local_dir.path(), "main").unwrap();
        fast_forward_to_origin(local_dir.path(), "main").unwrap();
        ensure_head_matches_ref(local_dir.path(), "origin/main").unwrap();
        ensure_head_is_not_merge_commit(local_dir.path()).unwrap();
        revert_head_commit(local_dir.path()).unwrap();
        push_branch(local_dir.path(), "main").unwrap();

        let inspect_root = TempDir::new().unwrap();
        let inspect_repo = inspect_root.path().join("inspect");
        let clone_output = Command::new("git")
            .args([
                "clone",
                remote_dir.path().to_str().unwrap(),
                inspect_repo.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(clone_output.status.success());

        assert_eq!(
            fs::read_to_string(inspect_repo.join("tracked.txt")).unwrap(),
            "before\n"
        );
        let subject = git_stdout(&inspect_repo, &["log", "-1", "--pretty=%s"]).unwrap();
        assert!(subject.starts_with("Revert "));
    }

    #[test]
    fn ensure_head_is_not_merge_commit_rejects_merge_commits() {
        if which::which("git").is_err() {
            return;
        }

        let temp_dir = TempDir::new().unwrap();
        init_repo(temp_dir.path());

        fs::write(temp_dir.path().join("base.txt"), "base\n").unwrap();
        git_ok(temp_dir.path(), &["add", "-A"]);
        git_ok(temp_dir.path(), &["commit", "-m", "Base"]);

        git_ok(temp_dir.path(), &["checkout", "-b", "feature"]);
        fs::write(temp_dir.path().join("feature.txt"), "feature\n").unwrap();
        git_ok(temp_dir.path(), &["add", "-A"]);
        git_ok(temp_dir.path(), &["commit", "-m", "Feature"]);

        git_ok(temp_dir.path(), &["checkout", "main"]);
        fs::write(temp_dir.path().join("main.txt"), "main\n").unwrap();
        git_ok(temp_dir.path(), &["add", "-A"]);
        git_ok(temp_dir.path(), &["commit", "-m", "Main"]);
        git_ok(
            temp_dir.path(),
            &["merge", "--no-ff", "feature", "-m", "Merge feature"],
        );

        let err = ensure_head_is_not_merge_commit(temp_dir.path()).unwrap_err();
        assert!(err.to_string().contains("merge commit"));
    }

    #[test]
    fn default_branch_for_default_profile_is_main() {
        assert_eq!(default_branch_for_profile("default"), "main");
        assert_eq!(default_branch_for_profile("work"), "dotdipper/work");
    }

    #[test]
    fn git_target_uses_explicit_branch_and_repo() {
        let mut config = crate::cfg::Config::default();
        config.github.username = Some("alice".into());
        config.github.repo_name = Some("dotfiles-work".into());
        config.github.branch = Some("main".into());
        let previous = std::env::var("DOTDIPPER_PROFILE").ok();
        std::env::set_var("DOTDIPPER_PROFILE", "work");
        let target = resolve_git_target(&config, None).unwrap();
        match previous {
            Some(value) => std::env::set_var("DOTDIPPER_PROFILE", value),
            None => std::env::remove_var("DOTDIPPER_PROFILE"),
        }
        assert_eq!(target.repo_name, "dotfiles-work");
        assert_eq!(target.branch, "main");
        assert_eq!(target.username, "alice");
    }

    #[test]
    fn git_target_defaults_profile_branch() {
        let mut config = crate::cfg::Config::default();
        config.github.username = Some("alice".into());
        let previous = std::env::var("DOTDIPPER_PROFILE").ok();
        std::env::set_var("DOTDIPPER_PROFILE", "work");
        let target = resolve_git_target(&config, None).unwrap();
        match previous {
            Some(value) => std::env::set_var("DOTDIPPER_PROFILE", value),
            None => std::env::remove_var("DOTDIPPER_PROFILE"),
        }
        assert_eq!(target.repo_name, "dotfiles");
        assert_eq!(target.branch, "dotdipper/work");
    }

    #[test]
    fn checkout_or_create_branch_keeps_worktree() {
        if which::which("git").is_err() {
            return;
        }

        let temp_dir = TempDir::new().unwrap();
        init_repo(temp_dir.path());
        fs::write(temp_dir.path().join("keep.txt"), "keep\n").unwrap();
        git_ok(temp_dir.path(), &["add", "-A"]);
        git_ok(temp_dir.path(), &["commit", "-m", "Initial"]);

        checkout_or_create_branch(temp_dir.path(), "dotdipper/work").unwrap();
        assert_eq!(
            git_stdout(temp_dir.path(), &["branch", "--show-current"]).unwrap(),
            "dotdipper/work"
        );
        assert_eq!(
            fs::read_to_string(temp_dir.path().join("keep.txt")).unwrap(),
            "keep\n"
        );
    }
}

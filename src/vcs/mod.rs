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

pub fn init_repo(repo_path: &Path) -> Result<()> {
    if repo_path.join(".git").exists() {
        return Ok(());
    }

    let output = Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(repo_path)
        .output()
        .context("Failed to initialize git repository")?;

    if !output.status.success() {
        anyhow::bail!(
            "Failed to initialize git repository: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    std::fs::write(repo_path.join(".gitignore"), BASE_GITIGNORE)?;

    Ok(())
}

pub fn push(
    config: &Config,
    message: Option<String>,
    force: bool,
    repo_override: Option<&str>,
) -> Result<String> {
    let repo_path = crate::paths::compiled_dir()?;
    let (username, repo_name) = resolve_identity(config, repo_override)?;

    // Ensure git is initialized
    init_repo(&repo_path)?;
    write_push_gitignore(&repo_path, config)?;

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
        ensure_git_identity(&repo_path)?;
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

    // Ensure the branch is named 'main'
    let branch_output = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(&repo_path)
        .output()
        .context("Failed to get current branch name")?;

    let current_branch = String::from_utf8_lossy(&branch_output.stdout)
        .trim()
        .to_string();
    if !current_branch.is_empty() && current_branch != "main" {
        let rename_output = Command::new("git")
            .args(["branch", "-M", "main"])
            .current_dir(&repo_path)
            .output()
            .context("Failed to rename branch to main")?;

        if !rename_output.status.success() {
            anyhow::bail!(
                "Failed to rename branch to main: {}",
                String::from_utf8_lossy(&rename_output.stderr)
            );
        }
    }

    if let Err(e) = ensure_github_repo(config, &repo_path, &username, &repo_name) {
        anyhow::bail!(
            "Could not create or access GitHub repo {}/{}: {:#}. Create it with `gh repo create` or check `gh auth login`.",
            username,
            repo_name,
            e
        );
    }

    // Push to remote
    let mut push_args = vec!["push", "origin", "main"];
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
                .args(["fetch", "origin", "main"])
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
                .args(["rebase", "origin/main"])
                .current_dir(&repo_path)
                .output()
                .context("Failed to rebase onto origin/main")?;
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
                .args(["push", "--set-upstream", "origin", "main"])
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

    Ok(repo_name)
}

pub fn pull(config: &Config, repo_override: Option<&str>) -> Result<String> {
    let repo_path = crate::paths::compiled_dir()?;
    let (username, repo_name) = resolve_identity(config, repo_override)?;

    // If repo doesn't exist, clone it
    if !repo_path.join(".git").exists() {
        clone_repo(&username, &repo_name, &repo_path)?;
    } else {
        // Ensure current origin points at the selected repo
        add_remote(&username, &repo_name, &repo_path)?;

        // Pull changes
        let output = Command::new("git")
            .args(["pull", "origin", "main"])
            .current_dir(&repo_path)
            .output()
            .context("Failed to pull from GitHub")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("no tracking information") {
                // Set tracking branch
                let output = Command::new("git")
                    .args(["branch", "--set-upstream-to=origin/main", "main"])
                    .current_dir(&repo_path)
                    .output()
                    .context("Failed to set tracking branch")?;

                if output.status.success() {
                    // Try pull again
                    let output = Command::new("git")
                        .args(["pull", "origin", "main"])
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

    if let Err(e) = crate::install::restore_artifacts_from_compiled() {
        ui::warn(&format!(
            "Pulled repo but could not restore install artifacts: {:#}",
            e
        ));
    }

    Ok(repo_name)
}

pub fn undo_last_push(config: &Config, force: bool, repo_override: Option<&str>) -> Result<String> {
    let repo_path = crate::paths::compiled_dir()?;
    let (username, repo_name) = resolve_identity(config, repo_override)?;

    if !repo_path.join(".git").exists() {
        clone_repo(&username, &repo_name, &repo_path)?;
    } else {
        add_remote(&username, &repo_name, &repo_path)?;
    }

    ensure_clean_worktree(&repo_path)?;
    fetch_origin_main(&repo_path)?;
    ensure_main_checked_out(&repo_path)?;
    fast_forward_main_to_origin(&repo_path)?;
    ensure_head_matches_ref(&repo_path, "origin/main")?;
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
        return Ok(repo_name);
    }

    revert_head_commit(&repo_path)?;
    push_main(&repo_path)?;

    ui::success(&format!(
        "Created and pushed a revert for {}",
        commit_summary
    ));
    Ok(repo_name)
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

fn fetch_origin_main(repo_path: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["fetch", "origin", "main"])
        .current_dir(repo_path)
        .output()
        .context("Failed to fetch origin/main")?;

    if !output.status.success() {
        anyhow::bail!(
            "Failed to fetch origin/main: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}

fn ensure_main_checked_out(repo_path: &Path) -> Result<()> {
    let current_branch = git_stdout(repo_path, &["branch", "--show-current"])?;
    if current_branch == "main" {
        return Ok(());
    }

    let args = if git_ref_exists(repo_path, "refs/heads/main")? {
        vec!["checkout", "main"]
    } else {
        vec!["checkout", "-B", "main", "origin/main"]
    };

    let output = Command::new("git")
        .args(&args)
        .current_dir(repo_path)
        .output()
        .context("Failed to switch to main branch")?;

    if !output.status.success() {
        anyhow::bail!(
            "Failed to switch to main branch: {}",
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

fn fast_forward_main_to_origin(repo_path: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["merge", "--ff-only", "origin/main"])
        .current_dir(repo_path)
        .output()
        .context("Failed to fast-forward local main branch")?;

    if !output.status.success() {
        anyhow::bail!(
            "Failed to fast-forward local main branch: {}",
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

fn push_main(repo_path: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["push", "origin", "main"])
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

fn github_remote_url(username: &str, repo_name: &str) -> String {
    match git_protocol() {
        GitProtocol::Ssh => format!("git@github.com:{}/{}.git", username, repo_name),
        GitProtocol::Https => format!("https://github.com/{}/{}.git", username, repo_name),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GitProtocol {
    Https,
    Ssh,
}

fn git_protocol() -> GitProtocol {
    if let Ok(output) = Command::new("gh")
        .args(["config", "get", "git_protocol"])
        .output()
    {
        if output.status.success() {
            let proto = String::from_utf8_lossy(&output.stdout)
                .trim()
                .to_ascii_lowercase();
            if proto == "ssh" {
                return GitProtocol::Ssh;
            }
            if proto == "https" {
                return GitProtocol::Https;
            }
        }
    }
    // New machines almost always have `gh auth login` (HTTPS) before SSH keys.
    GitProtocol::Https
}

/// Parse `owner/repo`, a repo name, or a GitHub URL into (optional owner, name).
pub fn parse_github_repo(spec: &str) -> (Option<String>, String) {
    let spec = spec.trim();
    let spec = spec.trim_end_matches(".git");
    let spec = spec
        .trim_start_matches("https://github.com/")
        .trim_start_matches("http://github.com/")
        .trim_start_matches("git@github.com:");
    if let Some((owner, name)) = spec.split_once('/') {
        let owner = owner.trim();
        let name = name.trim();
        if !owner.is_empty() && !name.is_empty() {
            return (Some(owner.to_string()), name.to_string());
        }
    }
    (None, spec.to_string())
}

fn add_remote(username: &str, repo_name: &str, repo_path: &Path) -> Result<()> {
    let remote_url = github_remote_url(username, repo_name);

    let output = Command::new("git")
        .args(["remote", "add", "origin", remote_url.as_str()])
        .current_dir(repo_path)
        .output()
        .context("Failed to add remote")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("already exists") {
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

fn clone_repo(username: &str, repo_name: &str, dest_path: &Path) -> Result<()> {
    if let Some(parent) = dest_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let spec = format!("{}/{}", username, repo_name);
    ui::info(&format!("Cloning {}", spec));

    // Prefer `gh repo clone` — uses the user's authenticated protocol (HTTPS or SSH).
    if Command::new("gh")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        let dest = dest_path.to_str().context("Invalid destination path")?;
        let output = Command::new("gh")
            .args(["repo", "clone", spec.as_str(), dest])
            .output()
            .context("Failed to clone repository with gh")?;
        if output.status.success() {
            ui::success("Repository cloned successfully");
            return Ok(());
        }
        ui::warn(&format!(
            "gh repo clone failed ({}); trying git clone",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let https_url = format!("https://github.com/{}/{}.git", username, repo_name);
    let ssh_url = format!("git@github.com:{}/{}.git", username, repo_name);
    let dest = dest_path.to_str().context("Invalid destination path")?;

    let https = Command::new("git")
        .args(["clone", https_url.as_str(), dest])
        .output()
        .context("Failed to clone repository")?;
    if https.status.success() {
        ui::success("Repository cloned successfully");
        return Ok(());
    }

    ui::warn("HTTPS clone failed; trying SSH");
    let ssh = Command::new("git")
        .args(["clone", ssh_url.as_str(), dest])
        .output()
        .context("Failed to clone repository")?;
    if ssh.status.success() {
        ui::success("Repository cloned successfully");
        return Ok(());
    }

    anyhow::bail!(
        "Failed to clone {}/{}. Authenticate with `gh auth login` (HTTPS) or set up an SSH key.\nHTTPS: {}\nSSH: {}",
        username,
        repo_name,
        String::from_utf8_lossy(&https.stderr).trim(),
        String::from_utf8_lossy(&ssh.stderr).trim()
    );
}

fn git_config_value(repo_path: &Path, key: &str) -> Option<String> {
    let output = Command::new("git")
        .args(["config", "--get", key])
        .current_dir(repo_path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn set_git_config(repo_path: &Path, key: &str, value: &str) -> Result<()> {
    let output = Command::new("git")
        .args(["config", key, value])
        .current_dir(repo_path)
        .output()
        .with_context(|| format!("Failed to set git config {}", key))?;
    if !output.status.success() {
        anyhow::bail!(
            "Failed to set git config {}: {}",
            key,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

/// New machines often have `gh auth login` but no `user.name`/`user.email`.
fn ensure_git_identity(repo_path: &Path) -> Result<()> {
    let mut name = git_config_value(repo_path, "user.name");
    let mut email = git_config_value(repo_path, "user.email");
    if name.is_some() && email.is_some() {
        return Ok(());
    }

    if let Ok(login) = get_github_username() {
        if name.is_none() {
            set_git_config(repo_path, "user.name", &login)?;
            name = Some(login.clone());
        }
        if email.is_none() {
            let generated = format!("{}@users.noreply.github.com", login);
            set_git_config(repo_path, "user.email", &generated)?;
            email = Some(generated);
        }
    }

    if name.is_none() || email.is_none() {
        anyhow::bail!(
            "Git user.name and user.email must be set to commit. Run `git config --global user.name \"Your Name\"` and `git config --global user.email you@example.com`, or authenticate with `gh auth login`."
        );
    }
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

pub fn resolve_identity(config: &Config, repo_override: Option<&str>) -> Result<(String, String)> {
    let parsed = repo_override.map(parse_github_repo);
    let username = match parsed.as_ref().and_then(|(owner, _)| owner.clone()) {
        Some(owner) => owner,
        None => resolve_github_username(config)?,
    };
    let repo_name = match parsed {
        Some((_, name)) if !name.is_empty() => name,
        _ => resolve_repo_name(config, None),
    };
    Ok((username, repo_name))
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
        fetch_origin_main(local_dir.path()).unwrap();
        ensure_main_checked_out(local_dir.path()).unwrap();
        fast_forward_main_to_origin(local_dir.path()).unwrap();
        ensure_head_matches_ref(local_dir.path(), "origin/main").unwrap();
        ensure_head_is_not_merge_commit(local_dir.path()).unwrap();
        revert_head_commit(local_dir.path()).unwrap();
        push_main(local_dir.path()).unwrap();

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
    fn parse_github_repo_accepts_owner_name_and_urls() {
        assert_eq!(
            parse_github_repo("alice/dotfiles"),
            (Some("alice".into()), "dotfiles".into())
        );
        assert_eq!(
            parse_github_repo("https://github.com/alice/dotfiles.git"),
            (Some("alice".into()), "dotfiles".into())
        );
        assert_eq!(
            parse_github_repo("git@github.com:alice/dotfiles.git"),
            (Some("alice".into()), "dotfiles".into())
        );
        assert_eq!(parse_github_repo("dotfiles"), (None, "dotfiles".into()));
        assert_eq!(
            parse_github_repo("  bob/my-dots  "),
            (Some("bob".into()), "my-dots".into())
        );
    }
}

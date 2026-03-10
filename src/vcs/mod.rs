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
    let repo_name = resolve_repo_name(config, repo_override);
    let username = resolve_github_username(config)?;

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
        ui::warn(&format!("Could not create GitHub repo: {}", e));
        ui::hint("Create a GitHub repository manually and add it as a remote");
        return Ok(repo_name);
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
        if stderr.contains("failed to push") || stderr.contains("rejected") {
            // Try to set upstream branch
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
    let repo_name = resolve_repo_name(config, repo_override);
    let username = resolve_github_username(config)?;

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

    Ok(repo_name)
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

fn add_remote(username: &str, repo_name: &str, repo_path: &Path) -> Result<()> {
    let remote_url = format!("git@github.com:{}/{}.git", username, repo_name);

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

fn clone_repo(username: &str, repo_name: &str, dest_path: &Path) -> Result<()> {
    let repo_url = format!("git@github.com:{}/{}.git", username, repo_name);

    ui::info(&format!("Cloning repository from {}", repo_url));

    // Create parent directory
    if let Some(parent) = dest_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let output = Command::new("git")
        .args(["clone", repo_url.as_str(), dest_path.to_str().unwrap()])
        .output()
        .context("Failed to clone repository")?;

    if !output.status.success() {
        anyhow::bail!(
            "Failed to clone: {}",
            String::from_utf8_lossy(&output.stderr)
        );
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

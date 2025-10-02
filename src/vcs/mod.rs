use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

use crate::cfg::Config;
use crate::ui;

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
        .arg("init")
        .current_dir(repo_path)
        .output()
        .context("Failed to initialize git repository")?;
    
    if !output.status.success() {
        anyhow::bail!("Failed to initialize git repository: {}", 
            String::from_utf8_lossy(&output.stderr));
    }
    
    // Create .gitignore
    let gitignore = r#"# Temporary files
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
    
    std::fs::write(repo_path.join(".gitignore"), gitignore)?;
    
    Ok(())
}

pub fn push(config: &Config, message: Option<String>, force: bool) -> Result<()> {
    let repo_path = dirs::home_dir()
        .context("Failed to find home directory")?
        .join(".dotdipper")
        .join("compiled");
    
    // Ensure git is initialized
    init_repo(&repo_path)?;
    
    // Add all files
    let output = Command::new("git")
        .args(&["add", "-A"])
        .current_dir(&repo_path)
        .output()
        .context("Failed to add files to git")?;
    
    if !output.status.success() {
        anyhow::bail!("Failed to add files: {}", String::from_utf8_lossy(&output.stderr));
    }
    
    // Check if there are changes to commit
    let status_output = Command::new("git")
        .args(&["status", "--porcelain"])
        .current_dir(&repo_path)
        .output()
        .context("Failed to check git status")?;
    
    if status_output.stdout.is_empty() {
        ui::info("No changes to commit");
    } else {
        // Commit changes
        let commit_message = message.unwrap_or_else(|| {
            format!("Update dotfiles - {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"))
        });
        
        let output = Command::new("git")
            .args(&["commit", "-m", &commit_message])
            .current_dir(&repo_path)
            .output()
            .context("Failed to commit changes")?;
        
        if !output.status.success() {
            anyhow::bail!("Failed to commit: {}", String::from_utf8_lossy(&output.stderr));
        }
        
        ui::success("Changes committed");
    }
    
    // Check if remote exists
    let remote_output = Command::new("git")
        .args(&["remote", "get-url", "origin"])
        .current_dir(&repo_path)
        .output();
    
    if remote_output.is_err() || !remote_output.unwrap().status.success() {
        // No remote, try to create GitHub repo
        if let Err(e) = create_github_repo(config, &repo_path) {
            ui::warn(&format!("Could not create GitHub repo: {}", e));
            ui::hint("Create a GitHub repository manually and add it as a remote");
            return Ok(());
        }
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
                .args(&["push", "--set-upstream", "origin", "main"])
                .current_dir(&repo_path)
                .output()
                .context("Failed to set upstream branch")?;
            
            if !output.status.success() {
                anyhow::bail!("Failed to push: {}", String::from_utf8_lossy(&output.stderr));
            }
        } else {
            anyhow::bail!("Failed to push: {}", stderr);
        }
    }
    
    Ok(())
}

pub fn pull(config: &Config) -> Result<()> {
    let repo_path = dirs::home_dir()
        .context("Failed to find home directory")?
        .join(".dotdipper")
        .join("compiled");
    
    // If repo doesn't exist, clone it
    if !repo_path.join(".git").exists() {
        if let Some(ref username) = config.github.username {
            let repo_name = config.github.repo_name.as_deref().unwrap_or("dotfiles");
            clone_repo(username, repo_name, &repo_path)?;
        } else {
            anyhow::bail!("No GitHub username configured. Run 'dotdipper config --edit' to set it.");
        }
    } else {
        // Pull changes
        let output = Command::new("git")
            .args(&["pull", "origin", "main"])
            .current_dir(&repo_path)
            .output()
            .context("Failed to pull from GitHub")?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("no tracking information") {
                // Set tracking branch
                let output = Command::new("git")
                    .args(&["branch", "--set-upstream-to=origin/main", "main"])
                    .current_dir(&repo_path)
                    .output()
                    .context("Failed to set tracking branch")?;
                
                if output.status.success() {
                    // Try pull again
                    let output = Command::new("git")
                        .args(&["pull", "origin", "main"])
                        .current_dir(&repo_path)
                        .output()
                        .context("Failed to pull from GitHub")?;
                    
                    if !output.status.success() {
                        anyhow::bail!("Failed to pull: {}", String::from_utf8_lossy(&output.stderr));
                    }
                } else {
                    anyhow::bail!("Failed to set tracking branch: {}", String::from_utf8_lossy(&output.stderr));
                }
            } else {
                anyhow::bail!("Failed to pull: {}", stderr);
            }
        }
    }
    
    Ok(())
}

fn create_github_repo(config: &Config, repo_path: &Path) -> Result<()> {
    check_gh()?;
    
    let username_string;
    let username = if let Some(u) = config.github.username.as_deref() {
        u
    } else if let Ok(u) = get_github_username() {
        username_string = u;
        username_string.as_str()
    } else {
        username_string = ui::prompt_text("Enter your GitHub username:", None);
        username_string.as_str()
    };
    
    if username.is_empty() {
        anyhow::bail!("GitHub username is required");
    }
    
    let repo_name = config.github.repo_name.as_deref()
        .unwrap_or("dotfiles");
    
    ui::info(&format!("Creating GitHub repository: {}/{}", username, repo_name));
    
    // Check if repo already exists
    let check_output = Command::new("gh")
        .args(&["repo", "view", &format!("{}/{}", username, repo_name)])
        .output();
    
    if check_output.is_ok() && check_output.unwrap().status.success() {
        ui::info("Repository already exists on GitHub");
        
        // Add as remote
        add_remote(&username, repo_name, repo_path)?;
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
                anyhow::bail!("Failed to create repo: {}", String::from_utf8_lossy(&output.stderr));
            }
            
            ui::success(&format!("Created GitHub repository: {}/{}", username, repo_name));
        } else {
            anyhow::bail!("Repository creation cancelled");
        }
    }
    
    Ok(())
}

fn add_remote(username: &str, repo_name: &str, repo_path: &Path) -> Result<()> {
    let remote_url = format!("git@github.com:{}/{}.git", username, repo_name);
    
    let output = Command::new("git")
        .args(&["remote", "add", "origin", &remote_url])
        .current_dir(repo_path)
        .output()
        .context("Failed to add remote")?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("already exists") {
            // Update existing remote
            let output = Command::new("git")
                .args(&["remote", "set-url", "origin", &remote_url])
                .current_dir(repo_path)
                .output()
                .context("Failed to update remote")?;
            
            if !output.status.success() {
                anyhow::bail!("Failed to update remote: {}", String::from_utf8_lossy(&output.stderr));
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
        .args(&["clone", &repo_url, dest_path.to_str().unwrap()])
        .output()
        .context("Failed to clone repository")?;
    
    if !output.status.success() {
        anyhow::bail!("Failed to clone: {}", String::from_utf8_lossy(&output.stderr));
    }
    
    ui::success("Repository cloned successfully");
    Ok(())
}

fn get_github_username() -> Result<String> {
    let output = Command::new("gh")
        .args(&["api", "user", "--jq", ".login"])
        .output()
        .context("Failed to get GitHub username")?;
    
    if !output.status.success() {
        anyhow::bail!("Failed to get GitHub username");
    }
    
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

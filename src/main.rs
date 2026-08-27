#[cfg(target_os = "macos")]
use dotdipper::apps;
use dotdipper::cfg;
use dotdipper::daemon;
use dotdipper::diff;
use dotdipper::hash;
use dotdipper::install;
use dotdipper::profiles;
use dotdipper::remote;
use dotdipper::repo;
use dotdipper::scan;
use dotdipper::secrets;
use dotdipper::snapshots;
use dotdipper::ui;
use dotdipper::vcs;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
use std::path::PathBuf;

/// Dotdipper - A smart dotfiles manager with GitHub sync and machine bootstrapping
#[derive(Parser)]
#[command(name = "dotdipper")]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Path to config file (defaults to ~/.config/dotdipper/config.toml)
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize dotdipper in the current directory
    Init {
        /// Force initialization even if config exists
        #[arg(short, long)]
        force: bool,
    },

    /// Discover dotfiles on the system
    Discover {
        /// Write discovered files to config
        #[arg(long)]
        write: bool,

        /// Show all files including ignored ones
        #[arg(long)]
        all: bool,

        /// Also discover required packages from dotfiles
        #[arg(long)]
        packages: bool,

        /// Target OS for package discovery (auto-detected if not specified)
        #[arg(long)]
        target_os: Option<String>,

        /// Include low-confidence package matches
        #[arg(long)]
        include_low_confidence: bool,

        /// Validate if discovered packages are already installed
        #[arg(long)]
        validate: bool,
    },

    /// Show status of dotfiles (changes since last snapshot)
    Status {
        /// Show detailed diff
        #[arg(long)]
        detailed: bool,
    },

    /// Show differences between compiled and system files
    Diff {
        /// Show detailed diff for each file
        #[arg(long)]
        detailed: bool,
    },

    /// Apply dotfiles to system
    Apply {
        /// Force overwrite without prompting
        #[arg(short, long)]
        force: bool,

        /// Interactive selection of files to apply
        #[arg(short, long)]
        interactive: bool,

        /// Only apply specific paths (comma-separated)
        #[arg(long)]
        only: Option<String>,

        /// Allow operations outside $HOME (unsafe)
        #[arg(long)]
        unsafe_allow_outside_home: bool,
    },

    /// Manage encrypted secrets
    #[command(subcommand)]
    Secrets(SecretsCommands),

    /// Manage snapshots (create, list, rollback, delete)
    #[command(subcommand)]
    Snapshot(SnapshotCommands),

    /// Manage profiles
    #[command(subcommand)]
    Profile(ProfileCommands),

    /// Manage remote backups
    #[command(subcommand)]
    Remote(RemoteCommands),

    /// Control auto-sync daemon
    #[command(subcommand)]
    Daemon(DaemonCommands),

    /// Push dotfiles to GitHub
    Push {
        /// Commit message
        #[arg(short, long)]
        message: Option<String>,

        /// Force push
        #[arg(short, long)]
        force: bool,

        /// Override the GitHub repository name (e.g. 'dotfiles-dotdipper')
        #[arg(long)]
        repo: Option<String>,
    },

    /// Pull dotfiles from GitHub
    Pull {
        /// Apply pulled changes to system (creates backups when general.backup = true)
        #[arg(long)]
        apply: bool,

        /// Discard uncommitted changes in the local compiled/ git store (HOME is untouched unless --apply)
        #[arg(short, long)]
        force: bool,

        /// Allow operations outside $HOME (unsafe)
        #[arg(long)]
        unsafe_allow_outside_home: bool,

        /// Override the GitHub repository name
        #[arg(long)]
        repo: Option<String>,
    },

    /// Undo the last pushed commit by creating a revert commit
    Undo {
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,

        /// Override the GitHub repository name
        #[arg(long)]
        repo: Option<String>,
    },

    /// Generate and run installation scripts
    Install {
        /// Print or export the generated setup_dotfiles.sh
        #[command(subcommand)]
        action: Option<InstallCommands>,

        /// Only generate scripts without running
        #[arg(long)]
        dry_run: bool,

        /// Target OS (auto-detected if not specified)
        #[arg(long)]
        target_os: Option<String>,

        /// Allow operations outside $HOME (unsafe)
        #[arg(long)]
        unsafe_allow_outside_home: bool,
    },

    /// Run diagnostics and check system health
    Doctor {
        /// Fix issues automatically where possible
        #[arg(long)]
        fix: bool,
    },

    /// Edit or view configuration
    Config {
        /// Open config in editor
        #[arg(long)]
        edit: bool,

        /// Show current configuration
        #[arg(long)]
        show: bool,

        /// Set a config value (format: key=value, e.g. github.repo_name=dotfiles-dotdipper)
        #[arg(long, value_name = "KEY=VALUE")]
        set: Option<String>,
    },

    /// Manage push-ignore patterns
    #[command(subcommand)]
    Ignore(IgnoreCommands),

    /// Capture and restore installed macOS applications
    #[cfg(target_os = "macos")]
    #[command(subcommand)]
    Apps(AppsCommands),
}

#[derive(Subcommand)]
enum SecretsCommands {
    /// Initialize secrets management (generate/import keys)
    Init,

    /// Encrypt a file
    Encrypt {
        /// Path to file to encrypt
        path: PathBuf,

        /// Output path (defaults to <path>.age or <path>.sops.<ext> for SOPS)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Decrypt a file
    Decrypt {
        /// Path to encrypted file
        path: PathBuf,

        /// Output path (defaults to stripping .age / .sops.* suffix)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Edit an encrypted file (decrypt, edit, re-encrypt)
    Edit {
        /// Path to encrypted file
        path: PathBuf,
    },
}

#[derive(Subcommand)]
enum SnapshotCommands {
    /// Create a new snapshot
    Create {
        /// Snapshot description
        #[arg(short, long)]
        message: Option<String>,

        /// Force snapshot even if no changes detected
        #[arg(short, long)]
        force: bool,
    },

    /// List all snapshots
    List,

    /// Rollback to a snapshot
    Rollback {
        /// Snapshot ID
        id: String,

        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },

    /// Delete a snapshot
    Delete {
        /// Snapshot ID
        id: String,

        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },

    /// Prune old snapshots based on criteria
    Prune {
        /// Keep N most recent snapshots
        #[arg(long)]
        keep_count: Option<usize>,

        /// Keep snapshots newer than duration (e.g., "30d", "7d", "2w", "1m")
        #[arg(long)]
        keep_age: Option<String>,

        /// Keep snapshots until total size is under limit (e.g., "1GB", "500MB")
        #[arg(long)]
        keep_size: Option<String>,

        /// Show what would be deleted without actually deleting
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
enum ProfileCommands {
    /// List all profiles
    List,

    /// Create a new profile
    Create {
        /// Profile name
        name: String,
    },

    /// Switch to a profile
    Switch {
        /// Profile name
        name: String,
    },

    /// Remove a profile
    Remove {
        /// Profile name
        name: String,

        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum RemoteCommands {
    /// Configure remote backend
    Set {
        /// Remote kind (localfs, s3, gcs, webdav)
        kind: String,

        /// Endpoint URL or path (required for localfs, webdav)
        #[arg(long)]
        endpoint: Option<String>,

        /// S3 bucket name (required for s3)
        #[arg(long)]
        bucket: Option<String>,

        /// AWS region (for s3, defaults to us-east-1)
        #[arg(long)]
        region: Option<String>,

        /// Prefix/path within bucket or endpoint
        #[arg(long)]
        prefix: Option<String>,
    },

    /// Show remote configuration
    Show,

    /// Push to remote
    Push {
        /// Dry run (don't actually push)
        #[arg(long)]
        dry_run: bool,
    },

    /// Pull from remote
    Pull,
}

#[derive(Subcommand)]
enum DaemonCommands {
    /// Start the daemon
    Start,

    /// Stop the daemon
    Stop,

    /// Check daemon status
    Status,

    /// Enable the daemon in configuration
    Enable,

    /// Disable the daemon in configuration
    Disable,
}

#[derive(Subcommand)]
enum IgnoreCommands {
    /// Add a pattern to push-ignore
    Add {
        /// Pattern to ignore on git push (e.g. ~/.config/karabiner/**)
        pattern: String,
    },

    /// Remove a pattern from push-ignore
    Remove {
        /// Pattern to remove
        pattern: String,
    },

    /// List all effective push-ignore patterns
    List,
}

#[derive(Subcommand)]
enum InstallCommands {
    /// Print or export the generated setup_dotfiles.sh from tracked files / manifest
    Script {
        /// Write the script to a file instead of printing to stdout
        #[arg(short = 'o', long = "out", value_name = "PATH")]
        out: Option<PathBuf>,
    },
}

#[cfg(target_os = "macos")]
#[derive(Subcommand)]
enum AppsCommands {
    /// Capture Homebrew, Mac App Store, and /Applications state
    Capture,

    /// Show the captured apps manifest
    List,

    /// Restore Homebrew packages from the captured Brewfile
    Install {
        /// Print what would be installed without running brew bundle
        #[arg(long)]
        dry_run: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Set up logging/verbosity
    if cli.verbose {
        std::env::set_var("RUST_LOG", "debug");
    }

    // Initialize UI module
    ui::init();

    // Get or create config
    let config_path = cli.config.unwrap_or_else(|| {
        dotdipper::paths::config_file().expect("Could not determine dotdipper config path")
    });

    let result = match cli.command {
        Commands::Init { force } => cmd_init(config_path, force).await,
        Commands::Discover {
            write,
            all,
            packages,
            target_os,
            include_low_confidence,
            validate,
        } => {
            cmd_discover(
                config_path,
                write,
                all,
                packages,
                target_os,
                include_low_confidence,
                validate,
            )
            .await
        }
        Commands::Status { detailed } => cmd_status(config_path, detailed).await,
        Commands::Diff { detailed } => cmd_diff(config_path, detailed).await,
        Commands::Apply {
            force,
            interactive,
            only,
            unsafe_allow_outside_home,
        } => {
            cmd_apply(
                config_path,
                force,
                interactive,
                only,
                unsafe_allow_outside_home,
            )
            .await
        }
        Commands::Secrets(subcmd) => cmd_secrets(config_path, subcmd).await,
        Commands::Snapshot(subcmd) => cmd_snapshot(config_path, subcmd).await,
        Commands::Profile(subcmd) => cmd_profile(config_path, subcmd).await,
        Commands::Remote(subcmd) => cmd_remote(config_path, subcmd).await,
        Commands::Daemon(subcmd) => cmd_daemon(config_path, subcmd).await,
        Commands::Push {
            message,
            force,
            repo,
        } => cmd_push(config_path, message, force, repo).await,
        Commands::Pull {
            apply,
            force,
            unsafe_allow_outside_home,
            repo,
        } => cmd_pull(config_path, apply, force, unsafe_allow_outside_home, repo).await,
        Commands::Undo { force, repo } => cmd_undo(config_path, force, repo).await,
        Commands::Install {
            action,
            dry_run,
            target_os,
            unsafe_allow_outside_home,
        } => match action {
            Some(InstallCommands::Script { out }) => cmd_install_script(config_path, out).await,
            None => cmd_install(config_path, dry_run, target_os, unsafe_allow_outside_home).await,
        },
        Commands::Doctor { fix } => cmd_doctor(config_path, fix).await,
        Commands::Config { edit, show, set } => cmd_config(config_path, edit, show, set).await,
        Commands::Ignore(subcmd) => cmd_ignore(config_path, subcmd).await,
        #[cfg(target_os = "macos")]
        Commands::Apps(subcmd) => cmd_apps(config_path, subcmd).await,
    };

    if let Err(e) = result {
        ui::error(&format!("Error: {:#}", e));
        std::process::exit(1);
    }

    Ok(())
}

async fn cmd_init(config_path: PathBuf, force: bool) -> Result<()> {
    ui::info("Initializing dotdipper...");
    cfg::init(config_path, force)?;
    ui::success("Dotdipper initialized successfully!");
    ui::hint("Run 'dotdipper discover --write' to find and add dotfiles to track");
    Ok(())
}

async fn cmd_discover(
    config_path: PathBuf,
    write: bool,
    all: bool,
    packages: bool,
    target_os: Option<String>,
    include_low_confidence: bool,
    validate: bool,
) -> Result<()> {
    ui::info("Discovering dotfiles...");
    let config = cfg::load(&config_path)?;
    let discovered = scan::discover(&config, all)?;

    ui::info(&format!("Found {} dotfiles", discovered.len()));

    // Handle package discovery if requested
    if packages {
        ui::info("Discovering required packages from dotfiles...");

        let os = target_os.unwrap_or_else(install::detect_os);
        ui::info(&format!("Target OS: {}", os));

        let discovery_config = install::DiscoveryConfig {
            target_os: os.clone(),
            include_low_confidence,
            custom_mappings: std::collections::HashMap::new(),
            exclude_patterns: config.exclude_patterns.clone(),
        };

        let result = install::discover::discover_packages(&config, &discovery_config)?;

        // Display discovered packages
        if result.has_packages() {
            ui::section("Discovered Packages:");
            let display_list = install::discover::get_package_display_list(&result);

            for (binary, package, confidence) in &display_list {
                if binary == package {
                    println!("  {} ({})", binary.green(), confidence.dimmed());
                } else {
                    println!(
                        "  {} -> {} ({})",
                        binary,
                        package.green(),
                        confidence.dimmed()
                    );
                }
            }

            println!();
            ui::info(&format!(
                "Found {} unique packages from {} binaries",
                result.unique_packages().len(),
                result.packages.len()
            ));
        } else {
            ui::info("No packages discovered from tracked dotfiles");
        }

        // Show unmapped binaries
        if !result.unmapped_binaries.is_empty() {
            println!();
            ui::warn("Unmapped binaries (not in package database):");
            for binary in &result.unmapped_binaries {
                println!("  {}", binary.yellow());
            }
        }

        // Show errors
        if result.has_errors() {
            println!();
            ui::warn("Errors during analysis:");
            for (path, error) in &result.errors {
                println!("  {}: {}", path.display(), error.red());
            }
        }

        // Validate packages if requested
        if validate && result.has_packages() {
            println!();
            ui::info("Validating package installation status...");

            let validation = install::validators::validate_packages(&result)?;

            if !validation.installed.is_empty() {
                ui::success(&format!(
                    "{} packages already installed",
                    validation.installed.len()
                ));
            }

            if !validation.missing.is_empty() {
                ui::warn(&format!(
                    "{} packages need installation:",
                    validation.missing.len()
                ));
                for binary in &validation.missing {
                    if let Some(package) = result.packages.get(binary) {
                        let instruction =
                            install::validators::get_install_instructions(package, &os);
                        println!("    {} -> {}", binary.red(), instruction.dimmed());
                    }
                }
            }
        }

        // Write packages to config if requested
        if write && result.has_packages() {
            install::discover::update_config_with_packages(&config_path, &result)?;
            ui::success("Updated configuration with discovered packages");
        } else if result.has_packages() && !write {
            println!();
            ui::hint("Use --write to add discovered packages to your configuration");
        }
    }

    // Handle file discovery display and write
    if !packages || write {
        if write {
            cfg::update_discovered(&config_path, &discovered)?;
            ui::success("Updated configuration with discovered files");
        } else if !packages {
            // Only show file list if not doing package discovery
            for file in discovered.iter().take(10) {
                println!("  {}", file.display().to_string().dimmed());
            }
            if discovered.len() > 10 {
                println!("  ... and {} more", discovered.len() - 10);
            }
            ui::hint("Use --write to add these files to your configuration");
        }
    }

    Ok(())
}

async fn cmd_snapshot_create(
    config_path: PathBuf,
    force: bool,
    message: Option<String>,
) -> Result<()> {
    ui::info("Creating snapshot...");
    let config = cfg::load(&config_path)?;

    // Run pre-snapshot hooks
    if let Some(hooks) = &config.hooks {
        for hook in &hooks.pre_snapshot {
            ui::info(&format!("Running pre-snapshot hook: {}", hook));
            run_hook(hook)?;
        }
    }

    // First, compile tracked files into the compiled directory
    let snapshot_result = repo::snapshot(&config, force)?;
    ui::success(&format!("Compiled {} files", snapshot_result.file_count));

    // Then create a versioned snapshot with the message
    snapshots::create(&config, message)?;

    // Run post-snapshot hooks
    if let Some(hooks) = &config.hooks {
        for hook in &hooks.post_snapshot {
            ui::info(&format!("Running post-snapshot hook: {}", hook));
            run_hook(hook)?;
        }
    }

    Ok(())
}

async fn cmd_status(config_path: PathBuf, detailed: bool) -> Result<()> {
    ui::info("Checking status...");
    let config = cfg::load(&config_path)?;
    let status = repo::status(&config)?;

    if status.is_clean() {
        ui::success("No changes detected - everything is up to date!");
    } else {
        ui::warn(&format!(
            "Changes detected: {} modified, {} added, {} deleted",
            status.modified.len(),
            status.added.len(),
            status.deleted.len()
        ));

        if detailed {
            status.print_detailed();
        }
    }

    Ok(())
}

async fn cmd_push(
    config_path: PathBuf,
    message: Option<String>,
    force: bool,
    repo: Option<String>,
) -> Result<()> {
    ui::info("Pushing to GitHub...");
    let config = cfg::load(&config_path)?;

    // Create snapshot first
    repo::snapshot(&config, false)?;

    #[cfg(target_os = "macos")]
    {
        let capture_on_push = config
            .apps
            .as_ref()
            .map(|apps| apps.capture_on_push)
            .unwrap_or(true);
        if capture_on_push {
            ui::info("Capturing installed applications...");
            match apps::capture(&config) {
                Ok(result) => {
                    ui::success(&format_apps_capture_success(&result));
                }
                Err(e) => {
                    ui::warn(&format!(
                        "App capture failed (continuing with push): {:#}",
                        e
                    ));
                }
            }
        }
    }

    // Push to GitHub
    let effective_repo = vcs::push(&config, message, force, repo.as_deref())?;

    if repo.is_some() && config.github.repo_name.is_none() {
        cfg::set_config_value(&config_path, "github.repo_name", &effective_repo)?;
        ui::info(&format!(
            "Saved '{}' as default GitHub repository in config",
            effective_repo
        ));
    }

    ui::success("Successfully pushed to GitHub!");
    Ok(())
}

async fn cmd_pull(
    config_path: PathBuf,
    apply: bool,
    force: bool,
    allow_outside_home: bool,
    repo: Option<String>,
) -> Result<()> {
    ui::info("Pulling from GitHub...");
    let config = cfg::load(&config_path)?;

    let effective_repo = vcs::pull(&config, force, repo.as_deref())?;

    if repo.is_some() && config.github.repo_name.is_none() {
        cfg::set_config_value(&config_path, "github.repo_name", &effective_repo)?;
        ui::info(&format!(
            "Saved '{}' as default GitHub repository in config",
            effective_repo
        ));
    }

    // Ensure tracked_files match what we pulled so install/status work on new machines
    if let Ok(manifest) = repo::load_manifest() {
        let _ = repo::sync_tracked_files_from_manifest(&config_path, &manifest);
    }

    ui::success("Successfully pulled from GitHub!");

    if apply {
        let config = cfg::load(&config_path)?;
        if !config.general.backup {
            ui::warn(
                "general.backup is false — existing $HOME files may be overwritten without .bak copies",
            );
            if !force && !ui::prompt_confirm("Continue applying without backups?", false) {
                ui::info("Apply cancelled; pulled files remain in the compiled store");
                return Ok(());
            }
        }

        ui::info("Applying changes to system...");
        let compiled_path = dotdipper::paths::compiled_dir()?;

        let manifest = match repo::load_manifest() {
            Ok(m) => m,
            Err(e) => {
                anyhow::bail!(
                    "Could not load manifest after pull: {}. \
                     Pulled files are in the compiled store; fix the manifest, then run 'dotdipper apply'.",
                    e
                );
            }
        };

        let opts = repo::apply::ApplyOpts {
            force,
            allow_outside_home,
        };
        repo::apply::apply(&compiled_path, &manifest, &config, &opts)?;
        ui::success("Changes applied successfully!");
    } else {
        ui::hint("Use --apply to apply the pulled changes to your system");
        ui::hint("Tip: run 'dotdipper diff' first to preview what would change in $HOME");
    }

    Ok(())
}

async fn cmd_undo(config_path: PathBuf, force: bool, repo: Option<String>) -> Result<()> {
    ui::info("Undoing the last pushed commit...");
    let config = cfg::load(&config_path)?;

    let effective_repo = vcs::undo_last_push(&config, force, repo.as_deref())?;

    if repo.is_some() && config.github.repo_name.is_none() {
        cfg::set_config_value(&config_path, "github.repo_name", &effective_repo)?;
        ui::info(&format!(
            "Saved '{}' as default GitHub repository in config",
            effective_repo
        ));
    }

    ui::success("Successfully reverted the last pushed commit!");
    Ok(())
}

#[cfg(target_os = "macos")]
fn format_apps_capture_success(result: &apps::CaptureResult) -> String {
    let mut msg = format!(
        "Captured {} formulae, {} casks, {} MAS, {} unmanaged",
        result.formulae, result.casks, result.mas, result.unmanaged
    );
    let mut extras = Vec::new();
    if result.promoted_casks > 0 || result.promoted_mas > 0 {
        extras.push(format!(
            "promoted {} casks, {} MAS",
            result.promoted_casks, result.promoted_mas
        ));
    }
    if result.skipped_stock > 0 {
        extras.push(format!("skipped {} stock Apple apps", result.skipped_stock));
    }
    if !extras.is_empty() {
        msg.push_str(" (");
        msg.push_str(&extras.join("; "));
        msg.push(')');
    }
    msg
}

#[cfg(target_os = "macos")]
async fn cmd_apps(config_path: PathBuf, subcmd: AppsCommands) -> Result<()> {
    let config = cfg::load(&config_path)?;

    match subcmd {
        AppsCommands::Capture => {
            ui::info("Capturing installed applications...");
            let result = apps::capture(&config)?;
            ui::success(&format_apps_capture_success(&result));
            ui::info(&format!("Wrote {}", result.brewfile_path.display()));
            ui::info(&format!("Wrote {}", result.manifest_path.display()));
        }
        AppsCommands::List => {
            apps::list()?;
        }
        AppsCommands::Install { dry_run } => {
            if dry_run {
                apps::dry_run_install()?;
            } else {
                apps::install(&config)?;
            }
        }
    }

    Ok(())
}

async fn cmd_ignore(config_path: PathBuf, subcmd: IgnoreCommands) -> Result<()> {
    match subcmd {
        IgnoreCommands::Add { pattern } => {
            cfg::add_push_ignore(&config_path, &pattern)?;
            ui::success(&format!("Added push-ignore pattern: {}", pattern));
        }
        IgnoreCommands::Remove { pattern } => {
            cfg::remove_push_ignore(&config_path, &pattern)?;
            ui::success(&format!("Removed push-ignore pattern: {}", pattern));
        }
        IgnoreCommands::List => {
            let config = cfg::load(&config_path)?;
            let patterns = cfg::resolve_push_ignored_paths(&config)?;

            if patterns.is_empty() {
                ui::info("No push-ignore patterns configured");
            } else {
                ui::section("Effective push-ignore patterns:");
                for pattern in patterns {
                    println!("  {}", pattern);
                }
            }
        }
    }

    Ok(())
}

async fn cmd_install_script(config_path: PathBuf, out: Option<PathBuf>) -> Result<()> {
    let config = cfg::load(&config_path)?;
    let script = install::generate_dotfiles_script(&config)?;

    if let Some(output_path) = out {
        if let Some(parent) = output_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create directory {}", parent.display()))?;
            }
        }

        std::fs::write(&output_path, &script.content).with_context(|| {
            format!(
                "Failed to write install script to {}",
                output_path.display()
            )
        })?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&output_path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&output_path, perms)?;
        }

        ui::success(&format!(
            "Wrote install script to {}",
            output_path.display()
        ));
    } else {
        print!("{}", script.content);
        if !script.content.ends_with('\n') {
            println!();
        }
    }

    Ok(())
}

async fn cmd_install(
    config_path: PathBuf,
    dry_run: bool,
    target_os: Option<String>,
    allow_outside_home: bool,
) -> Result<()> {
    ui::info("Generating installation scripts...");
    let mut config = cfg::load(&config_path)?;

    let os = target_os.unwrap_or_else(install::detect_os);

    // If tracked_files are empty but compiled has content (fresh pull), sync from manifest
    if config.general.tracked_files.is_empty() {
        if let Ok(manifest) = repo::load_manifest() {
            let _ = repo::sync_tracked_files_from_manifest(&config_path, &manifest);
            config = cfg::load(&config_path)?;
        }
    }

    // Auto-discover packages if none are configured
    if config.packages.common.is_empty() {
        ui::info("No packages configured, discovering from dotfiles...");

        let discovery_config = install::DiscoveryConfig {
            target_os: os.clone(),
            include_low_confidence: false,
            custom_mappings: std::collections::HashMap::new(),
            exclude_patterns: config.exclude_patterns.clone(),
        };

        let result = install::discover::discover_packages(&config, &discovery_config)?;

        if result.has_packages() {
            ui::info(&format!(
                "Discovered {} packages from dotfiles",
                result.unique_packages().len()
            ));

            // Add discovered packages to the config for script generation
            let discovered_packages = result.unique_packages();
            config.packages.common.extend(discovered_packages);

            // Show what was discovered
            ui::section("Auto-discovered packages:");
            for (binary, package, _) in install::discover::get_package_display_list(&result)
                .iter()
                .take(10)
            {
                if binary == package {
                    println!("  {}", binary);
                } else {
                    println!("  {} -> {}", binary, package);
                }
            }
            if result.packages.len() > 10 {
                println!("  ... and {} more", result.packages.len() - 10);
            }
            println!();
        }
    }

    let scripts = install::generate_scripts(&config, &os)?;

    ui::success(&format!("Generated {} installation scripts", scripts.len()));

    // Show script locations
    for script in &scripts {
        ui::info(&format!("  {}: {}", script.name, script.path.display()));
    }

    if !dry_run {
        // Only run the OS package installer here. Dotfile placement is done by the
        // Rust apply path below so excludes/backups/manifest rules are honored
        // (running setup_dotfiles.sh would otherwise link every file under compiled/).
        let package_scripts: Vec<_> = scripts
            .iter()
            .filter(|s| s.name.starts_with("install_") && s.name != "install.sh")
            .cloned()
            .collect();

        if package_scripts.is_empty() {
            ui::warn("No OS package install script was generated");
        } else {
            ui::info("Installing packages...");
            install::run_scripts(&package_scripts)?;
        }

        // Apply dotfiles after installation using the safe Rust path
        ui::info("Applying dotfiles...");
        let compiled_path = dotdipper::paths::compiled_dir()?;

        match repo::load_manifest() {
            Ok(manifest) => {
                if !config.general.backup {
                    ui::warn(
                        "general.backup is false — existing $HOME files may be overwritten without .bak copies",
                    );
                }
                let opts = repo::apply::ApplyOpts {
                    force: false,
                    allow_outside_home,
                };
                repo::apply::apply(&compiled_path, &manifest, &config, &opts)?;
            }
            Err(e) => {
                anyhow::bail!(
                    "Package scripts ran, but dotfile apply failed (manifest unavailable): {}. \
                     Run 'dotdipper pull' first, then 'dotdipper apply'.",
                    e
                );
            }
        }

        ui::success("Installation completed successfully!");
        ui::hint("Standalone scripts (including setup_dotfiles.sh) remain in the install/ directory for manual use");
    } else {
        ui::hint("Remove --dry-run to execute the installation scripts");
    }

    Ok(())
}

async fn cmd_doctor(config_path: PathBuf, fix: bool) -> Result<()> {
    ui::info("Running diagnostics...");

    let secrets_provider = if config_path.exists() {
        cfg::load(&config_path)
            .ok()
            .and_then(|c| c.secrets)
            .and_then(|s| s.provider)
            .unwrap_or_else(|| "age".to_string())
    } else {
        "age".to_string()
    };

    let mut issues: Vec<(&str, Result<()>)> = vec![
        ("Git installed", vcs::check_git()),
        ("GitHub CLI installed", vcs::check_gh()),
        ("Age encryption tools installed", secrets::check_age()),
        ("Config file exists", cfg::check_exists(&config_path)),
        ("Manifest valid", repo::check_manifest(&config_path)),
    ];

    if secrets_provider.eq_ignore_ascii_case("sops") {
        issues.insert(
            3,
            ("SOPS installed (secrets provider)", secrets::check_sops()),
        );
    }

    let mut has_issues = false;
    let mut missing_tools = false;
    for (check, result) in issues {
        match result {
            Ok(_) => ui::success(&format!("✓ {}", check)),
            Err(e) => {
                has_issues = true;
                if check.contains("installed") {
                    missing_tools = true;
                }
                ui::error(&format!("✗ {}: {}", check, e));
                if fix {
                    ui::hint(
                        "  Automatic --fix is not implemented yet. Install missing tools manually.",
                    );
                }
            }
        }
    }

    if !has_issues {
        ui::success("All checks passed!");
    } else if missing_tools {
        ui::hint("Install missing tools:");
        ui::hint("  macOS: brew install age sops git gh");
        ui::hint(
            "  Linux: apt install age git gh; install sops from https://github.com/getsops/sops",
        );
    } else {
        ui::hint("Fix the failed checks above, then re-run 'dotdipper doctor'.");
    }

    Ok(())
}

async fn cmd_diff(config_path: PathBuf, detailed: bool) -> Result<()> {
    ui::info("Computing diff...");
    let config = cfg::load(&config_path)?;

    let compiled_path = dotdipper::paths::compiled_dir()?;

    let manifest = repo::load_manifest()
        .context("No manifest found. Run 'dotdipper pull' or 'dotdipper snapshot create' first.")?;
    let _entries = diff::diff(&compiled_path, &manifest, &config, detailed)?;

    Ok(())
}

async fn cmd_apply(
    config_path: PathBuf,
    force: bool,
    interactive: bool,
    only: Option<String>,
    allow_outside_home: bool,
) -> Result<()> {
    ui::info("Applying dotfiles...");
    let config = cfg::load(&config_path)?;

    let compiled_path = dotdipper::paths::compiled_dir()?;

    let manifest =
        repo::load_manifest().context("No manifest found. Run 'dotdipper pull' first.")?;

    if !config.general.backup {
        ui::warn(
            "general.backup is false — existing $HOME files may be overwritten without .bak copies",
        );
        if !force && !ui::prompt_confirm("Continue applying without backups?", false) {
            ui::info("Apply cancelled");
            return Ok(());
        }
    }

    // Get diff entries
    let mut entries = diff::diff(&compiled_path, &manifest, &config, false)?;

    // Filter by paths if --only specified
    if let Some(only_str) = only {
        let paths: Vec<String> = only_str.split(',').map(|s| s.trim().to_string()).collect();
        entries = diff::filter_by_paths(entries, &paths)?;
        ui::info(&format!("Filtered to {} matching files", entries.len()));
    }

    // Interactive selection if requested
    let selected_paths = if interactive {
        diff::interactive_select(&entries)?
    } else {
        // Apply all non-identical files
        entries
            .iter()
            .filter(|e| e.status != diff::DiffStatus::Identical)
            .map(|e| e.rel_path.clone())
            .collect()
    };

    if selected_paths.is_empty() {
        ui::info("No files selected for apply");
        return Ok(());
    }

    // Run pre-apply hooks
    if let Some(hooks) = &config.hooks {
        for hook in &hooks.pre_apply {
            ui::info(&format!("Running pre-apply hook: {}", hook));
            run_hook(hook)?;
        }
    }

    // Filter manifest to only selected paths
    let mut filtered_manifest = crate::hash::Manifest::new();
    for (path, hash) in &manifest.files {
        if selected_paths.contains(path) {
            filtered_manifest.add_file(hash.clone());
        }
    }

    let opts = repo::apply::ApplyOpts {
        force,
        allow_outside_home,
    };

    repo::apply::apply(&compiled_path, &filtered_manifest, &config, &opts)?;

    // Run post-apply hooks
    if let Some(hooks) = &config.hooks {
        for hook in &hooks.post_apply {
            ui::info(&format!("Running post-apply hook: {}", hook));
            run_hook(hook)?;
        }
    }

    ui::success("Apply completed successfully!");
    Ok(())
}

async fn cmd_secrets(config_path: PathBuf, subcmd: SecretsCommands) -> Result<()> {
    let config = cfg::load(&config_path)?;

    match subcmd {
        SecretsCommands::Init => {
            ui::info("Initializing secrets management...");
            secrets::init(&config)?;
        }
        SecretsCommands::Encrypt { path, output } => {
            let out = secrets::encrypt(&config, &path, output.as_deref())?;
            ui::success(&format!("Encrypted to {}", out.display()));
        }
        SecretsCommands::Decrypt { path, output } => {
            let out = secrets::decrypt(&config, &path, output.as_deref())?;
            ui::success(&format!("Decrypted to {}", out.display()));
        }
        SecretsCommands::Edit { path } => {
            secrets::edit(&config, &path)?;
        }
    }

    Ok(())
}

async fn cmd_snapshot(config_path: PathBuf, subcmd: SnapshotCommands) -> Result<()> {
    match subcmd {
        SnapshotCommands::Create { message, force } => {
            cmd_snapshot_create(config_path, force, message).await?;
        }
        SnapshotCommands::List => {
            let config = cfg::load(&config_path)?;
            let snaps = snapshots::list(&config)?;
            ui::info(&format!("Found {} snapshots", snaps.len()));
        }
        SnapshotCommands::Rollback { id, force } => {
            let config = cfg::load(&config_path)?;
            snapshots::rollback(&config, &id, force)?;
        }
        SnapshotCommands::Delete { id, force } => {
            let config = cfg::load(&config_path)?;
            snapshots::delete(&config, &id, force)?;
        }
        SnapshotCommands::Prune {
            keep_count,
            keep_age,
            keep_size,
            dry_run,
        } => {
            let config = cfg::load(&config_path)?;
            let opts = snapshots::PruneOpts {
                keep_count,
                keep_age,
                keep_size,
                dry_run,
            };
            snapshots::prune(&config, &opts)?;
        }
    }

    Ok(())
}

async fn cmd_profile(config_path: PathBuf, subcmd: ProfileCommands) -> Result<()> {
    let config = cfg::load(&config_path)?;

    match subcmd {
        ProfileCommands::List => {
            let profs = profiles::list(&config)?;
            ui::info(&format!("Found {} profiles", profs.len()));
        }
        ProfileCommands::Create { name } => {
            profiles::create(&config, &name)?;
        }
        ProfileCommands::Switch { name } => {
            profiles::switch(&config, &name)?;
        }
        ProfileCommands::Remove { name, force } => {
            profiles::remove(&config, &name, force)?;
        }
    }

    Ok(())
}

async fn cmd_remote(config_path: PathBuf, subcmd: RemoteCommands) -> Result<()> {
    let config = cfg::load(&config_path)?;

    match subcmd {
        RemoteCommands::Set {
            kind,
            endpoint,
            bucket,
            region,
            prefix,
        } => {
            let mut options = Vec::new();
            if let Some(e) = endpoint {
                options.push(("endpoint".to_string(), e));
            }
            if let Some(b) = bucket {
                options.push(("bucket".to_string(), b));
            }
            if let Some(r) = region {
                options.push(("region".to_string(), r));
            }
            if let Some(p) = prefix {
                options.push(("prefix".to_string(), p));
            }
            remote::set(&config, &kind, options)?;
        }
        RemoteCommands::Show => {
            remote::show(&config)?;
        }
        RemoteCommands::Push { dry_run } => {
            remote::push(&config, dry_run).await?;
        }
        RemoteCommands::Pull => {
            remote::pull(&config).await?;
        }
    }

    Ok(())
}

async fn cmd_daemon(config_path: PathBuf, subcmd: DaemonCommands) -> Result<()> {
    match subcmd {
        DaemonCommands::Start => {
            let config = cfg::load(&config_path)?;
            daemon::start(&config)?;
        }
        DaemonCommands::Stop => {
            let config = cfg::load(&config_path)?;
            daemon::stop(&config)?;
        }
        DaemonCommands::Status => {
            let config = cfg::load(&config_path)?;
            daemon::status(&config)?;
        }
        DaemonCommands::Enable => {
            daemon::enable(&config_path)?;
        }
        DaemonCommands::Disable => {
            daemon::disable(&config_path)?;
        }
    }

    Ok(())
}

async fn cmd_config(
    config_path: PathBuf,
    edit: bool,
    show: bool,
    set: Option<String>,
) -> Result<()> {
    if let Some(kv) = set {
        let (key, value) = kv
            .split_once('=')
            .context("Invalid format. Use key=value (e.g. github.repo_name=dotfiles-dotdipper)")?;
        cfg::set_config_value(&config_path, key.trim(), value.trim())?;
        ui::success(&format!("Set {} = {}", key.trim(), value.trim()));
    } else if edit {
        cfg::edit(&config_path)?;
        ui::success("Configuration edited");
    } else if show {
        let config = cfg::load(&config_path)?;
        println!("{}", toml::to_string_pretty(&config)?);
        if let Ok(profile) = profiles::resolve_active_profile_name() {
            if let Ok(overlay) = cfg::overlay_path_for(&profile) {
                if overlay.exists() {
                    ui::hint(&format!("merged with overlay {}", overlay.display()));
                }
            }
        }
    } else {
        ui::hint("Use --edit to modify, --show to view, or --set key=value to set a value");
    }

    Ok(())
}

fn run_hook(hook: &str) -> Result<()> {
    use std::process::Command;

    let status = Command::new("sh")
        .arg("-c")
        .arg(hook)
        .status()
        .with_context(|| format!("Failed to run hook: {}", hook))?;

    if !status.success() {
        anyhow::bail!("Hook failed with exit code: {:?}", status.code());
    }

    Ok(())
}

#[cfg(target_os = "macos")]
use dotdipper::apps;
use dotdipper::cfg;
use dotdipper::daemon;
use dotdipper::diff;
use dotdipper::hash;
use dotdipper::install;
use dotdipper::profiles;
use dotdipper::publish;
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
use std::path::{Path, PathBuf};

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
    /// Initialize dotdipper, optionally from an existing dotfiles repository
    ///
    /// With a source, clones it, shows what it would write into $HOME, and
    /// with --apply writes it after you confirm.
    Init {
        /// Repository to set up from: owner, owner/repo, host/owner/repo,
        /// or an https:// or git@ URL. Anonymous HTTPS unless the URL says SSH.
        source: Option<String>,

        /// Force initialization even if config exists
        #[arg(short, long)]
        force: bool,

        /// Apply the repository's dotfiles to $HOME after the preview
        #[arg(long, requires = "source")]
        apply: bool,

        /// Branch to follow (default: the repository's default branch)
        #[arg(long, requires = "source")]
        branch: Option<String>,

        /// Use this exact commit (or tag) instead of the branch tip, so what
        /// is applied is what you reviewed
        #[arg(long = "ref", value_name = "COMMIT", requires = "source")]
        pin: Option<String>,

        /// Skip the confirmation before applying (for unattended runs)
        #[arg(short = 'y', long, requires = "apply")]
        yes: bool,

        /// After applying, install the repository's Homebrew packages and
        /// App Store titles with `install apps` (macOS only)
        #[arg(long, requires = "apply")]
        install: bool,

        /// Also apply the repository's own dotdipper settings
        /// (~/.config/dotdipper), including any hooks they define. Use this
        /// only for a repository you control, such as your own.
        #[arg(long, requires = "source")]
        adopt_config: bool,
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

    /// Show status of tracked files; lists changed paths by default
    Status {
        /// Same as default; kept for compatibility
        #[arg(long, hide = true)]
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

    /// Build and push a sanitized public mirror of the dotfiles
    Publish {
        /// Commit message
        #[arg(short, long)]
        message: Option<String>,

        /// Show what would be published, redacted and withheld; write nothing
        #[arg(long)]
        dry_run: bool,

        /// Write the sanitized tree to a directory instead of pushing
        #[arg(long)]
        out: Option<PathBuf>,

        /// Override the public repository name
        #[arg(long)]
        repo: Option<String>,

        /// Regenerate the allowlist from the current store and stop. Review
        /// the file it writes, then publish.
        #[arg(long)]
        review: bool,

        /// Accept one reviewed scanner finding by id (repeatable)
        #[arg(long = "allow-finding")]
        allow_finding: Vec<String>,

        /// Force push the public mirror
        #[arg(short, long)]
        force: bool,
    },

    /// Pull dotfiles from GitHub
    Pull {
        /// Apply pulled changes to system (creates backups when general.backup = true)
        #[arg(long)]
        apply: bool,

        /// Discard uncommitted compiled/ git changes. Symlink-restored $HOME files follow compiled/ immediately.
        #[arg(short, long)]
        force: bool,

        /// Allow operations outside $HOME (unsafe)
        #[arg(long)]
        unsafe_allow_outside_home: bool,

        /// Override the GitHub repository name
        #[arg(long)]
        repo: Option<String>,

        /// Clone over anonymous HTTPS instead of SSH. Use this to consume a
        /// public mirror on a machine with no GitHub sign-in and no SSH key.
        #[arg(long)]
        https: bool,
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

        /// With --edit, open the active profile overlay instead of the base config
        #[arg(long)]
        overlay: bool,

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

    /// Capture macOS system preferences as a replayable script
    #[cfg(target_os = "macos")]
    #[command(subcommand)]
    Macos(MacosCommands),
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

    /// Print or export a script that installs the captured tools, derived
    /// from the Brewfile and apps manifest without their machine details
    AppsScript {
        /// Write the script to a file instead of printing to stdout
        #[arg(short = 'o', long = "out", value_name = "PATH")]
        out: Option<PathBuf>,

        /// Drop taps owned by the configured GitHub user, as the public
        /// mirror does. Off by default, since a personal tap is wanted on
        /// your own machines.
        #[arg(long)]
        shareable: bool,
    },

    /// Install the captured Homebrew packages and App Store titles (macOS).
    ///
    /// Looks every entry up first, shows one filterable checklist with what
    /// each item is and what installing it involves, asks for confirmation
    /// and at most one password, then installs unattended and reports.
    Apps {
        /// No picker: install the default selection (everything except
        /// third-party taps and deprecated, disabled or unknown packages)
        #[arg(long, conflicts_with = "only")]
        all: bool,

        /// Install exactly these items (comma-separated names or App Store ids)
        #[arg(long, value_delimiter = ',', value_name = "NAMES")]
        only: Vec<String>,

        /// Leave these items out for this run (comma-separated)
        #[arg(long, value_delimiter = ',', value_name = "NAMES")]
        skip: Vec<String>,

        /// Resolve and print the plan; install nothing
        #[arg(long)]
        dry_run: bool,

        /// Skip the confirmation; without a terminal, use the default selection
        #[arg(short = 'y', long)]
        yes: bool,

        /// Offer items deselected on an earlier run again
        #[arg(long)]
        reselect: bool,
    },
}

#[cfg(target_os = "macos")]
#[derive(Subcommand)]
enum MacosCommands {
    /// Capture macOS preferences into a replayable defaults script
    Capture {
        /// Write the script somewhere other than ~/.config/macos/defaults.sh
        #[arg(short = 'o', long = "out", value_name = "PATH")]
        out: Option<PathBuf>,
    },

    /// Show which preference keys are eligible for capture
    Keys,
}

#[cfg(target_os = "macos")]
#[derive(Subcommand)]
enum AppsCommands {
    /// Capture Homebrew, Mac App Store, and /Applications state
    Capture,

    /// Show the captured apps manifest
    List,

    /// Install the captured Homebrew packages and App Store titles (same as `install apps`)
    Install {
        /// Print what the Brewfile lists without installing anything
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
        Commands::Init {
            source,
            force,
            apply,
            branch,
            pin,
            yes,
            install,
            adopt_config,
        } => match source {
            Some(source) => {
                cmd_init_from(
                    config_path,
                    InitFrom {
                        source,
                        force,
                        apply,
                        branch,
                        pin,
                        yes,
                        install,
                        adopt_config,
                    },
                )
                .await
            }
            None => cmd_init(config_path, force).await,
        },
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
        Commands::Status { detailed: _ } => cmd_status(config_path).await,
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
        Commands::Publish {
            message,
            dry_run,
            out,
            repo,
            review,
            allow_finding,
            force,
        } => {
            cmd_publish(
                config_path,
                PublishOptions {
                    message,
                    dry_run,
                    out,
                    repo,
                    review,
                    allow_finding,
                    force,
                },
            )
            .await
        }
        Commands::Pull {
            apply,
            force,
            unsafe_allow_outside_home,
            repo,
            https,
        } => {
            cmd_pull(
                config_path,
                apply,
                force,
                unsafe_allow_outside_home,
                repo,
                https,
            )
            .await
        }
        Commands::Undo { force, repo } => cmd_undo(config_path, force, repo).await,
        Commands::Install {
            action,
            dry_run,
            target_os,
            unsafe_allow_outside_home,
        } => match action {
            Some(InstallCommands::Script { out }) => cmd_install_script(config_path, out).await,
            Some(InstallCommands::AppsScript { out, shareable }) => {
                cmd_install_apps_script(config_path, out, shareable).await
            }
            Some(InstallCommands::Apps {
                all,
                only,
                skip,
                dry_run,
                yes,
                reselect,
            }) => cmd_install_apps(install::apps_install::Options {
                all,
                only,
                skip,
                dry_run,
                yes,
                reselect,
            }),
            None => cmd_install(config_path, dry_run, target_os, unsafe_allow_outside_home).await,
        },
        Commands::Doctor { fix } => cmd_doctor(config_path, fix).await,
        Commands::Config {
            edit,
            overlay,
            show,
            set,
        } => cmd_config(config_path, edit, overlay, show, set).await,
        Commands::Ignore(subcmd) => cmd_ignore(config_path, subcmd).await,
        #[cfg(target_os = "macos")]
        Commands::Apps(subcmd) => cmd_apps(config_path, subcmd).await,
        #[cfg(target_os = "macos")]
        Commands::Macos(subcmd) => cmd_macos(subcmd).await,
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

struct InitFrom {
    source: String,
    force: bool,
    apply: bool,
    branch: Option<String>,
    pin: Option<String>,
    yes: bool,
    install: bool,
    adopt_config: bool,
}

/// `dotdipper init <source>`: one command from an installed binary to a
/// home directory set up from someone's repository.
///
/// The repository may not be the user's own, so nothing reaches `$HOME`
/// until the user has seen exactly what will be written and agreed to it,
/// and nothing in the repository is ever executed.
async fn cmd_init_from(config_path: PathBuf, opts: InitFrom) -> Result<()> {
    let source = dotdipper::source::parse(&opts.source)?;
    ui::info(&format!("Setting up from {}", source.display()));

    let compiled = dotdipper::paths::compiled_dir();
    if let Ok(compiled) = &compiled {
        if compiled.join(".git").exists() {
            anyhow::bail!(
                "A dotfiles store already exists at {}. Run 'dotdipper pull' to update it, \
                 or move it aside to set up from a different repository.",
                compiled.display()
            );
        }
    }

    if config_path.exists() && !opts.force {
        ui::info(&format!(
            "Keeping existing config at {}",
            config_path.display()
        ));
    } else {
        cfg::init(config_path.clone(), opts.force)?;
        // Copies, not symlinks: with links, every later pull of a repository
        // someone else controls would change live files with no apply step.
        cfg::set_config_value(&config_path, "general.default_mode", "copy")?;
    }

    let branch = match opts.branch {
        Some(b) => b,
        None => vcs::remote_default_branch(&source.url)?.unwrap_or_else(|| "main".to_string()),
    };
    cfg::set_config_value(&config_path, "github.url", &source.url)?;
    cfg::set_config_value(&config_path, "github.branch", &branch)?;

    let config = cfg::load(&config_path)?;
    vcs::pull(&config, false, None, vcs::Transport::HttpsAnonymous)?;
    let compiled = dotdipper::paths::compiled_dir()?;

    if let Some(rev) = &opts.pin {
        let sha = vcs::pin_to_commit(&compiled, rev)?;
        repo::sync_manifest_from_compiled()?;
        ui::info(&format!("Pinned to commit {sha}"));
    }

    let layout = repo::store_layout(&compiled);
    ui::info(&format!("Detected a {}", layout.describe()));
    if !layout.is_applicable() {
        explain_unsupported_layout(layout, &compiled);
        if opts.apply {
            anyhow::bail!(
                "Nothing was applied: this repository does not map onto $HOME by position"
            );
        }
        return Ok(());
    }

    let manifest = repo::load_manifest()?;
    let _ = repo::sync_tracked_files_from_manifest(&config_path, &manifest);

    // The repository's dotdipper settings would replace the config just
    // written, and their hooks would run on the next apply: the one way a
    // repository could make dotdipper execute its code. They are held back
    // unless the user says the repository is theirs.
    let settings = dotdipper_settings_in(&manifest)?;
    let mut held_back: Vec<PathBuf> = Vec::new();
    if !settings.is_empty() {
        let hooks = hooks_defined_in(&compiled, &settings);
        if opts.adopt_config {
            ui::warn("Adopting this repository's dotdipper settings (--adopt-config).");
            if !hooks.is_empty() {
                ui::warn("These hooks will run on every later 'dotdipper apply':");
                for (when, cmd) in &hooks {
                    println!("  {when}: {cmd}");
                }
            }
        } else {
            for rel in &settings {
                cfg::exclude_from_apply(&config_path, &format!("~/{}", rel.display()))?;
            }
            held_back = settings.clone();
            ui::warn(&format!(
                "Not applying this repository's dotdipper settings ({} file(s)); they can define hooks, which run commands.",
                settings.len()
            ));
            for (when, cmd) in &hooks {
                println!("  {when}: {cmd}");
            }
            ui::hint(
                "If this repository is yours, re-run on a clean machine with --adopt-config to use its settings and hooks.",
            );
        }
    }
    let preview = preview_apply(&compiled, &manifest, &held_back)?;

    if !opts.apply {
        ui::success("Repository cloned; $HOME is unchanged.");
        ui::hint("Review the list above, then run 'dotdipper apply' (or 'dotdipper apply --interactive').");
        return Ok(());
    }
    let install_apps = opts.install;
    let yes = opts.yes;
    if preview.changes == 0 {
        ui::success("Every file already matches; nothing to apply.");
        return install_apps_after_apply(install_apps, yes, &compiled);
    }

    if !opts.yes {
        if !console::Term::stderr().is_term() {
            anyhow::bail!(
                "No terminal to confirm on. Review the list above, then re-run with --yes to apply."
            );
        }
        let prompt = if preview.flagged > 0 {
            format!(
                "Apply {} file(s), including {} flagged above?",
                preview.changes, preview.flagged
            )
        } else {
            format!("Apply {} file(s) to $HOME?", preview.changes)
        };
        // Default to no when anything flagged is about to land.
        if !ui::prompt_confirm(&prompt, preview.flagged == 0) {
            ui::info(
                "Nothing applied. The repository stays in the store; run 'dotdipper apply' later.",
            );
            return Ok(());
        }
    }

    let config = cfg::load(&config_path)?;
    let opts = repo::apply::ApplyOpts {
        force: true,
        allow_outside_home: false,
    };
    repo::apply::apply(&compiled, &manifest, &config, &opts)?;
    ui::success("Dotfiles applied. Files that already existed were backed up with a .bak suffix.");
    install_apps_after_apply(install_apps, yes, &compiled)?;
    ui::hint("Open a new terminal so shell configuration takes effect.");
    Ok(())
}

/// `init --install`: runs `install apps` once the dotfiles are in place.
///
/// Only on macOS and only when the store has something to install; a
/// repository without a Brewfile or manifest is not an error here, since
/// `--install` may be in a bootstrap command shared across repositories.
fn install_apps_after_apply(requested: bool, yes: bool, compiled: &Path) -> Result<()> {
    if !requested {
        return Ok(());
    }
    if !cfg!(target_os = "macos") {
        ui::info("Skipping --install: installing apps is macOS-only.");
        return Ok(());
    }
    if !compiled.join("Brewfile").exists() && !compiled.join("apps_manifest.toml").exists() {
        ui::info("Skipping --install: the repository has no Brewfile or apps_manifest.toml.");
        return Ok(());
    }
    ui::section("Apps");
    install::apps_install::run(
        compiled,
        &install::apps_install::Options {
            yes,
            ..Default::default()
        },
    )
}

fn explain_unsupported_layout(layout: repo::layout::Layout, compiled: &Path) {
    use repo::layout::Layout;
    match layout {
        Layout::Stow => ui::warn(
            "Stow packages (zsh/.zshrc) do not map onto $HOME by position, so nothing will be applied.",
        ),
        Layout::Chezmoi => ui::warn(
            "chezmoi sources (dot_zshrc) need chezmoi to render them; nothing will be applied. \
             Use 'chezmoi init --apply' for this repository.",
        ),
        _ => ui::warn("No top-level dotfiles were found to apply."),
    }
    if let Some(script) = repo::layout::install_script(compiled) {
        ui::warn(&format!(
            "The repository ships '{script}'. dotdipper never runs it; read it before running it yourself."
        ));
    }
    ui::hint(&format!("The files are at {}", compiled.display()));
}

/// Manifest paths that land in dotdipper's own directory: the conventional
/// `~/.config/dotdipper`, and `DOTDIPPER_HOME` when it lives under `$HOME`.
fn dotdipper_settings_in(manifest: &hash::Manifest) -> Result<Vec<PathBuf>> {
    let home = dirs::home_dir().context("Failed to find home directory")?;
    let mut prefixes = vec![PathBuf::from(".config/dotdipper")];
    if let Ok(base) = dotdipper::paths::base_dir() {
        if let Ok(rel) = base.strip_prefix(&home) {
            prefixes.push(rel.to_path_buf());
        }
    }
    Ok(manifest
        .files
        .keys()
        .filter(|rel| prefixes.iter().any(|p| rel.starts_with(p)))
        .cloned()
        .collect())
}

/// `(hook kind, command)` for every hook the repository's settings define.
/// Unreadable or malformed files define none as far as this report goes;
/// they are held back either way.
fn hooks_defined_in(compiled: &Path, settings: &[PathBuf]) -> Vec<(String, String)> {
    let mut hooks = Vec::new();
    for rel in settings.iter().filter(|r| r.ends_with("config.toml")) {
        let Ok(text) = std::fs::read_to_string(compiled.join(rel)) else {
            continue;
        };
        let Ok(value) = text.parse::<toml::Value>() else {
            continue;
        };
        let Some(table) = value.get("hooks").and_then(|h| h.as_table()) else {
            continue;
        };
        for (when, cmds) in table {
            for cmd in cmds.as_array().into_iter().flatten() {
                if let Some(cmd) = cmd.as_str() {
                    hooks.push((when.clone(), cmd.to_string()));
                }
            }
        }
    }
    hooks
}

struct ApplyPreview {
    /// Files that would be created or replaced.
    changes: usize,
    /// Of those, files that run code or grant access.
    flagged: usize,
}

/// Lists what applying `manifest` would do to `$HOME`, flagged paths first,
/// without touching anything.
fn preview_apply(
    compiled: &Path,
    manifest: &hash::Manifest,
    held_back: &[PathBuf],
) -> Result<ApplyPreview> {
    const SHOWN: usize = 40;
    let home = dirs::home_dir().context("Failed to find home directory")?;

    let mut rows: Vec<(bool, Option<&'static str>, &Path)> = Vec::new();
    let mut identical = 0usize;
    for rel in manifest.files.keys() {
        if held_back.contains(rel) {
            continue;
        }
        let target = home.join(rel);
        let source = compiled.join(rel);
        let same = match (std::fs::read(&target), std::fs::read(&source)) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        };
        if same {
            identical += 1;
            continue;
        }
        rows.push((target.exists(), repo::layout::risk(rel), rel.as_path()));
    }
    // Flagged first, then replacements, then new files.
    rows.sort_by_key(|(exists, risk, path)| (risk.is_none(), !exists, path.to_path_buf()));

    let flagged = rows.iter().filter(|(_, risk, _)| risk.is_some()).count();
    let replaced = rows.iter().filter(|(exists, _, _)| *exists).count();
    ui::section("What this repository would write into $HOME:");
    println!(
        "  {} new, {} replaced (backed up first), {} already identical",
        rows.len() - replaced,
        replaced,
        identical
    );
    for (exists, risk, path) in rows.iter().take(SHOWN) {
        let mark = if *exists { "~".yellow() } else { "+".green() };
        match risk {
            Some(why) => println!(
                "  {} ~/{}  {}",
                mark,
                path.display(),
                format!("[{why}]").red()
            ),
            None => println!("  {} ~/{}", mark, path.display()),
        }
    }
    if rows.len() > SHOWN {
        println!("  ... and {} more", rows.len() - SHOWN);
    }
    if flagged > 0 {
        ui::warn(&format!(
            "{flagged} file(s) can run code or grant access to this machine. Read them before applying a repository you do not control."
        ));
    }
    Ok(ApplyPreview {
        changes: rows.len(),
        flagged,
    })
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

async fn cmd_status(config_path: PathBuf) -> Result<()> {
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

        status.print_detailed();
    }

    Ok(())
}

/// Builds the sanitized public mirror and, unless asked to stop early,
/// pushes it to its own public repository.
///
/// The scan is not advisory. If it reports anything, this returns an error and
/// nothing is written or pushed, so a gap in the redaction rules costs a failed
/// command rather than a disclosure.
/// Flags for `dotdipper publish`, grouped so the handler keeps one argument.
struct PublishOptions {
    message: Option<String>,
    dry_run: bool,
    out: Option<PathBuf>,
    repo: Option<String>,
    review: bool,
    allow_finding: Vec<String>,
    force: bool,
}

async fn cmd_publish(config_path: PathBuf, opts: PublishOptions) -> Result<()> {
    let PublishOptions {
        message,
        dry_run,
        out,
        repo,
        review,
        allow_finding,
        force,
    } = opts;
    let mut config = cfg::load(&config_path)?;

    // CLI allowances are additive to the reviewed ones in config.
    if !allow_finding.is_empty() {
        let mut public = config.public.clone().unwrap_or_default();
        public.allow.extend(allow_finding);
        config.public = Some(public);
    }

    let source = dotdipper::paths::compiled_dir()?;
    if !source.exists() {
        anyhow::bail!(
            "No compiled store at {}. Run 'dotdipper push' or 'dotdipper snapshot create' first.",
            source.display()
        );
    }

    // Resolve the target before doing any work, so a misconfiguration fails
    // before we spend time building a tree that cannot be pushed.
    if !dry_run && !review && out.is_none() {
        vcs::resolve_public_repo(&config, repo.as_deref())?;
    }

    let list_path = publish::allowlist_path(&config)?;
    let allowlist = publish::Allowlist::load(&list_path)?;

    if allowlist.is_none() && !review {
        anyhow::bail!(
            "No allowlist at {}. Nothing is published until you have reviewed what would be.\n               Run 'dotdipper publish --review' to generate it, read it, then publish.",
            list_path.display()
        );
    }

    // --review derives the full picture, so it must not be filtered by the
    // allowlist it is about to rewrite. The identity terms recorded in it
    // are a different matter: they are an input, and they must survive a
    // review run or reviewing on a second machine would discard the terms
    // belonging to the machine the store was captured on.
    let effective = if review { None } else { allowlist.as_ref() };
    let recorded_terms: Vec<String> = allowlist
        .as_ref()
        .map(|l| l.identity_terms.clone())
        .unwrap_or_default();

    let dest = if dry_run || review {
        None
    } else {
        Some(match &out {
            Some(dir) => dir.clone(),
            None => publish::public_dir()?,
        })
    };

    ui::info("Building sanitized public mirror...");
    let plan = publish::build(
        &source,
        dest.as_deref(),
        &config,
        effective,
        &recorded_terms,
    )?;
    publish::report(&plan);

    if !plan.is_clean() {
        publish::report_findings(&plan);
        anyhow::bail!(
            "Publish aborted: {} unresolved finding(s). Nothing was written or pushed.",
            plan.findings.len()
        );
    }

    ui::success("Scan clean");

    if review {
        let generated = plan.to_allowlist();
        let count = generated.files.len();
        generated.save(&list_path)?;
        ui::success(&format!(
            "Allowlist written to {} ({} file(s) approved for publication)",
            list_path.display(),
            count
        ));
        ui::hint(
            "Read it, remove anything that should stay private, then run 'dotdipper publish'.",
        );
        return Ok(());
    }

    if dry_run {
        ui::info("Dry run: nothing written or pushed");
        return Ok(());
    }

    let dest = dest.expect("dest is set when not a dry run");

    if out.is_some() {
        ui::success(&format!("Public tree written to {}", dest.display()));
        return Ok(());
    }

    let repo_name = vcs::resolve_public_repo(&config, repo.as_deref())?.1;
    let pushed = vcs::publish_push(&config, &dest, &repo_name, message, force)?;
    ui::success(&format!("Published to {}", pushed));

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
    https: bool,
) -> Result<()> {
    ui::info("Pulling from GitHub...");
    let config = cfg::load(&config_path)?;

    let transport = if https {
        vcs::Transport::HttpsAnonymous
    } else {
        vcs::Transport::Ssh
    };
    let effective_repo = vcs::pull(&config, force, repo.as_deref(), transport)?;

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

#[cfg(target_os = "macos")]
async fn cmd_macos(subcmd: MacosCommands) -> Result<()> {
    use dotdipper::macos;

    match subcmd {
        MacosCommands::Keys => {
            ui::info(&format!(
                "{} preference key(s) are eligible for capture:",
                macos::ALLOWLIST.len()
            ));
            let mut domain = "";
            for setting in macos::ALLOWLIST {
                if setting.domain != domain {
                    println!("\n  {}", setting.domain);
                    domain = setting.domain;
                }
                println!("    {}", setting.key);
            }
            ui::hint(
                "Only these keys are read, and only scalar values. No preference plist is \
                 ever copied: Finder and Dock plists carry recent folders, home paths, and \
                 the pinned app lineup.",
            );
        }
        MacosCommands::Capture { out } => {
            ui::info("Capturing macOS preferences...");
            let result = macos::capture(out.as_deref())?;

            ui::success(&format!(
                "Captured {} setting(s) to {}",
                result.captured,
                result.path.display()
            ));
            if result.skipped_unset > 0 {
                ui::hint(&format!(
                    "{} allowlisted key(s) are not set on this machine and were skipped",
                    result.skipped_unset
                ));
            }
            // A key that is set but not recorded is the interesting case: it
            // means a value was refused, not that the machine lacks it.
            for refused in &result.refused {
                ui::warn(&format!(
                    "{} is set but was not recorded (not a scalar, or the value looks like a path)",
                    refused
                ));
            }
        }
    }

    Ok(())
}

/// `dotdipper install apps`. The flow itself lives in the library so that
/// `apps install` and `init --install` run exactly the same code.
fn cmd_install_apps(opts: install::apps_install::Options) -> Result<()> {
    let store = dotdipper::paths::compiled_dir()?;
    install::apps_install::run(&store, &opts)
}

async fn cmd_install_apps_script(
    config_path: PathBuf,
    out: Option<PathBuf>,
    shareable: bool,
) -> Result<()> {
    let config = cfg::load(&config_path)?;
    let store = dotdipper::paths::compiled_dir()?;

    let brewfile = std::fs::read_to_string(store.join("Brewfile")).ok();
    let manifest_text = std::fs::read_to_string(store.join("apps_manifest.toml")).ok();
    let apps = match &manifest_text {
        Some(text) => Some(dotdipper::install::apps_script::AppsInventory::parse(text)?),
        None => None,
    };

    if brewfile.is_none() && apps.is_none() {
        anyhow::bail!(
            "No Brewfile or apps_manifest.toml in {}. Run 'dotdipper apps capture' first.",
            store.display()
        );
    }

    let omit = if shareable {
        config.github.username.clone().into_iter().collect()
    } else {
        Vec::new()
    };

    let script = dotdipper::install::apps_script::generate(
        &dotdipper::install::apps_script::Inventory {
            brewfile: brewfile.as_deref(),
            apps: apps.as_ref(),
        },
        &omit,
    );

    if let Some(output_path) = out {
        if let Some(parent) = output_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create directory {}", parent.display()))?;
            }
        }
        std::fs::write(&output_path, &script)
            .with_context(|| format!("Failed to write {}", output_path.display()))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&output_path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&output_path, perms)?;
        }

        ui::success(&format!("Wrote apps script to {}", output_path.display()));
    } else {
        print!("{}", script);
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
        (
            "Profile overlay does not shadow the config",
            check_no_shadowing(&config_path),
        ),
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
    overlay: bool,
    show: bool,
    set: Option<String>,
) -> Result<()> {
    if let Some(kv) = set {
        let (key, value) = kv
            .split_once('=')
            .context("Invalid format. Use key=value (e.g. github.repo_name=dotfiles-dotdipper)")?;
        let written = cfg::set_config_value(&config_path, key.trim(), value.trim())?;
        ui::success(&format!("Set {} = {}", key.trim(), value.trim()));
        if written != config_path {
            ui::hint(&format!(
                "wrote {} — the active profile overlay already defines this key, \
                 so a write to the base config would have been overridden",
                written.display()
            ));
        }
    } else if edit {
        let target = if overlay {
            cfg::active_overlay_path()?
        } else {
            report_shadowed_keys(&config_path)?;
            config_path.clone()
        };
        cfg::edit(&target)?;
        ui::success(&format!("Edited {}", target.display()));
    } else if show {
        let config = cfg::load(&config_path)?;
        println!("{}", toml::to_string_pretty(&config)?);
        if let Ok(path) = cfg::active_overlay_path() {
            if path.exists() {
                ui::hint(&format!("merged with overlay {}", path.display()));
            }
        }
        report_shadowed_keys(&config_path)?;
    } else {
        ui::hint(
            "Use --edit to modify, --edit --overlay to modify the active profile overlay, \
             --show to view, or --set key=value to set a value",
        );
    }

    Ok(())
}

/// Doctor check: the base config and the active overlay must not disagree.
/// A disagreement means one of the two files is dead weight that still reads
/// as authoritative.
fn check_no_shadowing(config_path: &Path) -> Result<()> {
    let shadowed = cfg::shadowed_keys(config_path)?;
    if shadowed.is_empty() {
        return Ok(());
    }
    let keys: Vec<&str> = shadowed.iter().map(|k| k.key.as_str()).collect();
    let overlay_path = cfg::active_overlay_path()?;
    anyhow::bail!(
        "{} is overridden by {} for: {}. Edits to the base config for those keys do nothing",
        config_path.display(),
        overlay_path.display(),
        keys.join(", ")
    )
}

/// Warn about base-config keys the active profile overlay overrides. Editing
/// those in the base file has no effect, and the silence is the whole trap.
fn report_shadowed_keys(config_path: &Path) -> Result<()> {
    let shadowed = cfg::shadowed_keys(config_path)?;
    if shadowed.is_empty() {
        return Ok(());
    }
    let overlay_path = cfg::active_overlay_path()?;
    ui::warn(&format!(
        "{} key(s) in {} are overridden by the profile overlay and have no effect:",
        shadowed.len(),
        config_path.display()
    ));
    for key in &shadowed {
        println!(
            "    {} — base: {}, overlay: {}",
            key.key, key.base, key.overlay
        );
    }
    ui::hint(&format!(
        "the overlay wins on load. Edit {} instead (dotdipper config --edit --overlay)",
        overlay_path.display()
    ));
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

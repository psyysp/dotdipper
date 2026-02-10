# Dotdipper

> A safe, deterministic, and feature-rich dotfiles manager built in Rust with encryption, selective apply, snapshots, profiles, and cloud sync.

[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

---

## 🎯 What is Dotdipper?

Dotdipper is a comprehensive dotfiles manager that helps you synchronize, manage, and deploy your configuration files across multiple machines. Built with safety and determinism as core principles, it provides powerful features for managing dotfiles at scale.

### Key Features

- 🔐 **Secrets Encryption** - Age encryption for sensitive files with in-memory decryption
- 🎯 **Selective Apply** - Interactive TUI to choose which files to apply
- 📸 **Snapshot Management** - Create, list, and rollback to previous snapshots
- 👤 **Multiple Profiles** - Separate configs for work, personal, servers, etc.
- ☁️ **Cloud Backups** - Push/pull to LocalFS, S3, or WebDAV remotes
- 🤖 **Auto-Sync Daemon** - Watch files and auto-snapshot on changes
- 🪝 **Hooks System** - Automate workflows with pre/post hooks
- 🔄 **GitHub Sync** - Push/pull dotfiles to/from GitHub
- 📦 **Package Management** - Auto-discover and install system packages from dotfiles
- 🔍 **Smart Diff** - Git-style diffs before applying changes
- 🛡️ **Safety First** - Backups, confirmations, and HOME boundary enforcement

---

## 🚀 Quick Start

### Installation

#### macOS (Homebrew) - Recommended

```bash
brew tap psyysp/dotdipper
brew install dotdipper
```

This will also install `age` (required for secrets encryption) as a dependency.

#### Arch Linux (AUR)

```bash
# Using yay
yay -S dotdipper

# Or using paru
paru -S dotdipper

# Binary version (faster install)
yay -S dotdipper-bin
```

#### Nix / NixOS

The repo provides a flake at the repo root. The Nix package wraps the binary so `age` is on `PATH` (secrets encryption works without a separate `age` install).

```bash
# Install into your user profile (recommended)
nix profile install github:psyysp/dotdipper

# Or from a local clone
git clone https://github.com/psyysp/dotdipper && cd dotdipper
nix profile install .#dotdipper
```

**NixOS (flake-based config):** add to your flake inputs `dotdipper.url = "github:psyysp/dotdipper";`, then in `environment.systemPackages` (or Home Manager `home.packages`) add `inputs.dotdipper.packages.${pkgs.system}.default`.

**Development shell** (Rust + pkg-config, openssl, age):

```bash
nix develop
```

#### Cargo (Rust)

```bash
# Install from crates.io
cargo install dotdipper

# Or from source
cargo install --git https://github.com/psyysp/dotdipper
```

#### Windows (Scoop)

```powershell
# Add the bucket and install
scoop bucket add dotdipper https://github.com/psyysp/scoop-dotdipper
scoop install dotdipper

# Also install age for secrets
scoop install age
```

#### Manual Binary Download

```bash
# macOS Apple Silicon (M1/M2/M3)
curl -LO https://github.com/psyysp/dotdipper/releases/latest/download/dotdipper-aarch64-apple-darwin.tar.gz
tar -xzf dotdipper-aarch64-apple-darwin.tar.gz
sudo mv dotdipper /usr/local/bin/

# macOS Intel
curl -LO https://github.com/psyysp/dotdipper/releases/latest/download/dotdipper-x86_64-apple-darwin.tar.gz
tar -xzf dotdipper-x86_64-apple-darwin.tar.gz
sudo mv dotdipper /usr/local/bin/

# Linux x86_64
curl -LO https://github.com/psyysp/dotdipper/releases/latest/download/dotdipper-x86_64-unknown-linux-gnu.tar.gz
tar -xzf dotdipper-x86_64-unknown-linux-gnu.tar.gz
sudo mv dotdipper /usr/local/bin/

# Linux ARM64
curl -LO https://github.com/psyysp/dotdipper/releases/latest/download/dotdipper-aarch64-unknown-linux-gnu.tar.gz
tar -xzf dotdipper-aarch64-unknown-linux-gnu.tar.gz
sudo mv dotdipper /usr/local/bin/
```

#### Build from Source

```bash
# Prerequisites: Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build
git clone https://github.com/psyysp/dotdipper
cd dotdipper
cargo install --path .

# Verify
dotdipper --version
```

#### Install age (Required for Secrets)

When using the Nix flake, `age` is included on the binary’s `PATH`—no separate install needed. For other install methods:

```bash
# macOS
brew install age

# Ubuntu/Debian
sudo apt install age

# Arch Linux
sudo pacman -S age

# Fedora
sudo dnf install age

# Nix (if not using the dotdipper flake)
nix profile install nixpkgs#age

# Windows
scoop install age
```

### First-Time Setup

```bash
# 1. Initialize
dotdipper init

# 2. Setup secrets (optional)
dotdipper secrets init

# 3. Discover dotfiles
dotdipper discover --write

# 4. Create initial snapshot
dotdipper snapshot -m "Initial snapshot"

# 5. Push to GitHub (configure GitHub section in config first)
dotdipper push -m "Initial commit"
```

### New Machine Setup

```bash
# 1. Install dotdipper (see above)
# 2. Initialize
dotdipper init

# 3. Pull your dotfiles
dotdipper pull

# 4. Review changes
dotdipper diff --detailed

# 5. Apply selectively
dotdipper apply --interactive

# 6. Install packages
dotdipper install
```

---

## 📚 Core Features

### 🔐 Secrets Management

Securely manage sensitive dotfiles with age encryption:

```bash
# Initialize encryption
dotdipper secrets init

# Encrypt files
dotdipper secrets encrypt ~/.aws/credentials
# Creates: ~/.aws/credentials.age

# Edit encrypted files seamlessly
dotdipper secrets edit ~/.ssh/config.age

# Auto-decrypts during apply (in-memory only)
dotdipper apply
```

**Security Features:**

- Age encryption with public/private keys
- In-memory decryption (never writes plaintext to repo)
- Seamless edit workflow (decrypt → edit → re-encrypt)
- 0600 permissions on key files

### 🎯 Selective Apply & Diff

Review changes and selectively apply configurations:

```bash
# See what would change
dotdipper diff --detailed

# Interactive selection
dotdipper apply --interactive

# Apply specific files
dotdipper apply --only "~/.zshrc,~/.config/nvim"
```

**Features:**

- Pre-apply diffs with colored output
- Interactive TUI for file selection
- Path filtering (files or directories)
- Binary file detection

### 📸 Snapshot Management

Create point-in-time snapshots with efficient storage:

```bash
# Create snapshot
dotdipper snapshot create -m "Before major update"

# List snapshots
dotdipper snapshot list

# Rollback to snapshot
dotdipper snapshot rollback <id>

# Delete snapshot
dotdipper snapshot delete <id>
```

**Features:**

- Hardlink optimization for efficient storage
- ISO-8601 timestamp IDs
- Safety snapshots before rollback
- Metadata tracking (file count, size, message)

#### Auto-Pruning

Automatically prune old snapshots after creation to manage disk space:

```toml
[auto_prune]
enabled = true
keep_count = 10      # Keep 10 most recent snapshots
keep_age = "30d"     # Keep snapshots from last 30 days
keep_size = "1GB"    # Keep until total size exceeds 1GB
```

Any combination of criteria can be used. Snapshots are kept if they match ANY criterion. Auto-pruning runs automatically after each snapshot creation.

### 👤 Multiple Profiles

Manage different dotfile sets for different contexts:

```bash
# Create profiles
dotdipper profile create work
dotdipper profile create personal

# Switch profiles
dotdipper profile switch work

# List profiles
dotdipper profile list

# Remove profile
dotdipper profile remove work
```

**Features:**

- Base + overlay config merging
- Per-profile manifests and compiled directories
- Profile-specific configurations
- Legacy migration support

### ☁️ Cloud Backups

Push/pull dotfiles to remote storage:

```bash
# Configure LocalFS remote
dotdipper remote set localfs --endpoint ~/dotfiles-backup

# Configure S3 remote (requires --features s3)
dotdipper remote set s3 --bucket my-dotfiles --region us-east-1
# Set credentials via environment:
export AWS_ACCESS_KEY_ID=your-key
export AWS_SECRET_ACCESS_KEY=your-secret
# Or use custom S3-compatible endpoint (MinIO, DigitalOcean Spaces):
export AWS_ENDPOINT_URL=https://nyc3.digitaloceanspaces.com

# Configure WebDAV remote (requires --features webdav)
dotdipper remote set webdav --endpoint https://cloud.example.com/remote.php/webdav
# Set credentials via environment:
export WEBDAV_USERNAME=your-username
export WEBDAV_PASSWORD=your-password

# Show configuration
dotdipper remote show

# Push to remote
dotdipper remote push

# Pull from remote
dotdipper remote pull
```

**Supported Backends:**

- ✅ LocalFS (fully implemented)
- ✅ S3 (fully implemented, feature-gated)
- ✅ WebDAV (fully implemented, feature-gated)

**Features:**

- Compressed bundles (tar.zst)
- Bundle metadata tracking
- Dry-run support
- Profile-aware backups
- S3-compatible storage support (MinIO, DigitalOcean Spaces)
- WebDAV servers (Nextcloud, ownCloud, etc.)

### 🔀 Git vs Remote Backends: When to Use Each

Dotdipper provides two ways to sync your dotfiles to the cloud:

| Feature | GitHub Sync (`push`/`pull`) | Remote Backends (`remote push`/`remote pull`) |
|---------|----------------------------|----------------------------------------------|
| **Storage** | Git repository (GitHub, GitLab, etc.) | S3, WebDAV, LocalFS |
| **Version Control** | Full git history | Bundle-based (latest only by default) |
| **Collaboration** | Pull requests, issues, forks | Not designed for collaboration |
| **Setup Complexity** | Requires git + GitHub token | Environment variables only |
| **File Size Limits** | GitHub's limits apply | No practical limits |
| **Privacy** | Public/private repos | Fully private (your storage) |
| **Offline Access** | Clone repo locally | Download bundle when needed |

**Use GitHub Sync when you want:**

- Version history of all changes
- Collaboration with others
- Public sharing of your dotfiles
- Integration with GitHub workflows
- Easy cloning on new machines (`git clone`)

```bash
# GitHub workflow
dotdipper push -m "Update vim config"
dotdipper pull --apply
```

**Use Remote Backends when you want:**

- Simple backups without git complexity
- Private storage (your own S3/WebDAV)
- No GitHub account required
- Large files that exceed git limits
- Integration with existing cloud storage

```bash
# Remote backend workflow
dotdipper remote push
dotdipper remote pull
```

**Combined Approach:** You can use both! Use GitHub for version control and collaboration, while also pushing backups to S3/WebDAV for redundancy.

### 🤖 Auto-Sync Daemon (Opt-In)

The daemon watches your dotfiles and automatically creates snapshots when changes are detected. This is an **opt-in feature** that must be explicitly enabled in your configuration.

**When to use the daemon:**

- You want automatic backups without running `dotdipper snapshot` manually
- You make frequent changes to your dotfiles
- You want to catch every change without thinking about it

**When NOT to use the daemon:**

- You prefer manual control over when snapshots are created
- You're on a resource-constrained system
- You only occasionally modify dotfiles

**Setup:**

```bash
# 1. Enable the daemon (creates config if needed)
dotdipper daemon enable

# 2. Start the daemon
dotdipper daemon start

# 3. Check status
dotdipper daemon status

# 4. Stop the daemon
dotdipper daemon stop

# 5. Disable the daemon
dotdipper daemon disable
```

Or manually configure in `config.toml`:

```toml
[daemon]
enabled = true
mode = "ask"      # "ask" = prompt before snapshot, "auto" = auto-snapshot
debounce_ms = 1500  # Wait time after changes before processing
```

**Features:**

- File watching with configurable debouncing
- Two modes: "auto" (automatic snapshots) or "ask" (prompt before snapshot)
- PID file management for single-instance enforcement
- Graceful start/stop with cleanup
- CLI commands to enable/disable without editing config

### 🪝 Hooks System

Automate workflows with custom hooks:

```toml
[hooks]
pre_apply = ["echo 'Starting...'"]
post_apply = [
    "tmux source-file ~/.tmux.conf || true",
    "source ~/.zshrc"
]
post_snapshot = ["git add -A && git commit -m 'Snapshot' || true"]
```

**Use Cases:**

- Reload services after apply
- Auto-commit snapshots
- Validate configs before apply
- Custom backup strategies

---

## ⚙️ Configuration

Configuration is stored in `~/.dotdipper/config.toml`:

```toml
[general]
default_mode = "symlink"  # or "copy"
backup = true
active_profile = "default"
tracked_files = [
    "~/.zshrc",
    "~/.config/nvim",
    "~/.tmux.conf"
]

[github]
username = "psyysp"
repo_name = "dotfiles"
private = true

[secrets]
provider = "age"
key_path = "~/.config/age/keys.txt"

[hooks]
post_apply = ["tmux source-file ~/.tmux.conf || true"]

# Daemon is opt-in - uncomment to enable
# [daemon]
# enabled = true
# mode = "ask"  # or "auto"
# debounce_ms = 1500

[remote]
kind = "localfs"
endpoint = "~/dotfiles-backup"

# Per-file overrides
[files."~/.config/nvim"]
mode = "copy"

[files."~/.ssh/config"]
exclude = true

# Discovery patterns
include_patterns = ["~/.config/**", "~/.zshrc"]
exclude_patterns = ["~/.ssh/**", "**/*.key"]

[packages]
common = ["git", "vim", "tmux"]
macos = ["neovim", "fzf", "bat"]
linux = ["neovim", "fzf", "bat"]
```

---

## 📖 Command Reference

### Core Commands

```bash
dotdipper init                    # Initialize dotdipper
dotdipper discover [--write]      # Find dotfiles
dotdipper discover --packages     # Discover required packages from dotfiles
dotdipper snapshot [-m "msg"]     # Create snapshot (legacy)
dotdipper status [--detailed]     # Check status
dotdipper config --show | --edit  # View/edit config
dotdipper doctor [--fix]          # Health check
```

### Secrets Commands

```bash
dotdipper secrets init                # Setup encryption
dotdipper secrets encrypt <file>      # Encrypt file
dotdipper secrets decrypt <file>      # Decrypt file
dotdipper secrets edit <file>         # Edit encrypted file
```

### Diff & Apply

```bash
dotdipper diff [--detailed]                    # Show changes
dotdipper apply [--interactive]                # Apply changes
dotdipper apply --only "~/.zshrc"              # Apply specific files
dotdipper apply --force                        # No confirmations
```

### Snapshot Management

```bash
dotdipper snapshot create [-m "msg"]  # Create snapshot
dotdipper snapshot list               # List snapshots
dotdipper snapshot rollback <id>      # Rollback
dotdipper snapshot delete <id>        # Delete snapshot
dotdipper snapshot prune              # Prune old snapshots
```

**Pruning options:**

```bash
# Keep only the 10 most recent snapshots
dotdipper snapshot prune --keep-count 10

# Keep snapshots from the last 30 days
dotdipper snapshot prune --keep-age 30d

# Keep snapshots until total size exceeds 1GB
dotdipper snapshot prune --keep-size 1GB

# Combine criteria (keep if ANY criterion is met)
dotdipper snapshot prune --keep-count 5 --keep-age 7d

# Dry run - see what would be deleted without deleting
dotdipper snapshot prune --keep-count 5 --dry-run
```

### Profile Management

```bash
dotdipper profile list              # List profiles
dotdipper profile create <name>     # Create profile
dotdipper profile switch <name>     # Switch profile
dotdipper profile remove <name>     # Remove profile
```

### Remote Backups

```bash
dotdipper remote set <kind>         # Configure remote
dotdipper remote show               # Show config
dotdipper remote push               # Push to remote
dotdipper remote pull               # Pull from remote
```

### Daemon

```bash
dotdipper daemon enable             # Enable daemon in config
dotdipper daemon disable            # Disable daemon in config
dotdipper daemon start              # Start daemon
dotdipper daemon status             # Check status
dotdipper daemon stop               # Stop daemon
```

### GitHub Sync

```bash
dotdipper push [-m "msg"]           # Push to GitHub
dotdipper pull [--apply]            # Pull from GitHub
```

### Package Management

```bash
# Discover packages from your dotfiles
dotdipper discover --packages                     # Auto-detect required packages
dotdipper discover --packages --validate          # Check which are already installed
dotdipper discover --packages --write             # Add discovered packages to config
dotdipper discover --packages --include-low-confidence  # Include uncertain matches

# Install packages
dotdipper install [--dry-run]       # Install packages
dotdipper install --target-os ubuntu  # Target specific OS
```

---

## 🎓 Common Workflows

### Daily Workflow

```bash
# Make changes
vim ~/.zshrc

# Create snapshot
dotdipper snapshot create -m "Updated aliases"

# Push to GitHub
dotdipper push -m "Update zsh config"
```

### Managing Secrets

```bash
# Encrypt credential
dotdipper secrets encrypt ~/.aws/credentials

# Track encrypted version
# Add ~/.aws/credentials.age to tracked_files in config

# Snapshot and push
dotdipper snapshot create -m "Add AWS creds"
dotdipper push

# On new machine
dotdipper pull
dotdipper apply  # Auto-decrypts
```

### Selective Updates

```bash
# Pull latest
dotdipper pull

# Review changes
dotdipper diff --detailed

# Apply specific files
dotdipper apply --only "~/.zshrc,~/.bashrc"

# Or use interactive mode
dotdipper apply --interactive
```

### Multi-Profile Setup

```bash
# Create work profile
dotdipper profile create work

# Switch to work
dotdipper profile switch work

# Work-specific snapshot
dotdipper snapshot create -m "Work dotfiles"

# Switch back to personal
dotdipper profile switch default
```

---

## 🏗️ Feature Status

### ✅ Fully Implemented

- **Milestone 1:** Secrets Encryption (age encryption)
- **Milestone 2:** Selective Apply & Diff (interactive TUI)
- **Milestone 3:** Snapshot Management (hardlink optimization)
- **Milestone 4:** Multi-Profile Support (overlay semantics)
- **Milestone 5:** Remote Backends (LocalFS, S3, WebDAV fully implemented)
- **Milestone 6:** Auto-Sync Daemon (file watching, debouncing)
- **Core Features:** Init, discover, status, push, pull
- **Hooks System:** Pre/post hooks for operations
- **Package Management:** OS-specific installation
- **GitHub Sync:** Full push/pull support

---

## 🛡️ Safety Features

Dotdipper is designed with safety as a core principle:

- **HOME Boundary Enforcement** - Refuses operations outside `$HOME`
- **Backup Creation** - Creates `.bak.<timestamp>` backups
- **Confirmation Prompts** - Interactive confirmations
- **Hash-Based Detection** - BLAKE3 hashing
- **Deterministic Behavior** - Sorted manifests
- **No Plaintext Secrets** - In-memory decryption only

---

## 🔍 Troubleshooting

### Age not found

```bash
brew install age  # macOS
sudo apt install age  # Ubuntu
```

### Permission denied

```bash
chmod 600 ~/.config/age/keys.txt
dotdipper apply --force
```

### Diff fails

```bash
which git  # Ensure git is installed
dotdipper diff  # Without --detailed
```

### Hook fails

```bash
# Test manually
sh -c "your-hook-command"

# Make non-fatal
post_apply = ["command || true"]
```

---

## 📊 Platform Support

- **macOS:** Full support ✅
- **Linux:** Full support (Ubuntu, Arch, Fedora) ✅
- **Windows:** Future milestone 🚧

---

## 🧪 Testing

```bash
# Run all tests
cargo test

# Run specific test suite
cargo test --test secrets_tests
cargo test --test snapshots_tests
cargo test --test profiles_tests

# Build and run
cargo build --release
./target/release/dotdipper --help
```

---

## 🤝 Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Write tests for new features
4. Run `cargo test` and `cargo clippy`
5. Submit a pull request

---

## 📄 License

MIT License - See LICENSE file for details

---

## 🙏 Acknowledgments

- **Age:** Modern encryption by Filippo Valsorda
- **Rust Community:** Amazing crates ecosystem

---

## 📞 Support

- **Documentation:** This README and `dotdipper --help`
- **Issues:** Report bugs via GitHub Issues
- **Help:** Run `dotdipper <command> --help` for any command

---

**Version:** 0.3.0 (All Milestones Complete)  
**Status:** Production-ready  
**Last Updated:** January 20, 2026  
**Installation:** `brew tap psyysp/dotdipper && brew install dotdipper`

**Happy dotfile management! 🚀**

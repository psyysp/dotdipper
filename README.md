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
- 📦 **Package Management** - Bootstrap new machines with system packages
- 🔍 **Smart Diff** - Git-style diffs before applying changes
- 🛡️ **Safety First** - Backups, confirmations, and HOME boundary enforcement

---

## 🚀 Quick Start

### Prerequisites

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install age (for secrets encryption)
# macOS
brew install age

# Ubuntu/Debian
sudo apt install age

# Arch Linux
sudo pacman -S age
```

### Installation

```bash
# Clone and build
git clone https://github.com/yourusername/dotdipper
cd dotdipper
cargo install --path .

# Verify
dotdipper --version
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
# Configure remote
dotdipper remote set localfs --endpoint ~/dotfiles-backup
# or
dotdipper remote set s3 --bucket my-dotfiles --region us-east-1

# Show configuration
dotdipper remote show

# Push to remote
dotdipper remote push

# Pull from remote
dotdipper remote pull
```

**Supported Backends:**

- ✅ LocalFS (fully implemented)
- 🚧 S3 (feature-gated, stub implementation)
- 🚧 WebDAV (feature-gated, stub implementation)

**Features:**

- Compressed bundles (tar.zst)
- Bundle metadata tracking
- Dry-run support
- Profile-aware backups

### 🤖 Auto-Sync Daemon

Automatically watch files and create snapshots:

```bash
# Start daemon
dotdipper daemon start

# Check status
dotdipper daemon status

# Stop daemon
dotdipper daemon stop
```

**Features:**

- File watching with debouncing
- Two modes: "auto" (automatic) or "ask" (prompt)
- PID file management
- Graceful start/stop

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
username = "yourusername"
repo_name = "dotfiles"
private = true

[secrets]
provider = "age"
key_path = "~/.config/age/keys.txt"

[hooks]
post_apply = ["tmux source-file ~/.tmux.conf || true"]

[daemon]
enabled = true
mode = "ask"  # or "auto"
debounce_ms = 1500

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
- **Milestone 5:** Remote Backends (LocalFS complete, S3/WebDAV stubs)
- **Milestone 6:** Auto-Sync Daemon (file watching, debouncing)
- **Core Features:** Init, discover, status, push, pull
- **Hooks System:** Pre/post hooks for operations
- **Package Management:** OS-specific installation
- **GitHub Sync:** Full push/pull support

### 🚧 Partial Implementation

- **S3 Backend:** Interface ready, needs API implementation
- **WebDAV Backend:** Interface ready, needs API implementation

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
**Last Updated:** November 11, 2025

**Happy dotfile management! 🚀**

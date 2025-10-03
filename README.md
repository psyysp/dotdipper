# Dotdipper

> A safe, deterministic, and feature-rich dotfiles manager built in Rust with encryption, selective apply, and cloud sync capabilities.

[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

---

## 🎯 What is Dotdipper?

Dotdipper is a best-in-class dotfiles manager that helps you synchronize, manage, and deploy your configuration files across multiple machines. Built with safety and determinism as core principles, it provides powerful features like:

- 🔐 **Secrets Encryption** - Manage sensitive config files with age encryption
- 🎯 **Selective Apply** - Review and choose which files to apply
- 🪝 **Hooks System** - Automate workflows with pre/post hooks
- 🔄 **GitHub Sync** - Push/pull dotfiles to/from GitHub
- 📦 **Package Management** - Bootstrap new machines with system packages
- 🔍 **Smart Diff** - Git-style diffs before applying changes
- 🛡️ **Safety First** - Backups, confirmations, and HOME boundary enforcement

---

## 🌟 Key Features

### 🔐 Secrets Management (Milestone 1)

Securely manage sensitive dotfiles with age encryption:

```bash
# Initialize encryption
dotdipper secrets init

# Encrypt sensitive files
dotdipper secrets encrypt ~/.aws/credentials
# Creates: ~/.aws/credentials.age

# Edit encrypted files seamlessly (decrypt → edit → re-encrypt)
dotdipper secrets edit ~/.ssh/config.age

# Auto-decrypt during apply (in-memory only, never writes plaintext)
dotdipper apply
```

**Security Features:**

- ✅ Age encryption with public/private key pairs
- ✅ In-memory decryption (never writes plaintext to repo)
- ✅ Seamless edit workflow with automatic re-encryption
- ✅ Restrictive permissions (0600) on key files
- ✅ SOPS compatibility (stub for future implementation)

### 🎯 Selective Apply & Diff (Milestone 2)

Review changes and selectively apply configurations:

```bash
# See what would change with git-style diffs
dotdipper diff --detailed

# Interactive selection with TUI
dotdipper apply --interactive

# Apply specific files only
dotdipper apply --only "~/.zshrc,~/.config/nvim"

# Apply entire directory
dotdipper apply --only "~/.config/kitty"
```

**Features:**

- ✅ Pre-apply diffs with colored output
- ✅ Interactive TUI for file selection
- ✅ Path filtering (files or directories)
- ✅ Binary file detection and handling
- ✅ Summary counts and detailed listings

### 🪝 Hooks System

Automate your workflow with custom hooks:

```toml
[hooks]
pre_apply = ["echo 'Backing up current configs...'"]
post_apply = [
    "tmux source-file ~/.tmux.conf || true",
    "source ~/.zshrc"
]
pre_snapshot = ["echo 'Creating snapshot...'"]
post_snapshot = ["git add -A && git commit -m 'Auto-snapshot' || true"]
```

**Common Use Cases:**

- Reload configurations after applying
- Auto-commit snapshots to Git
- Validate syntax before applying
- Backup critical files

### 🛡️ Safety Features

Dotdipper is designed with safety as a core principle:

- **HOME Boundary Enforcement** - Refuses operations outside `$HOME` by default
- **Backup Creation** - Creates `.bak.<timestamp>` backups before overwriting
- **Confirmation Prompts** - Interactive confirmations unless `--force` is used
- **Hash-Based Detection** - BLAKE3 hashing for reliable change detection
- **Deterministic Behavior** - Sorted manifests and idempotent operations

---

## 🚀 Quick Start

### Prerequisites

- **Rust and Cargo** - Install from [rustup.rs](https://rustup.rs/)
- **Git** - Required for GitHub sync
- **Age** (optional) - For secrets encryption

```bash
# macOS
brew install age

# Ubuntu/Debian
sudo apt install age

# Arch Linux
sudo pacman -S age
```

### Installation

```bash
# Clone the repository
git clone https://github.com/yourusername/dotdipper
cd dotdipper

# Build and install
cargo install --path .

# Verify installation
dotdipper --version
```

### First-Time Setup

```bash
# 1. Initialize dotdipper
dotdipper init

# 2. (Optional) Setup secrets encryption
dotdipper secrets init

# 3. Discover existing dotfiles
dotdipper discover --write

# 4. Create initial snapshot
dotdipper snapshot -m "Initial snapshot"

# 5. Configure GitHub sync (edit config.toml)
dotdipper config --edit

# 6. Push to GitHub
dotdipper push -m "Initial commit"
```

### New Machine Setup

```bash
# 1. Install dotdipper (see Installation above)
dotdipper init

# 2. Configure GitHub (edit ~/.dotdipper/config.toml)
dotdipper config --edit

# 3. Pull your dotfiles
dotdipper pull

# 4. Review what will change
dotdipper diff --detailed

# 5. Apply selectively
dotdipper apply --interactive

# 6. Install system packages
dotdipper install
```

---

## 📖 Usage

### Core Commands

```bash
# Initialize dotdipper
dotdipper init

# Discover dotfiles on your system
dotdipper discover --write

# Create a snapshot of current dotfiles
dotdipper snapshot -m "Updated zsh config"

# Check status of tracked files
dotdipper status --detailed

# Push to GitHub
dotdipper push -m "Update configurations"

# Pull from GitHub
dotdipper pull

# Health check
dotdipper doctor
```

### Secrets Commands

```bash
# Initialize age encryption
dotdipper secrets init

# Encrypt a file
dotdipper secrets encrypt ~/.aws/credentials

# Decrypt a file
dotdipper secrets decrypt ~/.aws/credentials.age

# Edit encrypted file (auto decrypt/encrypt)
dotdipper secrets edit ~/.ssh/config.age
```

### Diff & Apply Commands

```bash
# View differences
dotdipper diff                    # Summary only
dotdipper diff --detailed         # Full git-style diff

# Apply changes
dotdipper apply                   # Apply all with confirmations
dotdipper apply --interactive     # Choose files with TUI
dotdipper apply --force           # Apply all without confirmations
dotdipper apply --only "~/.zshrc" # Apply specific file(s)
```

---

## ⚙️ Configuration

Configuration is stored in `~/.dotdipper/config.toml`. Here's a comprehensive example:

```toml
[general]
# Default mode for file operations
default_mode = "symlink"  # or "copy"

# Create backups before overwriting
backup = true

# Active profile (for multi-profile support)
active_profile = "default"

# Files to track
tracked_files = [
    "~/.zshrc",
    "~/.bashrc",
    "~/.config/nvim",
    "~/.tmux.conf",
    "~/.gitconfig"
]

# Include patterns (glob-style)
include_patterns = [
    "~/.config/**",
    "~/.zshrc",
    "~/.bashrc"
]

# Exclude patterns
exclude_patterns = [
    "~/.ssh/**",
    "~/.gnupg/**",
    "**/*.key",
    "**/node_modules/**"
]

[github]
username = "yourusername"
repo_name = "dotfiles"
private = true

[secrets]
provider = "age"  # or "sops" (future)
key_path = "~/.config/age/keys.txt"

[hooks]
pre_apply = ["echo 'Applying dotfiles...'"]
post_apply = [
    "tmux source-file ~/.tmux.conf || true",
    "source ~/.zshrc"
]
pre_snapshot = []
post_snapshot = ["git add -A && git commit -m 'Auto-snapshot' || true"]

# Per-file overrides
[files."~/.config/nvim"]
mode = "copy"  # Copy instead of symlink

[files."~/.ssh/config"]
exclude = true  # Never track this file

[files."~/.gitconfig"]
mode = "copy"  # Allow local modifications
```

---

## 🎓 Common Workflows

### Daily Workflow

```bash
# Make changes to your dotfiles
vim ~/.zshrc

# Create snapshot
dotdipper snapshot -m "Updated zsh aliases"

# Push to GitHub
dotdipper push -m "Update zsh configuration"
```

### Managing Secrets

```bash
# Add new encrypted credential
dotdipper secrets encrypt ~/.aws/credentials

# Move encrypted version to tracked location
mv ~/.aws/credentials.age ~/dotfiles/aws/credentials.age

# Add to config.toml tracked_files
dotdipper config --edit

# Snapshot and push
dotdipper snapshot -m "Add AWS credentials"
dotdipper push

# On another machine
dotdipper pull
dotdipper apply  # Auto-decrypts .age files in-memory
```

### Selective Updates

```bash
# Pull latest changes
dotdipper pull

# Review all changes
dotdipper diff --detailed

# Apply only shell configs
dotdipper apply --only "~/.zshrc,~/.bashrc"

# Or use interactive mode
dotdipper apply --interactive
```

### Multi-Machine Setup

```bash
# Machine-specific overrides in config.toml
[files."~/.gitconfig"]
mode = "copy"  # Allow different user.email per machine

[files."~/.config/alacritty/alacritty.yml"]
mode = "copy"  # Allow different font sizes per machine
```

---

## 🏗️ Project Status

### ✅ Implemented Features

- **Milestone 1: Secrets Encryption** - Full age encryption support
- **Milestone 2: Selective Apply & Diff** - Interactive apply with diffs
- **Hooks System** - Pre/post hooks for apply and snapshot
- **Core Features** - Init, discover, snapshot, status, push, pull
- **GitHub Sync** - Push/pull to GitHub repositories
- **Package Management** - OS-specific package installation

### 🚧 Future Milestones (Stub Implementation)

- **Milestone 3: Snapshot Management** - Advanced snapshot operations
- **Milestone 4: Multi-Profile Support** - Switch between profiles
- **Milestone 5: Remote Backends** - S3, GCS, WebDAV support
- **Milestone 6: Auto-Sync Daemon** - Automatic file watching

For detailed milestone status, see [MILESTONE_STATUS.md](MILESTONE_STATUS.md).

---

## 📚 Documentation

- **[Quick Start Guide](QUICK_START.md)** - Get started quickly
- **[Commands Reference](COMMANDS_REFERENCE.md)** - All commands and options
- **[Features Overview](README_FEATURES.md)** - Detailed feature descriptions
- **[Architecture](ARCHITECTURE.md)** - System design and modules
- **[Build & Test](BUILD_AND_TEST.md)** - Build instructions and testing
- **[Remote Backends](REMOTE_BACKENDS.md)** - Cloud sync configuration
- **[Verification Checklist](VERIFICATION_CHECKLIST.md)** - Testing checklist

---

## 🧪 Testing

```bash
# Run all tests
cargo test

# Run specific test suite
cargo test --test secrets_tests
cargo test --test diff_tests
cargo test --test full_workflow_test

# Run with verbose output
cargo test -- --nocapture

# Build and run
cargo build --release
./target/release/dotdipper --help
```

---

## 🛠️ Development

### Project Structure

```
dotdipper/
├── src/
│   ├── main.rs           # CLI entry point
│   ├── cfg/              # Configuration management
│   ├── secrets/          # Encryption & secrets
│   ├── diff/             # Diff & selective apply
│   ├── repo/             # Repository operations
│   ├── hash/             # File hashing (BLAKE3)
│   ├── vcs/              # Git & GitHub integration
│   ├── scan/             # Dotfiles discovery
│   ├── install/          # Package installation
│   ├── ui/               # User interface utilities
│   ├── snapshots/        # Snapshot management (stub)
│   ├── profiles/         # Profile management (stub)
│   ├── remote/           # Remote backends (stub)
│   └── daemon/           # Auto-sync daemon (stub)
├── tests/                # Integration tests
├── Cargo.toml            # Dependencies
└── docs/                 # Documentation
```

### Building from Source

```bash
# Debug build
cargo build

# Release build (optimized)
cargo build --release

# Install locally
cargo install --path .

# Run without installing
cargo run -- init
```

### Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Write tests for new features
4. Run `cargo test` and `cargo clippy`
5. Submit a pull request

---

## 🔍 Troubleshooting

### Common Issues

**Age command not found:**

```bash
# macOS
brew install age

# Ubuntu/Debian
sudo apt install age

# Arch Linux
sudo pacman -S age
```

**Permission denied on apply:**

```bash
# Check file ownership
ls -la ~/.dotdipper/compiled/

# Fix with force flag
dotdipper apply --force
```

**Diff fails:**

```bash
# Ensure git is installed
which git

# Use summary mode instead
dotdipper diff  # Without --detailed
```

**Hook execution fails:**

```bash
# Make hooks non-fatal with || true
post_apply = ["tmux source-file ~/.tmux.conf || true"]

# Test hook manually
sh -c "your-hook-command"
```

### Getting Help

```bash
# General help
dotdipper --help

# Command-specific help
dotdipper secrets --help
dotdipper apply --help

# Run diagnostics
dotdipper doctor

# View configuration
dotdipper config --show
```

---

## 💡 Tips & Best Practices

1. **Always review diffs before applying:**

   ```bash
   dotdipper pull && dotdipper diff --detailed && dotdipper apply --interactive
   ```

2. **Use hooks to automate workflows:**
   - Reload configurations after applying
   - Auto-commit snapshots
   - Validate syntax before applying

3. **Encrypt sensitive files:**
   - SSH keys
   - AWS/GCP credentials
   - API tokens
   - GPG keys

4. **Use per-file overrides wisely:**
   - Copy (not symlink) configs that need machine-specific changes
   - Exclude truly sensitive files from tracking
   - Symlink stable configs for easy updates

5. **Test on a VM first:**

   ```bash
   # In VM
   dotdipper pull
   dotdipper diff --detailed
   dotdipper apply --interactive
   ```

---

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---

## 🙏 Acknowledgments

- Built with [Rust](https://www.rust-lang.org/)
- Encryption via [age](https://github.com/FiloSottile/age)
- BLAKE3 hashing for fast file integrity checks
- Terminal UI with [dialoguer](https://github.com/console-rs/dialoguer)

---

## 📞 Support

- **Documentation:** See [QUICK_START.md](QUICK_START.md) and other docs
- **Issues:** Report bugs or request features via GitHub Issues
- **Discussions:** Join GitHub Discussions for questions

---

**Happy dotfile management! 🚀**

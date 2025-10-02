# Dotdipper - Best-in-Class Dotfiles Manager

> A safe, deterministic, and feature-rich dotfiles manager with encryption, selective apply, and cloud sync.

## 🌟 What's New (Milestones 1-2)

### 🔐 Secrets Encryption (Milestone 1)

Securely manage sensitive dotfiles with age encryption:

```bash
# Initialize encryption
dotdipper secrets init

# Encrypt sensitive files
dotdipper secrets encrypt ~/.aws/credentials
# Creates: ~/.aws/credentials.age

# Edit encrypted files seamlessly
dotdipper secrets edit ~/.ssh/config.age

# Auto-decrypt during apply (in-memory only)
dotdipper apply  # Decrypts .age files transparently
```

**Features:**

- ✅ Age encryption (preferred) with sops compatibility
- ✅ In-memory decryption during apply (never writes plaintext to repo)
- ✅ Seamless edit workflow (decrypt → edit → re-encrypt)
- ✅ Public/private key management
- ✅ 0600 permissions on key files

### 🎯 Selective Apply & Diff (Milestone 2)

Review and selectively apply changes:

```bash
# See what would change
dotdipper diff --detailed

# Interactive selection
dotdipper apply --interactive

# Apply specific files only
dotdipper apply --only "~/.zshrc,~/.config/nvim"

# Apply directory
dotdipper apply --only "~/.config/kitty"
```

**Features:**

- ✅ Pre-apply diffs with git-style output
- ✅ Interactive TUI for file selection
- ✅ Path filtering (comma-separated or directory prefixes)
- ✅ Binary file detection and handling
- ✅ Colored, sorted output

### 🪝 Hooks System

Automate your workflow with custom hooks:

```toml
[hooks]
pre_apply = ["echo 'Backing up...'"]
post_apply = [
    "tmux source-file ~/.tmux.conf || true",
    "source ~/.zshrc"
]
pre_snapshot = []
post_snapshot = ["git add -A && git commit -m 'Snapshot' || true"]
```

**Use Cases:**

- Reload configs after apply
- Auto-commit snapshots
- Validate syntax before apply
- Backup critical files

## 🚀 Quick Start

### Installation

```bash
# Install Rust and cargo first
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build
git clone https://github.com/yourusername/dotdipper
cd dotdipper
cargo install --path .

# Install age for secrets (optional but recommended)
# macOS
brew install age

# Ubuntu/Debian
apt install age

# Arch
pacman -S age
```

### Basic Setup

```bash
# 1. Initialize
dotdipper init

# 2. Discover dotfiles
dotdipper discover --write

# 3. Create initial snapshot
dotdipper snapshot -m "Initial setup"

# 4. Push to GitHub
dotdipper push -m "Initial commit"
```

### With Secrets

```bash
# 1. Setup secrets
dotdipper secrets init

# 2. Encrypt sensitive files
dotdipper secrets encrypt ~/.aws/credentials
dotdipper secrets encrypt ~/.ssh/config

# 3. Add encrypted files to tracking
# Edit ~/.dotdipper/config.toml:
# tracked_files = [
#   "~/.aws/credentials.age",
#   "~/.ssh/config.age"
# ]

# 4. Snapshot and push
dotdipper snapshot -m "Added encrypted AWS credentials"
dotdipper push
```

### On a New Machine

```bash
# 1. Install dotdipper and age
brew install age
cargo install --path /path/to/dotdipper

# 2. Initialize
dotdipper init

# 3. Copy your age key from secure location
cp /secure/backup/keys.txt ~/.config/age/keys.txt
chmod 600 ~/.config/age/keys.txt

# 4. Pull dotfiles
dotdipper pull

# 5. Review what would change
dotdipper diff --detailed

# 6. Apply selectively
dotdipper apply --interactive

# 7. Install packages
dotdipper install
```

## 📖 Core Features

### ✅ Currently Implemented

#### Dotfiles Management

- **Hybrid mode:** Default symlink with per-file copy override
- **Backup creation:** Timestamped backups before overwrite
- **Idempotent apply:** Detects already-applied files
- **Safety checks:** Refuses operations outside $HOME (override with flag)

#### Secrets Encryption (Milestone 1)

- **Age encryption:** Modern, simple, secure
- **Edit workflow:** Transparent encrypt/decrypt/edit
- **In-memory decryption:** Never writes plaintext to repo
- **Key management:** Generate, import, validate keys

#### Diff & Apply (Milestone 2)

- **Pre-apply diffs:** Git-style diffs with colors
- **Interactive selection:** TUI for choosing files
- **Path filtering:** `--only` for specific paths
- **Binary handling:** Smart detection and comparison

#### Hooks & Automation

- **Pre/post hooks:** For apply and snapshot
- **Shell integration:** Full shell command support
- **Error handling:** Hooks can stop execution

#### Discovery & Scanning

- **Smart discovery:** Find dotfiles based on patterns
- **Gitignore-style patterns:** Include/exclude rules
- **Safety defaults:** Exclude secrets, caches, temp files

#### GitHub Integration

- **Push/pull:** Sync with GitHub repositories
- **Private repos:** Default to private
- **GitHub CLI:** Uses `gh` for authentication

#### Bootstrap & Installation

- **Multi-OS support:** macOS, Ubuntu, Arch, Fedora
- **Package management:** Brew, apt, pacman support
- **Generated scripts:** OS-specific install scripts

#### Health Checks

- **Doctor command:** Verify git, gh, age, config, manifest
- **Auto-fix:** Attempt repairs where possible
- **Helpful hints:** Installation instructions

### 🚧 Planned (Milestones 3-6)

All interfaces and CLI commands are ready - implementation pending:

#### Snapshots & Rollback (Milestone 3)

```bash
dotdipper snapshot-cmd create -m "Before major update"
dotdipper snapshot-cmd list
dotdipper snapshot-cmd rollback <id>
```

#### Multiple Profiles (Milestone 4)

```bash
dotdipper profile create work
dotdipper profile switch work
dotdipper profile list
```

#### Cloud Backups (Milestone 5)

```bash
dotdipper remote set s3
dotdipper remote push --dry-run
dotdipper remote pull
```

#### Auto-Sync Daemon (Milestone 6)

```bash
dotdipper daemon start
dotdipper daemon status
dotdipper daemon stop
```

## 📝 Configuration

### Complete Example

```toml
[general]
default_mode = "symlink"      # or "copy"
backup = true
active_profile = "default"
tracked_files = [
    "~/.zshrc",
    "~/.bashrc",
    "~/.config/nvim",
    "~/.tmux.conf",
    "~/.gitconfig"
]

[github]
username = "yourusername"
repo_name = "dotfiles"
private = true

[packages]
common = ["git", "vim", "tmux", "curl"]
macos = ["neovim", "fzf", "bat", "ripgrep"]
linux = ["neovim", "fzf", "bat", "ripgrep"]

[secrets]                      # Milestone 1 ✅
provider = "age"
key_path = "~/.config/age/keys.txt"

[hooks]                        # Implemented ✅
pre_apply = ["echo 'Applying dotfiles...'"]
post_apply = [
    "tmux source-file ~/.tmux.conf || true",
    "source ~/.zshrc"
]

# Per-file overrides
[files."~/.config/nvim"]
mode = "copy"                  # Copy instead of symlink

[files."~/.ssh/private_key"]
exclude = true                 # Never track this file

# Discovery patterns
include_patterns = [
    "~/.config/**",
    "~/.zshrc",
    "~/.bashrc",
    "~/.gitconfig"
]

exclude_patterns = [
    "~/.ssh/**",              # Exclude SSH keys
    "~/.gnupg/**",            # Exclude GPG keys
    "**/*.key",
    "**/node_modules/**",
    "**/cache/**"
]
```

## 🎯 Command Reference

### Essential Commands

```bash
# Initialize dotdipper
dotdipper init

# Find dotfiles
dotdipper discover [--write]

# Create snapshot
dotdipper snapshot [-m "message"]

# Check status
dotdipper status [--detailed]

# View configuration
dotdipper config --show
dotdipper config --edit
```

### New Commands (Milestones 1-2)

```bash
# Secrets Management
dotdipper secrets init                    # Setup encryption
dotdipper secrets encrypt <file>          # Encrypt a file
dotdipper secrets decrypt <file>          # Decrypt a file
dotdipper secrets edit <file>             # Edit encrypted file

# Diff & Apply
dotdipper diff [--detailed]               # Show changes
dotdipper apply                           # Apply all changes
dotdipper apply --interactive             # Choose files to apply
dotdipper apply --only "~/.zshrc"         # Apply specific files
dotdipper apply --force                   # Skip confirmations
```

### GitHub Sync

```bash
# Push to GitHub
dotdipper push [-m "message"]

# Pull from GitHub
dotdipper pull [--apply]

# Pull and apply with review
dotdipper pull
dotdipper diff --detailed
dotdipper apply --interactive
```

### System Bootstrap

```bash
# Install packages for current OS
dotdipper install

# Generate scripts only
dotdipper install --dry-run

# Target specific OS
dotdipper install --target-os ubuntu
```

### Health Check

```bash
# Run diagnostics
dotdipper doctor

# Auto-fix issues
dotdipper doctor --fix
```

## 🔒 Security Best Practices

### Encrypting Secrets

**DO:**

- ✅ Encrypt before committing: `dotdipper secrets encrypt <file>`
- ✅ Use `.age` suffix for encrypted files
- ✅ Back up your age key securely (offline, password manager)
- ✅ Set key permissions: `chmod 600 ~/.config/age/keys.txt`

**DON'T:**

- ❌ Commit plaintext secrets
- ❌ Share private keys
- ❌ Track unencrypted API tokens, passwords, SSH keys

### Example Workflow

```bash
# Bad - tracking plaintext secret
echo "API_KEY=secret123" > ~/.env
dotdipper snapshot  # ❌ Leaks secret to git

# Good - encrypt first
echo "API_KEY=secret123" > ~/.env
dotdipper secrets encrypt ~/.env
rm ~/.env  # Remove plaintext
# Track ~/.env.age instead

# On new machine
dotdipper pull
dotdipper secrets decrypt ~/.env.age
```

### Files to Always Encrypt

- `~/.aws/credentials`
- `~/.ssh/id_*` (private keys)
- `~/.gnupg/` directory
- API tokens and secrets in config files
- Password files
- Certificate private keys

## 🛠️ Advanced Usage

### Hooks Examples

#### Reload Services After Apply

```toml
[hooks]
post_apply = [
    "tmux source-file ~/.tmux.conf || true",
    "killall -USR1 kitty || true",
    "defaults read -g AppleInterfaceStyle 2>/dev/null | grep -q Dark && echo 'Dark mode' || echo 'Light mode'"
]
```

#### Auto-Commit After Snapshot

```toml
[hooks]
post_snapshot = [
    "cd ~/.dotdipper/compiled",
    "git add -A",
    "git commit -m \"Snapshot $(date '+%Y-%m-%d %H:%M:%S')\" || true"
]
```

#### Validate Config Before Apply

```toml
[hooks]
pre_apply = [
    "zsh -n ~/.zshrc || exit 1",
    "tmux -f ~/.tmux.conf -C 'display-message -p \"OK\"' || exit 1"
]
```

### Per-File Overrides

```toml
# Default: symlink everything
[general]
default_mode = "symlink"

# But copy neovim config (allow local changes)
[files."~/.config/nvim"]
mode = "copy"

# Exclude SSH private keys
[files."~/.ssh/id_rsa"]
exclude = true

# Copy gitconfig (allow per-machine user.email)
[files."~/.gitconfig"]
mode = "copy"
```

### Selective Apply Workflows

```bash
# Only update shell configs
dotdipper apply --only "~/.zshrc,~/.bashrc"

# Only update one directory
dotdipper apply --only "~/.config/kitty"

# Interactive - pick from list
dotdipper apply --interactive

# Force apply everything (no prompts)
dotdipper apply --force
```

## 🧪 Testing

### Run Tests

```bash
# All tests
cargo test

# With external tools (requires age, git, gh)
cargo test -- --ignored

# Specific test suite
cargo test --test secrets_tests
cargo test --test diff_tests
cargo test --test hooks_tests

# Verbose output
cargo test -- --nocapture
```

### Test Coverage

- ✅ Secrets encryption/decryption
- ✅ Diff generation and filtering
- ✅ Apply with various flags
- ✅ Hook execution and failure handling
- ✅ Config parsing with all sections
- ✅ Stub commands for future milestones

## 🗺️ Roadmap

### ✅ Milestone 1 - Secrets Encryption (COMPLETE)

- Age encryption with edit workflow
- In-memory decryption for apply
- Key management and validation

### ✅ Milestone 2 - Selective Apply & Diff (COMPLETE)

- Pre-apply diffs with git integration
- Interactive file selection
- Path filtering for targeted applies

### 🚧 Milestone 3 - Snapshots & Rollback (STUB READY)

- Time-based snapshots with metadata
- Efficient storage (hardlinks)
- Rollback to any snapshot
- Snapshot listing and management

### 🚧 Milestone 4 - Multiple Profiles (STUB READY)

- Work, personal, server profiles
- Base + overlay config merging
- Per-profile manifests
- Profile switching

### 🚧 Milestone 5 - Cloud Backups (STUB READY)

- S3, GCS, WebDAV remotes
- Credentials discovery from env/SDKs
- Dry-run for cost estimation
- Beyond-GitHub sync

### 🚧 Milestone 6 - Auto-Sync Daemon (STUB READY)

- File watching with debouncing
- Auto-snapshot on drift
- Ask or auto mode
- Background daemon

## 🏆 Competitive Features

| Feature | Dotdipper | Chezmoi | Dotbot | Yadm |
|---------|-----------|---------|--------|------|
| Symlink by default | ✅ | ✅ | ✅ | ✅ |
| Per-file copy override | ✅ | ✅ | ❌ | ❌ |
| Secrets encryption | ✅ (age) | ✅ (age) | ❌ | ✅ (GPG) |
| Interactive apply | ✅ | ❌ | ❌ | ❌ |
| Pre-apply diffs | ✅ | ✅ | ❌ | ✅ |
| Hooks | ✅ | ✅ | ✅ | ✅ |
| Multiple profiles | 🚧 | ✅ | ❌ | ✅ |
| Snapshots/rollback | 🚧 | ❌ | ❌ | ❌ |
| Cloud backups | 🚧 | ❌ | ❌ | ❌ |
| Auto-sync daemon | 🚧 | ❌ | ❌ | ❌ |
| Bootstrap installer | ✅ | ✅ | ✅ | ✅ |
| Cross-platform | ✅ | ✅ | ✅ | ⚠️ |

## 🎨 Design Philosophy

### Safety by Default

- Never operates outside $HOME (unless explicitly allowed)
- Prompts before destructive operations
- Creates backups automatically
- Excludes secrets by default

### Deterministic Behavior

- Sorted manifests for stable diffs
- BLAKE3 hashing for reliable change detection
- Idempotent operations
- Reproducible snapshots

### Clear UX

- Colored, informative output
- Progress bars for long operations
- Helpful hints and suggestions
- Rich error messages with context

### Extensible Architecture

- Modular design with clear interfaces
- Pluggable backends (secrets, remotes)
- Hook system for customization
- Profile system for different contexts

## 📚 Documentation

- **[QUICK_START.md](QUICK_START.md)** - Get started quickly
- **[ARCHITECTURE.md](ARCHITECTURE.md)** - Technical architecture
- **[MILESTONE_STATUS.md](MILESTONE_STATUS.md)** - Implementation status
- **[example-config.toml](example-config.toml)** - Full config reference

## 🤝 Common Workflows

### Daily Development

```bash
# Make changes
vim ~/.zshrc

# Snapshot
dotdipper snapshot -m "Updated aliases"

# Push to GitHub
dotdipper push -m "Update zsh config"
```

### Setting Up New Machine

```bash
# Pull and review
dotdipper pull
dotdipper diff --detailed

# Apply interactively
dotdipper apply --interactive

# Or apply everything
dotdipper apply --force
```

### Managing Secrets

```bash
# Add new secret
dotdipper secrets encrypt ~/.secret_token
# Track in config: tracked_files = ["~/.secret_token.age"]

# Edit existing secret
dotdipper secrets edit ~/.aws/credentials.age

# Update on all machines
dotdipper push
# On other machine:
dotdipper pull && dotdipper apply
```

### Selective Updates

```bash
# Only update shell configs
dotdipper pull
dotdipper apply --only "~/.zshrc,~/.bashrc"

# Only update editor
dotdipper apply --only "~/.config/nvim"

# Review and pick
dotdipper diff --detailed
dotdipper apply --interactive
```

## 🐛 Troubleshooting

### Age not found

```bash
# macOS
brew install age

# Ubuntu/Debian
sudo apt install age

# Arch
sudo pacman -S age

# Verify
which age && which age-keygen
```

### Permission denied

```bash
# Fix permissions
chmod 600 ~/.config/age/keys.txt
chmod 755 ~/.dotdipper/compiled

# Force apply
dotdipper apply --force
```

### Hook fails

```bash
# Test hook manually
sh -c "your-hook-command"

# Make non-fatal
post_apply = ["command || true"]

# Check hook output
dotdipper apply --verbose
```

### Diff shows unexpected changes

```bash
# Force re-snapshot
dotdipper snapshot --force

# Check manifest
cat ~/.dotdipper/manifest.lock | jq

# Verify file hash
blake3sum ~/.zshrc
```

## 🔬 Technical Details

### Dependencies

**Core:**

- `clap` - CLI parsing
- `serde` + `toml` - Config management
- `anyhow` - Error handling
- `blake3` - File hashing

**Encryption:**

- `age` - Age encryption
- `which` - Binary detection

**UI:**

- `colored` - Terminal colors
- `dialoguer` - Interactive prompts
- `indicatif` - Progress bars

**File System:**

- `walkdir` - Directory traversal
- `ignore` - Gitignore patterns
- `tempfile` - Secure temp files
- `shellexpand` - Tilde expansion

**Future:**

- `notify` - File watching (M6)
- `tera` - Templates (future)

### Platform Support

- **macOS:** Full support (primary development platform)
- **Linux:** Full support (Ubuntu, Arch, Fedora tested)
- **Windows:** Future milestone (interface ready)

### Performance

- Hashing: BLAKE3 (faster than SHA-256)
- Binary detection: 8KB sample scan
- Incremental snapshots: Only copy changed files
- Progress feedback: All long operations

## 📊 Architecture Highlights

### Module Responsibilities

```
cfg/       → Configuration parsing, validation
hash/      → BLAKE3 hashing, manifest management
repo/      → Snapshot, status, apply operations
secrets/   → Encryption/decryption with age/sops
diff/      → Diff generation, interactive selection
vcs/       → Git and GitHub operations
install/   → OS detection, package installation
ui/        → Progress bars, prompts, colors
```

### Data Flow

```
Discover → Hash → Snapshot → Manifest → Push → GitHub
                                                    ↓
New Machine ← Apply ← Diff ← Pull ← GitHub ← ← ← ←
```

### Configuration Hierarchy

```
Default values (code)
  ↓
config.toml [general]
  ↓
config.toml [files."<path>"]  (per-file override)
  ↓
CLI flags (--force, --only, etc.)
```

## 🎓 Learning Resources

### For Users

1. Start with [QUICK_START.md](QUICK_START.md)
2. Read example-config.toml for all options
3. Try `dotdipper <command> --help` for any command
4. Check MILESTONE_STATUS.md for feature availability

### For Developers

1. Read [ARCHITECTURE.md](ARCHITECTURE.md) for design decisions
2. Review module structure in `src/`
3. Check tests/ for usage examples
4. See stub modules for future implementation guides

## 📜 License

MIT License - See LICENSE file for details

## 🙏 Acknowledgments

- **Age:** Modern encryption tool by Filippo Valsorda
- **Chezmoi:** Inspiration for some features
- **Dotbot:** Hook system inspiration
- **Rust Community:** Amazing crates ecosystem

## 🚀 Getting Involved

### Reporting Issues

Use GitHub Issues for:

- Bug reports
- Feature requests
- Documentation improvements

### Contributing

1. Fork the repository
2. Create feature branch
3. Add tests for new features
4. Update documentation
5. Submit pull request

### Development Setup

```bash
git clone https://github.com/yourusername/dotdipper
cd dotdipper
cargo build
cargo test
cargo run -- --help
```

---

**Current Version:** 0.2.0 (Milestones 1-2 Complete)  
**Status:** Production-ready for secrets and selective apply  
**Next Release:** Milestone 3 (Snapshots & Rollback)

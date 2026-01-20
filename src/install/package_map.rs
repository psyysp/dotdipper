//! Binary to package name mapping for different operating systems.
//!
//! This module provides mappings from binary names (like `rg`) to their
//! corresponding package names on different package managers (like `ripgrep` on Homebrew).

use anyhow::Result;
use std::collections::HashMap;

/// Maps binary names to OS-specific package names
pub struct PackageMapper {
    mappings: HashMap<String, String>,
    target_os: String,
}

impl PackageMapper {
    /// Create a new package mapper for the given target OS
    pub fn new(target_os: &str) -> Result<Self> {
        let mappings = Self::build_mappings(target_os);

        Ok(Self {
            mappings,
            target_os: target_os.to_string(),
        })
    }

    /// Get the target OS for this mapper
    pub fn target_os(&self) -> &str {
        &self.target_os
    }

    /// Map a binary name to its package name
    pub fn map_binary(&self, binary: &str) -> Option<String> {
        // Try direct mapping first
        if let Some(package) = self.mappings.get(binary) {
            return Some(package.clone());
        }

        // Try normalized name (e.g., "rg" -> "ripgrep")
        let normalized = Self::normalize_binary_name(binary);
        if let Some(package) = self.mappings.get(&normalized) {
            return Some(package.clone());
        }

        // If not found in mappings, assume package name equals binary name
        // This works for many tools like git, curl, wget, etc.
        Some(binary.to_string())
    }

    /// Map multiple binaries to packages
    pub fn map_binaries(&self, binaries: &[String]) -> HashMap<String, String> {
        let mut result = HashMap::new();
        for binary in binaries {
            if let Some(package) = self.map_binary(binary) {
                result.insert(binary.clone(), package);
            }
        }
        result
    }

    /// Normalize a binary name to its canonical form
    fn normalize_binary_name(binary: &str) -> String {
        match binary {
            "rg" => "ripgrep".to_string(),
            "fd" => "fd-find".to_string(),
            "btm" => "bottom".to_string(),
            "hx" => "helix".to_string(),
            "nvim" => "neovim".to_string(),
            "difft" => "difftastic".to_string(),
            _ => binary.to_string(),
        }
    }

    /// Add a custom mapping (for user overrides)
    pub fn add_custom_mapping(&mut self, binary: String, package: String) {
        self.mappings.insert(binary, package);
    }

    /// Build OS-specific package mappings
    fn build_mappings(target_os: &str) -> HashMap<String, String> {
        let mut mappings = HashMap::new();

        match target_os {
            "macos" => Self::build_macos_mappings(&mut mappings),
            "ubuntu" | "debian" => Self::build_debian_mappings(&mut mappings),
            "arch" | "manjaro" | "endeavouros" => Self::build_arch_mappings(&mut mappings),
            "fedora" | "redhat" | "centos" => Self::build_fedora_mappings(&mut mappings),
            _ => Self::build_default_mappings(&mut mappings),
        }

        mappings
    }

    /// Build mappings for macOS (Homebrew)
    fn build_macos_mappings(mappings: &mut HashMap<String, String>) {
        let macos_packages = [
            // Modern CLI tools
            ("fzf", "fzf"),
            ("ripgrep", "ripgrep"),
            ("rg", "ripgrep"),
            ("bat", "bat"),
            ("fd", "fd"),
            ("fd-find", "fd"),
            ("exa", "exa"),
            ("eza", "eza"),
            ("lsd", "lsd"),
            ("zoxide", "zoxide"),
            ("starship", "starship"),
            ("atuin", "atuin"),
            ("dust", "dust"),
            ("duf", "duf"),
            ("procs", "procs"),
            ("bottom", "bottom"),
            ("btm", "bottom"),
            ("htop", "htop"),
            ("btop", "btop"),
            ("glances", "glances"),
            // Text processing
            ("jq", "jq"),
            ("yq", "yq"),
            ("xq", "xq"),
            ("gron", "gron"),
            // Shell tools
            ("tmux", "tmux"),
            ("zellij", "zellij"),
            ("zsh", "zsh"),
            ("bash", "bash"),
            ("fish", "fish"),
            // Editors
            ("nvim", "neovim"),
            ("neovim", "neovim"),
            ("vim", "vim"),
            ("helix", "helix"),
            ("hx", "helix"),
            // Git tools
            ("git", "git"),
            ("gh", "gh"),
            ("hub", "hub"),
            ("delta", "git-delta"),
            ("git-delta", "git-delta"),
            ("diff-so-fancy", "diff-so-fancy"),
            ("lazygit", "lazygit"),
            ("tig", "tig"),
            ("difftastic", "difftastic"),
            ("difft", "difftastic"),
            ("git-lfs", "git-lfs"),
            // Container tools
            ("docker", "docker"),
            ("podman", "podman"),
            ("kubectl", "kubernetes-cli"),
            ("k9s", "k9s"),
            ("helm", "helm"),
            // Cloud tools
            ("terraform", "terraform"),
            ("aws", "awscli"),
            ("gcloud", "google-cloud-sdk"),
            ("az", "azure-cli"),
            ("doctl", "doctl"),
            // Languages
            ("node", "node"),
            ("npm", "node"),
            ("yarn", "yarn"),
            ("pnpm", "pnpm"),
            ("bun", "bun"),
            ("deno", "deno"),
            ("python3", "python@3"),
            ("python", "python@3"),
            ("pip3", "python@3"),
            ("go", "go"),
            ("rust-analyzer", "rust-analyzer"),
            ("rustup", "rustup-init"),
            // Network tools
            ("curl", "curl"),
            ("wget", "wget"),
            ("httpie", "httpie"),
            ("http", "httpie"),
            ("aria2c", "aria2"),
            // File tools
            ("tree", "tree"),
            ("ncdu", "ncdu"),
            ("broot", "broot"),
            ("ranger", "ranger"),
            ("nnn", "nnn"),
            ("lf", "lf"),
            // Misc
            ("pandoc", "pandoc"),
            ("ffmpeg", "ffmpeg"),
            ("imagemagick", "imagemagick"),
            ("convert", "imagemagick"),
            ("gpg", "gnupg"),
            ("age", "age"),
            ("ctags", "universal-ctags"),
            ("shellcheck", "shellcheck"),
            // Terminal emulators
            ("alacritty", "alacritty"),
            ("kitty", "kitty"),
            ("wezterm", "wezterm"),
        ];

        for (binary, package) in macos_packages {
            mappings.insert(binary.to_string(), package.to_string());
        }
    }

    /// Build mappings for Debian/Ubuntu (apt)
    fn build_debian_mappings(mappings: &mut HashMap<String, String>) {
        let debian_packages = [
            // Modern CLI tools
            ("fzf", "fzf"),
            ("ripgrep", "ripgrep"),
            ("rg", "ripgrep"),
            ("bat", "bat"),
            ("fd", "fd-find"),
            ("fd-find", "fd-find"),
            ("exa", "exa"),
            ("zoxide", "zoxide"),
            ("htop", "htop"),
            // Text processing
            ("jq", "jq"),
            // Shell tools
            ("tmux", "tmux"),
            ("zsh", "zsh"),
            ("bash", "bash"),
            ("fish", "fish"),
            // Editors
            ("nvim", "neovim"),
            ("neovim", "neovim"),
            ("vim", "vim"),
            // Git tools
            ("git", "git"),
            ("gh", "gh"),
            ("delta", "git-delta"),
            ("git-delta", "git-delta"),
            ("tig", "tig"),
            ("git-lfs", "git-lfs"),
            // Container tools
            ("docker", "docker.io"),
            ("podman", "podman"),
            ("kubectl", "kubectl"),
            // Languages
            ("node", "nodejs"),
            ("npm", "npm"),
            ("python3", "python3"),
            ("python", "python3"),
            ("pip3", "python3-pip"),
            ("go", "golang"),
            // Network tools
            ("curl", "curl"),
            ("wget", "wget"),
            ("httpie", "httpie"),
            ("http", "httpie"),
            ("aria2c", "aria2"),
            // File tools
            ("tree", "tree"),
            ("ncdu", "ncdu"),
            ("ranger", "ranger"),
            ("nnn", "nnn"),
            // Misc
            ("pandoc", "pandoc"),
            ("ffmpeg", "ffmpeg"),
            ("imagemagick", "imagemagick"),
            ("convert", "imagemagick"),
            ("gpg", "gnupg"),
            ("ctags", "universal-ctags"),
            ("shellcheck", "shellcheck"),
        ];

        for (binary, package) in debian_packages {
            mappings.insert(binary.to_string(), package.to_string());
        }
    }

    /// Build mappings for Arch Linux (pacman)
    fn build_arch_mappings(mappings: &mut HashMap<String, String>) {
        let arch_packages = [
            // Modern CLI tools
            ("fzf", "fzf"),
            ("ripgrep", "ripgrep"),
            ("rg", "ripgrep"),
            ("bat", "bat"),
            ("fd", "fd"),
            ("fd-find", "fd"),
            ("exa", "exa"),
            ("eza", "eza"),
            ("lsd", "lsd"),
            ("zoxide", "zoxide"),
            ("starship", "starship"),
            ("dust", "dust"),
            ("duf", "duf"),
            ("procs", "procs"),
            ("bottom", "bottom"),
            ("btm", "bottom"),
            ("htop", "htop"),
            ("btop", "btop"),
            // Text processing
            ("jq", "jq"),
            ("yq", "yq"),
            // Shell tools
            ("tmux", "tmux"),
            ("zellij", "zellij"),
            ("zsh", "zsh"),
            ("bash", "bash"),
            ("fish", "fish"),
            // Editors
            ("nvim", "neovim"),
            ("neovim", "neovim"),
            ("vim", "vim"),
            ("helix", "helix"),
            ("hx", "helix"),
            // Git tools
            ("git", "git"),
            ("gh", "github-cli"),
            ("delta", "git-delta"),
            ("git-delta", "git-delta"),
            ("lazygit", "lazygit"),
            ("tig", "tig"),
            ("difftastic", "difftastic"),
            ("difft", "difftastic"),
            ("git-lfs", "git-lfs"),
            // Container tools
            ("docker", "docker"),
            ("podman", "podman"),
            ("kubectl", "kubectl"),
            ("k9s", "k9s"),
            ("helm", "helm"),
            // Languages
            ("node", "nodejs"),
            ("npm", "npm"),
            ("yarn", "yarn"),
            ("pnpm", "pnpm"),
            ("bun", "bun"),
            ("deno", "deno"),
            ("python3", "python"),
            ("python", "python"),
            ("pip3", "python-pip"),
            ("go", "go"),
            ("rust-analyzer", "rust-analyzer"),
            ("rustup", "rustup"),
            // Network tools
            ("curl", "curl"),
            ("wget", "wget"),
            ("httpie", "httpie"),
            ("http", "httpie"),
            ("aria2c", "aria2"),
            // File tools
            ("tree", "tree"),
            ("ncdu", "ncdu"),
            ("broot", "broot"),
            ("ranger", "ranger"),
            ("nnn", "nnn"),
            ("lf", "lf"),
            // Misc
            ("pandoc", "pandoc"),
            ("ffmpeg", "ffmpeg"),
            ("imagemagick", "imagemagick"),
            ("convert", "imagemagick"),
            ("gpg", "gnupg"),
            ("age", "age"),
            ("ctags", "ctags"),
            ("shellcheck", "shellcheck"),
            // Terminal emulators
            ("alacritty", "alacritty"),
            ("kitty", "kitty"),
        ];

        for (binary, package) in arch_packages {
            mappings.insert(binary.to_string(), package.to_string());
        }
    }

    /// Build mappings for Fedora/RHEL (dnf)
    fn build_fedora_mappings(mappings: &mut HashMap<String, String>) {
        let fedora_packages = [
            // Modern CLI tools
            ("fzf", "fzf"),
            ("ripgrep", "ripgrep"),
            ("rg", "ripgrep"),
            ("bat", "bat"),
            ("fd", "fd-find"),
            ("fd-find", "fd-find"),
            ("htop", "htop"),
            // Text processing
            ("jq", "jq"),
            // Shell tools
            ("tmux", "tmux"),
            ("zsh", "zsh"),
            ("bash", "bash"),
            ("fish", "fish"),
            // Editors
            ("nvim", "neovim"),
            ("neovim", "neovim"),
            ("vim", "vim-enhanced"),
            // Git tools
            ("git", "git"),
            ("gh", "gh"),
            ("delta", "git-delta"),
            ("tig", "tig"),
            ("git-lfs", "git-lfs"),
            // Container tools
            ("docker", "docker-ce"),
            ("podman", "podman"),
            ("kubectl", "kubernetes-client"),
            // Languages
            ("node", "nodejs"),
            ("npm", "npm"),
            ("python3", "python3"),
            ("python", "python3"),
            ("pip3", "python3-pip"),
            ("go", "golang"),
            // Network tools
            ("curl", "curl"),
            ("wget", "wget"),
            ("httpie", "httpie"),
            ("aria2c", "aria2"),
            // File tools
            ("tree", "tree"),
            ("ncdu", "ncdu"),
            ("ranger", "ranger"),
            // Misc
            ("pandoc", "pandoc"),
            ("ffmpeg", "ffmpeg"),
            ("imagemagick", "ImageMagick"),
            ("convert", "ImageMagick"),
            ("gpg", "gnupg2"),
            ("ctags", "ctags"),
            ("shellcheck", "ShellCheck"),
        ];

        for (binary, package) in fedora_packages {
            mappings.insert(binary.to_string(), package.to_string());
        }
    }

    /// Build default mappings (generic Linux)
    fn build_default_mappings(mappings: &mut HashMap<String, String>) {
        // Use Debian-like mappings as default
        Self::build_debian_mappings(mappings);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_macos_mapping() {
        let mapper = PackageMapper::new("macos").unwrap();

        assert_eq!(mapper.map_binary("fzf"), Some("fzf".to_string()));
        assert_eq!(mapper.map_binary("rg"), Some("ripgrep".to_string()));
        assert_eq!(mapper.map_binary("ripgrep"), Some("ripgrep".to_string()));
        assert_eq!(mapper.map_binary("nvim"), Some("neovim".to_string()));
        assert_eq!(mapper.map_binary("kubectl"), Some("kubernetes-cli".to_string()));
    }

    #[test]
    fn test_debian_mapping() {
        let mapper = PackageMapper::new("ubuntu").unwrap();

        assert_eq!(mapper.map_binary("fd"), Some("fd-find".to_string()));
        assert_eq!(mapper.map_binary("docker"), Some("docker.io".to_string()));
        assert_eq!(mapper.map_binary("node"), Some("nodejs".to_string()));
    }

    #[test]
    fn test_arch_mapping() {
        let mapper = PackageMapper::new("arch").unwrap();

        assert_eq!(mapper.map_binary("fd"), Some("fd".to_string()));
        assert_eq!(mapper.map_binary("gh"), Some("github-cli".to_string()));
        assert_eq!(mapper.map_binary("python3"), Some("python".to_string()));
    }

    #[test]
    fn test_custom_mapping() {
        let mut mapper = PackageMapper::new("macos").unwrap();
        mapper.add_custom_mapping("my-tool".to_string(), "my-custom-package".to_string());

        assert_eq!(mapper.map_binary("my-tool"), Some("my-custom-package".to_string()));
    }

    #[test]
    fn test_unknown_binary() {
        let mapper = PackageMapper::new("macos").unwrap();

        // Unknown binaries should return the binary name as package name
        assert_eq!(mapper.map_binary("unknown-tool"), Some("unknown-tool".to_string()));
    }

    #[test]
    fn test_normalize_binary_name() {
        assert_eq!(PackageMapper::normalize_binary_name("rg"), "ripgrep");
        assert_eq!(PackageMapper::normalize_binary_name("fd"), "fd-find");
        assert_eq!(PackageMapper::normalize_binary_name("btm"), "bottom");
        assert_eq!(PackageMapper::normalize_binary_name("nvim"), "neovim");
    }
}

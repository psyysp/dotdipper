//! Vim/Neovim configuration analyzer for detecting binary dependencies.
//!
//! Analyzes vim/neovim configuration files (.vimrc, init.vim, init.lua) to find
//! references to external binaries and tools that plugins or configurations depend on.

use anyhow::Result;
use regex::Regex;
use std::collections::HashSet;

/// Analyze vim/neovim configuration content for binary dependencies
pub fn analyze(content: &str) -> Result<HashSet<String>> {
    let mut binaries = HashSet::new();

    // Pattern 1: External commands in vim config
    // :!command, :r !command, system('command')
    let system_call = Regex::new(r#"system\s*\(\s*['"]([a-zA-Z0-9_-]+)"#)?;
    for cap in system_call.captures_iter(content) {
        if let Some(binary) = cap.get(1) {
            if !is_vim_command(binary.as_str()) {
                binaries.insert(binary.as_str().to_string());
            }
        }
    }

    // Pattern 2: executable() checks
    let executable_check = Regex::new(r#"executable\s*\(\s*['"]([a-zA-Z0-9_-]+)['"]"#)?;
    for cap in executable_check.captures_iter(content) {
        if let Some(binary) = cap.get(1) {
            binaries.insert(binary.as_str().to_string());
        }
    }

    // Pattern 3: Plugin dependencies - fzf.vim
    if content.contains("fzf") || content.contains("FZF") {
        binaries.insert("fzf".to_string());
    }

    // Pattern 4: ripgrep references (used by many search plugins)
    if content.contains("ripgrep") || content.contains("'rg'") || content.contains("\"rg\"") {
        binaries.insert("rg".to_string());
    }

    // Pattern 5: bat (used for syntax highlighting in previews)
    if content.contains("'bat'") || content.contains("\"bat\"") {
        binaries.insert("bat".to_string());
    }

    // Pattern 6: fd (fast file finder)
    if content.contains("'fd'") || content.contains("\"fd\"") {
        binaries.insert("fd".to_string());
    }

    // Pattern 7: git (required for many plugins and fugitive)
    if content.contains("fugitive")
        || content.contains("gitgutter")
        || content.contains("gitsigns")
    {
        binaries.insert("git".to_string());
    }

    // Pattern 8: Language servers
    analyze_lsp_servers(content, &mut binaries);

    // Pattern 9: Formatters and linters
    analyze_formatters_linters(content, &mut binaries);

    // Pattern 10: Terminal/shell integrations
    if content.contains("terminal")
        || content.contains("floaterm")
        || content.contains("toggleterm")
    {
        // These typically need a shell
        binaries.insert("zsh".to_string());
    }

    // Pattern 11: Tree-sitter (needs node for some parsers)
    if content.contains("treesitter") || content.contains("tree-sitter") {
        binaries.insert("node".to_string());
        binaries.insert("git".to_string());
    }

    // Pattern 12: ctags/universal-ctags
    if content.contains("tagbar") || content.contains("gutentags") || content.contains("ctags") {
        binaries.insert("ctags".to_string());
    }

    // Pattern 13: External grep programs
    if content.contains("grepprg") {
        let grepprg = Regex::new(r#"grepprg\s*=\s*['"]?([a-zA-Z0-9_-]+)"#)?;
        for cap in grepprg.captures_iter(content) {
            if let Some(binary) = cap.get(1) {
                binaries.insert(binary.as_str().to_string());
            }
        }
    }

    // Pattern 14: External diff programs
    if content.contains("diffopt") || content.contains("DiffOrig") {
        binaries.insert("diff".to_string());
    }

    // Filter out vim commands
    binaries.retain(|b| !is_vim_command(b));

    Ok(binaries)
}

/// Analyze content for LSP server dependencies
fn analyze_lsp_servers(content: &str, binaries: &mut HashSet<String>) {
    let lsp_servers = [
        ("rust_analyzer", "rust-analyzer"),
        ("rust-analyzer", "rust-analyzer"),
        ("tsserver", "typescript-language-server"),
        ("typescript", "typescript-language-server"),
        ("pylsp", "python-lsp-server"),
        ("pyright", "pyright"),
        ("gopls", "gopls"),
        ("clangd", "clangd"),
        ("lua_ls", "lua-language-server"),
        ("sumneko_lua", "lua-language-server"),
        ("bashls", "bash-language-server"),
        ("jsonls", "vscode-json-languageserver"),
        ("yamlls", "yaml-language-server"),
        ("cssls", "vscode-css-languageserver"),
        ("html", "vscode-html-languageserver"),
        ("tailwindcss", "tailwindcss-language-server"),
        ("solargraph", "solargraph"),
        ("intelephense", "intelephense"),
    ];

    for (pattern, binary) in lsp_servers {
        if content.contains(pattern) {
            binaries.insert(binary.to_string());
        }
    }

    // Generic LSP/nvim-lspconfig detection
    if content.contains("lsp") || content.contains("LanguageServer") || content.contains("lspconfig")
    {
        // Check for specific language mentions
        if content.contains("rust") {
            binaries.insert("rust-analyzer".to_string());
        }
        if content.contains("typescript") || content.contains("javascript") {
            binaries.insert("typescript-language-server".to_string());
        }
        if content.contains("python") {
            binaries.insert("pyright".to_string());
        }
        if content.contains("golang") || content.to_lowercase().contains("go.") {
            binaries.insert("gopls".to_string());
        }
    }
}

/// Analyze content for formatter and linter dependencies
fn analyze_formatters_linters(content: &str, binaries: &mut HashSet<String>) {
    let tools = [
        // Formatters
        ("prettier", "prettier"),
        ("black", "black"),
        ("rustfmt", "rustfmt"),
        ("gofmt", "gofmt"),
        ("stylua", "stylua"),
        ("shfmt", "shfmt"),
        ("clang-format", "clang-format"),
        ("autopep8", "autopep8"),
        ("isort", "isort"),
        // Linters
        ("eslint", "eslint"),
        ("pylint", "pylint"),
        ("flake8", "flake8"),
        ("mypy", "mypy"),
        ("rubocop", "rubocop"),
        ("shellcheck", "shellcheck"),
        ("hadolint", "hadolint"),
        ("golangci-lint", "golangci-lint"),
        ("clippy", "cargo-clippy"),
        ("luacheck", "luacheck"),
    ];

    for (pattern, binary) in tools {
        if content.contains(pattern) {
            binaries.insert(binary.to_string());
        }
    }

    // null-ls/none-ls commonly used formatters
    if content.contains("null-ls") || content.contains("none-ls") {
        // These plugins aggregate many tools
        if content.contains("formatting") {
            binaries.insert("prettier".to_string());
        }
        if content.contains("diagnostics") {
            binaries.insert("eslint".to_string());
        }
    }
}

/// Check if a string is a vim command (not an external binary)
fn is_vim_command(cmd: &str) -> bool {
    matches!(
        cmd,
        "set"
            | "let"
            | "if"
            | "else"
            | "elseif"
            | "endif"
            | "for"
            | "endfor"
            | "while"
            | "endwhile"
            | "function"
            | "endfunction"
            | "call"
            | "return"
            | "try"
            | "catch"
            | "finally"
            | "endtry"
            | "throw"
            | "autocmd"
            | "augroup"
            | "map"
            | "nmap"
            | "vmap"
            | "imap"
            | "cmap"
            | "omap"
            | "xmap"
            | "smap"
            | "lmap"
            | "nnoremap"
            | "vnoremap"
            | "inoremap"
            | "cnoremap"
            | "onoremap"
            | "xnoremap"
            | "snoremap"
            | "lnoremap"
            | "noremap"
            | "command"
            | "highlight"
            | "hi"
            | "syntax"
            | "syn"
            | "colorscheme"
            | "source"
            | "runtime"
            | "filetype"
            | "setlocal"
            | "setglobal"
            | "echo"
            | "echom"
            | "echomsg"
            | "echoerr"
            | "execute"
            | "normal"
            | "silent"
            | "unlet"
            | "lua"
            | "require"
            | "vim"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fzf_detection() {
        let content = r#"
Plug 'junegunn/fzf', { 'do': { -> fzf#install() } }
Plug 'junegunn/fzf.vim'
"#;
        let binaries = analyze(content).unwrap();
        assert!(binaries.contains("fzf"));
    }

    #[test]
    fn test_executable_check() {
        let content = r#"
if executable('rg')
    set grepprg=rg\ --vimgrep
endif
"#;
        let binaries = analyze(content).unwrap();
        assert!(binaries.contains("rg"));
    }

    #[test]
    fn test_lsp_detection() {
        let content = r#"
require'lspconfig'.rust_analyzer.setup{}
require'lspconfig'.tsserver.setup{}
"#;
        let binaries = analyze(content).unwrap();
        assert!(binaries.contains("rust-analyzer"));
        assert!(binaries.contains("typescript-language-server"));
    }

    #[test]
    fn test_formatter_detection() {
        let content = r#"
let g:neoformat_enabled_python = ['black', 'isort']
"#;
        let binaries = analyze(content).unwrap();
        assert!(binaries.contains("black"));
        assert!(binaries.contains("isort"));
    }

    #[test]
    fn test_treesitter_detection() {
        let content = r#"
require'nvim-treesitter.configs'.setup {
    ensure_installed = { "lua", "rust", "python" },
}
"#;
        let binaries = analyze(content).unwrap();
        assert!(binaries.contains("node"));
        assert!(binaries.contains("git"));
    }
}

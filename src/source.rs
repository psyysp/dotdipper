//! Where a dotfiles repository comes from.
//!
//! `dotdipper init <source>` takes whatever a user would naturally paste: a
//! GitHub account, an `owner/repo` pair, or a full HTTPS or SSH URL from any
//! git host. This module turns that into one git URL and refuses everything
//! git would accept but a dotfiles manager has no business fetching.
//!
//! The refusals are the point. The source is about to be handed to
//! `git clone`, and git's URL grammar is far wider than "a repository on a
//! server": `ext::` runs an arbitrary command, `file://` and bare paths read
//! the local disk, a leading `-` is parsed as an option (`--upload-pack=...`
//! runs a command), and `http://` lets anyone on the network substitute the
//! files about to be written into `$HOME`. None of those is a plausible
//! dotfiles source, so none is accepted.

use anyhow::{bail, Result};

/// Repository name assumed when a source names only an account, matching
/// the convention most dotfiles repositories follow.
pub const DEFAULT_REPO_NAME: &str = "dotfiles";

/// The transport a source implies. Unlike `pull --https`, the user does not
/// choose it separately: a URL already says how it is reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceTransport {
    /// Anonymous HTTPS. Works with no account and no key, which is what a
    /// fresh machine has.
    Https,
    /// SSH. Requires a key, and is what a private repository needs.
    Ssh,
}

/// A validated repository source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoSource {
    /// The URL handed to git.
    pub url: String,
    pub transport: SourceTransport,
    /// Host, account and repository, for messages. Not used to reach the
    /// remote: `url` is authoritative.
    pub host: String,
    pub owner: String,
    pub repo: String,
}

impl RepoSource {
    /// A short `host/owner/repo` label for messages.
    pub fn display(&self) -> String {
        format!("{}/{}/{}", self.host, self.owner, self.repo)
    }
}

/// Parses a user-supplied source.
///
/// Accepted forms:
/// - `owner` → `https://github.com/owner/dotfiles.git`
/// - `owner/repo` → `https://github.com/owner/repo.git`
/// - `host.tld/owner/repo` → `https://host.tld/owner/repo.git`
/// - `https://host/owner/repo[.git]`
/// - `ssh://[user@]host[:port]/owner/repo[.git]`
/// - `git@host:owner/repo[.git]`
pub fn parse(input: &str) -> Result<RepoSource> {
    let s = input.trim();
    if s.is_empty() {
        bail!("Repository source is empty");
    }
    if s.starts_with('-') {
        // git parses a leading dash as an option, and `--upload-pack=` or
        // `--config=` from a pasted string would run a command.
        bail!("Repository source '{s}' looks like an option, not a repository");
    }
    if s.chars().any(|c| c.is_control() || c.is_whitespace()) {
        bail!("Repository source contains whitespace or control characters");
    }

    if let Some(rest) = s.strip_prefix("https://") {
        return parse_hierarchical(rest, SourceTransport::Https, s);
    }
    if let Some(rest) = s.strip_prefix("ssh://") {
        return parse_hierarchical(rest, SourceTransport::Ssh, s);
    }
    if let Some(scheme) = s.split_once("://").map(|(scheme, _)| scheme) {
        // http:// lets anyone on the path rewrite what lands in $HOME; file://
        // reads local disk; git:// is unauthenticated plaintext.
        bail!(
            "Unsupported scheme '{scheme}://'. Use an https:// or ssh:// URL, \
             a git@host:owner/repo address, or owner/repo for GitHub"
        );
    }
    if s.contains("::") {
        // git's transport-helper syntax (`ext::sh -c ...`) runs a command.
        bail!("Repository source '{s}' uses git transport-helper syntax, which is not allowed");
    }
    if let Some((userhost, path)) = scp_like(s) {
        let host = userhost.rsplit('@').next().unwrap_or(userhost);
        let (owner, repo) = owner_repo(path, s)?;
        validate_host(host, s)?;
        return Ok(RepoSource {
            url: s.to_string(),
            transport: SourceTransport::Ssh,
            host: host.to_string(),
            owner,
            repo,
        });
    }
    if s.starts_with('/') || s.starts_with('.') || s.starts_with('~') {
        bail!("Local paths are not accepted as a repository source: '{s}'");
    }

    // Shorthand: owner, owner/repo, or host/owner/repo.
    let parts: Vec<&str> = s.trim_end_matches('/').split('/').collect();
    let (host, owner, repo) = match parts.as_slice() {
        [owner] => ("github.com", *owner, DEFAULT_REPO_NAME),
        [owner, repo] => ("github.com", *owner, *repo),
        [host, owner, repo] if host.contains('.') => (*host, *owner, *repo),
        _ => bail!(
            "Could not read '{s}' as a repository. Use owner, owner/repo, \
             or a full https:// or git@ URL"
        ),
    };
    validate_host(host, s)?;
    let repo = repo.strip_suffix(".git").unwrap_or(repo);
    validate_segment(owner, "account", s)?;
    validate_segment(repo, "repository", s)?;
    Ok(RepoSource {
        url: format!("https://{host}/{owner}/{repo}.git"),
        transport: SourceTransport::Https,
        host: host.to_string(),
        owner: owner.to_string(),
        repo: repo.to_string(),
    })
}

/// Re-validates a URL read back from config, which a user can edit by hand.
/// Only URLs that [`parse`] would itself accept are used.
pub fn validate_url(url: &str) -> Result<RepoSource> {
    parse(url)
}

/// `https://` and `ssh://` share a shape: authority, then a path whose
/// last two segments are owner and repository.
fn parse_hierarchical(
    rest: &str,
    transport: SourceTransport,
    original: &str,
) -> Result<RepoSource> {
    let (authority, path) = rest
        .split_once('/')
        .ok_or_else(|| anyhow::anyhow!("'{original}' has no repository path"))?;
    if transport == SourceTransport::Https && authority.contains('@') {
        // A credential in the URL would be written into config.toml and
        // .git/config in plain text.
        bail!("Do not embed credentials in the URL; use a git credential helper or SSH");
    }
    let host_port = authority.rsplit('@').next().unwrap_or(authority);
    let host = host_port.split(':').next().unwrap_or(host_port);
    validate_host(host, original)?;
    let (owner, repo) = owner_repo(path, original)?;
    Ok(RepoSource {
        url: original.to_string(),
        transport,
        host: host.to_string(),
        owner,
        repo,
    })
}

/// Splits `user@host:path` (git's scp-like syntax). A colon before any
/// slash is what distinguishes it from a relative path.
fn scp_like(s: &str) -> Option<(&str, &str)> {
    let colon = s.find(':')?;
    let slash = s.find('/').unwrap_or(usize::MAX);
    if colon < slash && s[..colon].contains('@') {
        Some((&s[..colon], &s[colon + 1..]))
    } else {
        None
    }
}

/// Takes the last two path segments as owner and repository. GitLab-style
/// subgroups (`group/sub/repo`) keep working because only the tail matters
/// for display; the URL itself is passed to git unchanged.
fn owner_repo(path: &str, original: &str) -> Result<(String, String)> {
    let segments: Vec<&str> = path
        .trim_end_matches('/')
        .split('/')
        .filter(|seg| !seg.is_empty())
        .collect();
    if segments.len() < 2 {
        bail!("'{original}' must name an account and a repository");
    }
    for seg in &segments {
        if *seg == ".." || *seg == "." {
            bail!("'{original}' contains a relative path segment");
        }
    }
    let repo = segments[segments.len() - 1];
    let repo = repo.strip_suffix(".git").unwrap_or(repo);
    let owner = segments[segments.len() - 2];
    validate_segment(owner, "account", original)?;
    validate_segment(repo, "repository", original)?;
    Ok((owner.to_string(), repo.to_string()))
}

fn validate_host(host: &str, original: &str) -> Result<()> {
    let ok = !host.is_empty()
        && !host.starts_with('-')
        && !host.starts_with('.')
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    if !ok {
        bail!("'{original}' does not name a valid host");
    }
    Ok(())
}

fn validate_segment(seg: &str, what: &str, original: &str) -> Result<()> {
    let ok = !seg.is_empty()
        && !seg.starts_with('-')
        && !seg.starts_with('.')
        && seg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if !ok {
        bail!("'{original}' does not contain a valid {what} name");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_account_alone_means_its_dotfiles_repository_on_github() {
        let s = parse("octocat").unwrap();
        assert_eq!(s.url, "https://github.com/octocat/dotfiles.git");
        assert_eq!(s.transport, SourceTransport::Https);
    }

    #[test]
    fn owner_and_repo_default_to_anonymous_https_on_github() {
        let s = parse("octocat/dots").unwrap();
        assert_eq!(s.url, "https://github.com/octocat/dots.git");
        assert_eq!(s.display(), "github.com/octocat/dots");
    }

    #[test]
    fn a_host_prefix_selects_another_forge() {
        let s = parse("gitlab.com/someone/dotfiles.git").unwrap();
        assert_eq!(s.url, "https://gitlab.com/someone/dotfiles.git");
    }

    #[test]
    fn full_urls_are_passed_through_unchanged() {
        for (url, transport) in [
            ("https://github.com/octocat/dots", SourceTransport::Https),
            (
                "https://github.com/octocat/dots.git",
                SourceTransport::Https,
            ),
            ("git@github.com:octocat/dots.git", SourceTransport::Ssh),
            (
                "ssh://git@example.com:2222/team/dots.git",
                SourceTransport::Ssh,
            ),
            (
                "https://gitlab.com/group/sub/dots.git",
                SourceTransport::Https,
            ),
        ] {
            let s = parse(url).unwrap_or_else(|e| panic!("{url}: {e}"));
            assert_eq!(s.url, url);
            assert_eq!(s.transport, transport);
            assert_eq!(s.repo, "dots");
        }
    }

    #[test]
    fn sources_that_would_run_a_command_or_read_local_disk_are_refused() {
        for bad in [
            "--upload-pack=touch /tmp/pwned",
            "-ufoo",
            "ext::sh -c touch% /tmp/pwned",
            "ext::sh",
            "file:///etc",
            "/etc",
            "./dots",
            "~/dots",
            "http://github.com/octocat/dots",
            "git://github.com/octocat/dots",
            "https://user:token@github.com/octocat/dots",
            "https://github.com/octocat",
            "https://github.com/../etc/dots",
            "octocat/dots repo",
            "a/b/c/d",
            "octocat/-dots",
            "",
        ] {
            assert!(parse(bad).is_err(), "accepted {bad:?}");
        }
    }
}

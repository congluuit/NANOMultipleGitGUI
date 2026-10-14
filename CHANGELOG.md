# Changelog

All notable changes to this project are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses [Semantic Versioning](https://semver.org/). Entries are derived from [Conventional Commits](https://www.conventionalcommits.org/).

## [Unreleased]

### Added

- Monorepo scaffold: Tauri 2 desktop app (React 19 + TypeScript + Vite) and a Cargo workspace of crates.
- Design tokens for Light / Dark / System themes (warm stone + ember palette) with automated WCAG AA contrast tests and a palette preview page.
- Main window layout shell (account pill, repo tabs, toolbar, sidebar, commit panel, status bar) with sample data.
- Vietnamese and English UI (i18next).
- Typed IPC bindings generated with tauri-specta.
- SQLite storage with schema migrations; settings persisted in the database.
- Logging to rotating files with redaction of tokens, `Authorization` headers and URL credentials.
- CI (lint, typecheck, tests on Windows/macOS/Linux) and a release workflow producing `.exe`/`.msi`, universal `.dmg` (ad-hoc signed) and Linux packages with SHA-256 checksums; optional SignPath signing.
- Install guides for macOS and Windows, contributor and security documentation, ADR-001 to ADR-004.
- Prototypes: `nanomgg-credential` helper with per-account HTTPS credentials (incl. Git LFS) and a virtualized canvas commit graph.
- Open existing repositories (from any subfolder) and create new ones with a chosen initial branch; a folder that is not a repository offers to initialize it.
- Repository tabs that are restored on the next launch, including a recovery screen when a repository folder has moved or disappeared.
- Recent repositories on the welcome screen and in the new-tab menu.
- Startup check for git ≥ 2.35 (blocking banner) and git-lfs ≥ 3.0 (dismissible warning).
- Sidebar with real local branches (upstream, ahead/behind, gone upstream), remotes, tags and stashes; status bar with remote, transport and branch.
- Automatic refresh of branches and the commit graph when a repository changes on disk (e.g. commits made in a terminal).
- `nanomgg <path>` opens a repository from the command line.
- Commit selection in the graph (mouse and keyboard) with a commit details panel: message, author/committer, parents and changed files with line counts.
- Commit search by message, author or SHA (Ctrl/Cmd+F) with match highlighting and navigation.
- Clicking a branch, remote branch or tag in the sidebar jumps to its commit.
- Incremental history refresh: new commits are read without reloading the whole history, keeping the selection and scroll position.
- Working tree panel: staged, unstaged, untracked and conflicted files with size and "Large" badges, multi-select, stage/unstage/discard per file or all, and a "WIP" row above the history.
- Diff viewer: hunk view where single hunks or individual lines can be staged, unstaged or discarded, plus inline and side-by-side views with Monaco (themed from the design tokens); file diffs of any commit.
- Commit and amend with summary/description, character counter, sign-off, Ctrl+Enter, and the identity git will record.
- Right-click menu on commits: checkout (detached), create branch or tag here, cherry-pick, revert, reset soft/mixed/hard, amend the last commit, copy SHA or subject, and actions for the branches and tags on that commit. The same branch/tag actions on sidebar rows.
- Branches: create, checkout (including remote branches as tracking branches), rename, delete (with a forced delete when not fully merged) and merge (fast-forward, no-ff, squash, ff-only).
- Banner for an in-progress merge / rebase / cherry-pick / revert with conflict count, Abort and Continue.
- Fetch, pull (merge, rebase, ff-only) and push (sets upstream for new branches, force-with-lease) with live progress and a cancel button in the status bar.
- Stash create (including untracked, keep staged), apply, pop, drop and file list.
- Git LFS panel: git-lfs version, tracked patterns, track/untrack, install the LFS hooks for the repository only.
- Large File Guard before committing (GitHub warns over 50 MB and blocks over 100 MB, GitLab warns over 50 MB) with one-click tracking in LFS.
- Multiple GitHub/GitLab accounts used side by side, including several accounts on the same host: sign-in with GitHub device flow, GitLab browser login (PKCE) or personal access tokens, GitHub Enterprise and self-hosted GitLab instances; tokens stored in the OS keychain only, with token health (missing scopes, expiry) and automatic GitLab token refresh.
- Each repository (and each remote) is bound to an account: rules by host/owner, folder or URL pick it automatically; the account pill, colored window border and tabs show which account is in use; "Use for this repo" (optionally locked) or "Only for this action"; `Ctrl/Cmd+Alt+1..9` switch accounts.
- Every git command — commit, fetch, pull, push, Git LFS, the integrated terminal — runs as the bound account through the bundled `nanomgg-credential` helper, without touching global git config; the account's identity is written to the repository's `.git/config` (can be turned off).
- SSH keys per account: generate (ed25519) or import, stored with owner-only permissions, upload to GitHub/GitLab, test which user a key authenticates as; no host aliases needed.
- Import of existing `~/.gitconfig` (including `includeIf`), `~/.ssh/config`, GitHub CLI and glab configuration on first run.
- Clone dialog listing repositories of every account, account guess from a pasted URL, destination template per account, HTTPS/SSH, shallow, submodules and Git LFS download now / later / partially.
- Commit Guard (identity email not matching the account or rule, unverified email, missing signing key) and Push Guard (commits with another author with a one-click fix, missing push permission, heavy files in unpushed commits with conversion to LFS), always with a backup ref and Undo.
- Git LFS uploads and downloads with an estimate, per-file progress, speed and ETA in a transfer queue, cancel/resume and plain-language errors (permission, quota, size limits).
- Integrated terminal (Git Bash / PowerShell / login shell) that uses the repository's account.
- Command palette (`Ctrl/Cmd+K`) with fuzzy search, including "Switch account", "Commit as", "Push as" and "Clone as".

### Changed

- The app icon now uses the project's artwork (`apps/desktop/src-tauri/icons/app-icon.png` is the source for all sizes).
- The file watcher ignores changes to files matched by `.gitignore` (build output, `node_modules`...).

### Fixed

- Two history refreshes arriving together (file watcher and UI) no longer reload the whole history and reset the graph version.
- Cancelling fetch, pull, push or an LFS transfer on Windows now also stops processes started by hooks (Windows Job Object).
- An HTTP 403 from the server is reported as missing permission instead of a connection problem.

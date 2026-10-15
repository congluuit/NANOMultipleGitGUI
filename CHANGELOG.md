# Changelog

All notable changes to this project are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses [Semantic Versioning](https://semver.org/). Entries are derived from [Conventional Commits](https://www.conventionalcommits.org/).

## [Unreleased]

## [1.0.1] - 2026-10-16

### Added

- Publish a new project to GitHub / GitLab in one step: the Push button becomes **Publish** while a repository has no remote. Choose the account, where to create it (your account or an organization / group), the name and Private / Public — the app creates the repository, adds it as `origin` with that account and pushes your branch.
- Already created the repository on the web? Add it by URL (toolbar Push menu, the Remote section of the left panel, or the command palette).
- A "New project" checklist under the toolbar walks a freshly created repository through first commit → publish → push.

### Changed

- The first push of a branch that is not on the server yet is now labelled **Publish branch**.

## [1.0.0] - 2026-10-15

First public release.

### Added

- Double-click a branch in the left panel to switch to it safely: uncommitted changes can be stashed automatically and are restored when you come back to that branch.
- Custom themes: two High Contrast presets and a theme editor (export / import as JSON).
- Export / import account settings between computers through a password-encrypted file (sign-in tokens optional).
- AI commit message suggestions with your own API key (OpenAI, Anthropic, Gemini, OpenAI-compatible) or a local model through Ollama.
- "My notes" tab in My Work: colored tags, priority, pin, due date, export / import.
- Full backup and restore of all app data through a password-encrypted file.
- Full Guide in Settings: every feature explained in plain language (English / Vietnamese).
- Monorepo scaffold: Tauri 2 desktop app (React 19 + TypeScript + Vite) and a Cargo workspace of crates.
- Design tokens for Light / Dark / System themes (warm stone + ember palette) with automated WCAG AA contrast tests and a palette preview page.
- Main window: account pill, repository tabs, toolbar, sidebar, commit panel and status bar.
- Vietnamese and English UI (i18next).
- Typed IPC bindings generated with tauri-specta.
- SQLite storage with schema migrations; settings persisted in the database.
- Logging to rotating files with redaction of tokens, `Authorization` headers and URL credentials.
- CI (lint, typecheck, tests on Windows/macOS/Linux) and a release workflow producing `.exe`/`.msi`, universal `.dmg` (ad-hoc signed) and Linux packages with SHA-256 checksums; optional SignPath signing.
- Install guides for macOS and Windows, contributor and security documentation, ADR-001 to ADR-004.
- `nanomgg-credential` helper giving git and Git LFS per-account HTTPS credentials, and a virtualized canvas commit graph that stays smooth with 100k+ commits.
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
- Rebase onto a branch or commit, and interactive rebase with drag-to-reorder (or Alt+↑/↓), pick / reword / squash / fixup / drop / edit, a preview of the result and a warning for commits that are already pushed; a backup ref is always made first.
- Undo and Redo (toolbar, `Ctrl/Cmd+Z`, `Ctrl/Cmd+Shift+Z`) for commit, amend, reset, checkout, merge, rebase, cherry-pick, revert, pull and branch create / delete / rename, without ever overwriting uncommitted work.
- Drag a branch onto another branch (sidebar or graph labels) to fast-forward, merge or rebase.
- 3-way conflict editor with per-block choices, including binary and Git LFS files with image previews and clear "ours / theirs" labels (also during a rebase, where git swaps them).
- Image diff (side by side, swipe, onion skin), also for Git LFS images.
- File history (following renames), blame with jump-to-commit and blame at the parent commit, and viewing a file at any commit.
- Submodules and worktrees in the sidebar: status, add, update, sync, remove, open as a tab; network access uses the account of each submodule's host.
- Git LFS manager: file list with download state, file locking (lock, unlock, force unlock, lock badges in the file list), lockable patterns, prune with a preview and integrity check.
- Commit signing with GPG or SSH per identity, a "Test signing" button, and the signature status (verified, untrusted key, bad…) in commit details.
- Accounts already signed in with GitHub CLI (`gh`, including several accounts per host) or GitLab CLI (`glab`) are found on start and can be added with one click: the token is read from the CLI only when you click Add, checked like a personal access token and stored in the OS keychain; the gh/glab configuration is never changed. Missing scopes come with the exact command to fix them.
- Getting-started guide on first launch: welcome (language, theme), a tool check for git, Git LFS, OpenSSH, gh, glab and WebView2 with links to the official downloads and an "Install it for me" button (winget / Homebrew / Xcode tools), accounts, and a summary with Open / Clone. It can be run again from Settings.
- Settings → Storage: size and location of the log, cache and app data folders, open / move (with progress) / back to default, clear cache and delete old logs; app data moves on the next start. Warning when the drive holding app data has less than 1 GB free.
- Check for updates from published GitHub releases (on start at most once a day and from Settings or the command palette) with release notes, "Update & restart", "Later" and "Skip this version", plus a beta channel. Builds without a signing key or repository say that updates are not enabled.
- Resizable sidebar and right panel (drag, keyboard, double-click to reset, remembered), collapsible to an icon rail (`Ctrl/Cmd+B`, `Ctrl/Cmd+Alt+B`), and a "⋯" menu for toolbar buttons that do not fit.
- Pull requests (GitHub) and merge requests (GitLab) per account: list with filters in the sidebar, details with description, timeline, commits and changed files, comments (also on lines), reviews, merge (merge / squash / rebase, delete the source branch), close / reopen, create from a branch (offers to push first), and check out a PR/MR branch — also from forks.
- Issues per repository: list, view, comment, create, close / reopen, labels and assignees, create a branch from an issue; `#` suggestions in the commit message and clickable `#123` / `!12` in commit details.
- "My work" panel gathering, across all signed-in accounts, the PRs/MRs waiting for your review, your own PRs/MRs and the issues assigned to you.
- CI status (GitHub Actions checks and statuses, GitLab pipelines) in commit details with each job and a link to its log, in the status bar for HEAD and on pull requests, with a note that CI runs after a push and does not tell whether the push worked. **Off by default**: Settings → "Show CI" turns it on; while off nothing about CI is shown or queried.
- Commit graph styled for quick reading: a "Branch / Tag" column on the left with one lane-colored pill per commit (local and remote branch merged into one pill with laptop / cloud icons, ✓ on the checked-out branch, "+N" for more refs), a connector line to the node, a faint lane-colored row band, and author avatars on commit nodes (GitHub noreply addresses, your accounts, Gravatar; initials when unavailable or when Settings → "Author avatars" is off).
- Branch context menu (sidebar and graph labels): pull (fast-forward if possible, also for a branch that is not checked out), push any branch, set / remove upstream, checkout, create worktree, create branch here, reset, edit commit message, drop commit, revert, merge / rebase, push and start a pull request, rename, delete the branch, its remote branch or both, copy branch name / SHA / web links, create tag or annotated tag. Commit menu gains edit message, drop commit, copy web link and annotated tag.
- The Push and Pull buttons show how many commits wait to be pushed (↑N) or pulled (↓N); pushing a branch that misses commits from its upstream offers "Pull, then push".
- The remotes of the open repository are fetched in the background (on open, every 5 minutes and when the window regains focus) for remotes bound to an account, so the ↓N count stays current.
- A successful push shows a green message with the number of commits, the target branch and the account used.
- Fork & clone (fork to your account or an organization, clone the fork, add `upstream`) and "Fork this repository" for an open repository.
- Every provider action shows the account it runs as and can use another account for that action only.
- Graph columns can be resized (double-click a handle to restore), the message column is the widest by default, and hovering a cut-off message, author or date shows the full text.
- `.env.example` documenting the optional build-time settings in English and Vietnamese.

### Changed

- The bundle identifier is now `io.github.congluuit.nanomultiplegitgui` (app folders follow it). On the first start, data from earlier test builds (accounts, settings, SSH keys, storage locations) is copied over automatically; sign-in tokens stay in the OS keychain.
- The app icon now uses the project's artwork (`apps/desktop/src-tauri/icons/app-icon.png` is the source for all sizes).
- The file watcher ignores changes to files matched by `.gitignore` (build output, `node_modules`...).
- Dialogs scroll inside the window instead of overflowing it; the graph hides the Author and then the Date column on narrow windows.
- The database schema moves to version 4 (new `imported_cli` sign-in method); existing data is kept.
- File rows show "Stage" / "Unstage" / "Undo" as words instead of icons, and "Stage all" / "Unstage all" stand out more.
- The graph is centered in the "Branch / Graph" column.
- English is listed before Vietnamese in the language choices.

### Fixed

- Settings → "View palette" no longer shows a blank window in release builds: the contrast check rejected the short `#fff` colors produced by the minified CSS.
- Closing a terminal on Windows 10 before it finished starting no longer hangs (the console host waited for a cursor-position answer); this also fixes opening a new terminal after closing all of them.
- Typing right after picking an issue from the `#` suggestions no longer puts the next characters before the inserted reference.
- Worktree actions work when the folder is given through a Windows short name (`RUNNER~1`) or a symlink.
- Two history refreshes arriving together (file watcher and UI) no longer reload the whole history and reset the graph version.
- Cancelling fetch, pull, push or an LFS transfer on Windows now also stops processes started by hooks (Windows Job Object).
- An HTTP 403 from the server is reported as missing permission instead of a connection problem.
- Monaco editors no longer crash the window in the Light theme of release builds (short `#fff` colors from the minified CSS); a failing center view now shows an error with a Close button instead of a blank window.
- The toolbar no longer duplicates the Fetch / Pull / Push / Branch buttons after switching repository tabs.
- Backup refs made before history rewrites are no longer drawn in the commit graph.
- Buttons no longer disappear when the window is made smaller, and the file list of the Changes panel keeps its space on short windows.
- GitHub accounts signed in through the browser no longer ask to sign in again right after signing in (and every 8 hours): their expiring tokens are now refreshed automatically, like GitLab's.

[Unreleased]: https://github.com/congluuit/NANOMultipleGitGUI/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/congluuit/NANOMultipleGitGUI/releases/tag/v1.0.0

# 0003. Per-account HTTPS credentials via a sidecar credential helper and local IPC

- Status: Accepted
- Date: 2026-10-14

## Context

The core feature is using several GitHub/GitLab accounts **on the same host** side by side. Standard credential helpers (Git Credential Manager, osxkeychain, store) keep one credential per host, so a second `github.com` account overwrites the first. Alternatives:

- Token in the remote URL (`https://token@github.com/...`): leaks into `.git/config`, logs, process lists and error messages.
- `GIT_ASKPASS` scripts with the token in an environment variable: visible to every child process and in crash dumps; does not cover all LFS paths cleanly.
- Rewriting the user's global git config per operation: racy with concurrent operations and violates "never break the user's global config" (PLAN §1.3).

## Decision

- The app runs a **credential server on a local socket**: a named pipe on Windows, a Unix-domain socket inside a `0700` temp directory elsewhere (`crates/credentials`).
- Every git process the app spawns gets, through `GIT_CONFIG_COUNT` (inherited by git-lfs, hooks and filters):
  `credential.helper=` (reset — disables GCM/osxkeychain/store) → `credential.helper=!'<path>/nanomgg-credential'` → `credential.useHttpPath=true`, plus `GIT_TERMINAL_PROMPT=0` and `NANOMGG_SOCKET`, `NANOMGG_SESSION` (random per launch), `NANOMGG_ACCOUNT`.
- The sidecar **`nanomgg-credential`** (`crates/credential-helper`) forwards git's `get` request over the socket and prints the answer. On `store`/`erase` it does nothing; outside the app (variables missing) it stays silent.
- The server answers only if the session token matches (constant-time compare), the account exists, and **the requested host equals the account's provider host** over HTTPS (plain HTTP only for loopback). Otherwise it denies and the helper prints `quit=1` so git neither tries other helpers nor prompts.
- Tokens are held in `Zeroizing` buffers and never logged; Phase 2 replaces the in-memory source with the OS keychain.

## Consequences

- Two accounts on one host work concurrently, including Git LFS uploads/downloads, because git-lfs inherits the same environment and calls the same helper.
- Tokens never appear in URLs, `.git/config` or command lines.
- A token is never sent to a third-party LFS server configured via `lfs.url`; the app will ask for a separate credential in that case.
- The sidecar must be bundled with the app (Tauri `externalBin`) and signed with it — Phase 2.
- Hardening backlog for Phase 2: restrictive DACL on the Windows pipe (current default lets other local users open it, though they still need the session token), peer PID/UID checks, `SSH_ASKPASS` support for key passphrases.

## Evidence

`crates/credential-helper/tests/multi_account.rs` runs against a local multi-user smart-HTTP + LFS server (`tools/test-git-server`), with a decoy helper in the "global" config that returns a wrong password:

- alice and bob push to their own repos on the same host, each with an LFS object; the server records the right user for git and LFS requests; commits carry the injected identity;
- bob pushing to alice's repo gets HTTP 403; bob cannot clone it;
- clone with `GIT_LFS_SKIP_SMUDGE=1` + `git lfs pull` downloads LFS objects as alice;
- `git credential fill` for `evil.example.com`, `lfs.thirdparty.io` or plain-HTTP `github.com` returns nothing, and the decoy is never consulted;
- 200 MB LFS file pushed in 5.0 s (~40 MiB/s locally); re-pushing uploads nothing (content-addressed resume);
- mutation check: removing the `credential.helper=` reset makes all four tests fail with the decoy password.

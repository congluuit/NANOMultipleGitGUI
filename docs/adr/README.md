# Architecture Decision Records

Significant technical decisions are recorded here so future contributors understand _why_ the code looks the way it does. One file per decision, numbered, never rewritten after acceptance — a later ADR supersedes an earlier one instead.

| #                                         | Decision                                                                     | Status   |
| ----------------------------------------- | ---------------------------------------------------------------------------- | -------- |
| [0001](0001-tauri-desktop-shell.md)       | Tauri 2 + Rust + React/TypeScript as the desktop stack                       | Accepted |
| [0002](0002-git-cli-and-gix.md)           | System git CLI for writes; machine-readable CLI output (gix later) for reads | Accepted |
| [0003](0003-credential-helper-sidecar.md) | Per-account HTTPS credentials via a sidecar credential helper and local IPC  | Accepted |
| [0004](0004-lfs-two-step-transfer.md)     | Git LFS uploads in two steps (estimate, then transfer with progress)         | Accepted |
| [0005](0005-provider-http-client.md)      | One small reqwest client (rustls + ring) for GitHub and GitLab, no octocrab  | Accepted |

## Template

```markdown
# NNNN. Title

- Status: Proposed | Accepted | Superseded by NNNN
- Date: YYYY-MM-DD

## Context

What problem are we solving and what constraints apply?

## Decision

What we chose, stated plainly.

## Consequences

What becomes easier or harder; follow-up work; risks.

## Evidence (optional)

Prototypes, measurements, links.
```

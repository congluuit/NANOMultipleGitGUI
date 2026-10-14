# 0004. Git LFS uploads in two steps (estimate, then transfer with progress)

- Status: Accepted (implementation in Phase 2)
- Date: 2026-10-14

## Context

Pushing a repository with large LFS objects through a plain `git push` gives poor feedback: the `pre-push` hook uploads everything with no machine-readable progress, the UI cannot say how much will be uploaded or with which account, cancelling leaves the user unsure what was sent, and quota/permission errors arrive as raw hook output.

## Decision

A push from the app runs as:

1. **Estimate** — `git lfs push --dry-run <remote> <ref>` lists the objects and sizes. The UI shows "N LFS files · X GB · as account Y" before anything is sent.
2. **Upload LFS** — `git lfs push <remote> <ref>` with `GIT_LFS_PROGRESS=<temp file>`; the app tails the progress file (files done/total, bytes done/total, current file) to drive per-file and overall progress, speed and ETA in a background transfer queue. Cancel kills the process.
3. **Push git** — normal `git push`; the `pre-push` hook finds every object already on the server and finishes quickly.

All three steps run with the same account context (ADR 0003). Server errors (401/403, quota, size limits, LFS disabled) are mapped to plain-language messages with actions (switch account, open billing page, enable LFS).

## Consequences

- Accurate progress and an upfront size/account confirmation for large uploads.
- Resume is free at file granularity: LFS objects are content-addressed, so after a cancel or network failure the next push only uploads missing objects.
- Three processes instead of one; the estimate step adds a round trip for small pushes (skipped when the repository has no LFS-tracked changes).
- Depends on git-lfs ≥ 3.0 output formats; covered by integration tests against the local LFS server.

## Evidence

The Phase 0 prototype confirms the building blocks: git-lfs inherits the injected credential configuration for batch and transfer requests, the local test server implements the Batch API, and re-pushing after a completed 200 MB upload transfers zero LFS objects (see ADR 0003 evidence).

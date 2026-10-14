# 0002. System git CLI for writes; machine-readable CLI output (gix later) for reads

- Status: Accepted
- Date: 2026-10-14

## Context

A Git GUI must behave exactly like the user's git: hooks, Git LFS filters, commit signing, `safe.directory`, credential helpers, `includeIf`, partial clone and new features as they ship. Libraries (libgit2, gitoxide/`gix`) reimplement git and lag behind or omit some of these (notably hooks and LFS filters). On the other hand, spawning processes and parsing human-oriented output is slow and brittle.

## Decision

1. **All write operations use the system `git` CLI** (≥ 2.35) through `GitRunner`, which injects the account context with `GIT_CONFIG_COUNT`/`GIT_CONFIG_KEY_n`/`GIT_CONFIG_VALUE_n` (see ADR 0003).
2. **Reads also use the CLI for now, but only machine-readable formats**: `--porcelain=v2 -z`, NUL/US-separated `--format`, `--decorate=full`, `git lfs … --json`.
3. **gitoxide (`gix`) is introduced per hot path only when profiling shows the CLI is the bottleneck** (candidates: incremental log for 500k+ commit repos, `status` on huge working trees, object lookups for blame/diff).

## Consequences

- Behavior matches the user's terminal by construction; hooks and LFS just work.
- Parsing code is small and covered by unit tests with fixtures (Unicode/Vietnamese authors and subjects included).
- Each read costs a process spawn; operations are batched (one `git log` for the whole history, paged to the UI from memory) rather than issued per row.
- Minimum git version is enforced at startup (Phase 1).

## Evidence

Graph prototype on generated fixtures (`scripts/gen-history-fixture.mjs`), Windows 10, release build:

| Repo         | `git log --all --topo-order` + parse | Lane layout | First full paint in app | Scroll (600 rows/s)                |
| ------------ | ------------------------------------ | ----------- | ----------------------- | ---------------------------------- |
| 50k commits  | 309–578 ms                           | 21–40 ms    | 372–456 ms              | 143 fps, p95 7.1 ms                |
| 100k commits | 690–835 ms                           | 44–71 ms    | 780 ms                  | 143 fps, p95 7.1 ms, 0 slow frames |

PLAN target is < 2 s for 100k commits and 60 fps scrolling, so `gix` is not needed for the graph today.

# Contributing to NANOMultipleGitGUI

Cảm ơn bạn đã quan tâm! Tài liệu này viết bằng tiếng Anh để mọi contributor đều đọc được; issue và PR có thể viết bằng **tiếng Việt hoặc tiếng Anh**.

Thanks for your interest! Issues and pull requests are welcome in **English or Vietnamese**.

By participating you agree to follow the [Code of Conduct](CODE_OF_CONDUCT.md).
Security problems must **not** be reported in public issues — see [SECURITY.md](SECURITY.md).

## Ways to contribute

- Report bugs and request features through the issue templates.
- Pick an issue labelled `good first issue` or `help wanted`.
- Improve documentation, translations (`apps/desktop/src/i18n/locales/`) or test fixtures.
- Review open pull requests.

For larger changes (new feature, new dependency, architectural change) please open an issue first so we can agree on the approach. Significant decisions are recorded as ADRs in [`docs/adr/`](docs/adr/).

## Development setup

Prerequisites and commands are in the [README](README.md#hướng-dẫn-phát-triển). In short:

```bash
pnpm install
pnpm dev        # run the desktop app with hot reload
pnpm check      # everything CI runs: format, lint, typecheck, tests, clippy
```

Useful extras:

| Command                                                   | What it does                                                                               |
| --------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `pnpm test` / `pnpm --filter @nanomgg/desktop test:watch` | Frontend unit tests (Vitest)                                                               |
| `pnpm rust:test`                                          | Rust tests; also regenerates `apps/desktop/src/ipc/bindings.ts`                            |
| `pnpm dev:web`                                            | Frontend only in a browser; add `?theme=dark&lang=en&view=palette` for quick visual checks |
| `NANOMGG_LOG=debug pnpm dev`                              | Verbose backend logging                                                                    |

## Project layout

- `apps/desktop/src` — React + TypeScript UI. Features live in `src/features/<feature>/`, shared UI in `src/components/`, design tokens in `src/theme/tokens.css`.
- `apps/desktop/src-tauri` — the Tauri app crate: IPC commands, app state, bundling config.
- `crates/*` — reusable Rust crates (git engine, storage, logging, credentials, …).
- `docs/` — install guides and ADRs.

## Coding guidelines

- **Rust:** `cargo fmt`, no `clippy` warnings (`-D warnings`), no `unsafe` without a documented reason. Return typed errors (`thiserror`); never log secrets — everything goes through the redacting logger, but do not rely on it.
- **TypeScript:** strict mode, ESLint + Prettier. Use the generated `commands` from `@/ipc` instead of calling `invoke` with strings. All user-facing text goes through i18next with keys in **both** `vi.json` and `en.json` (a test enforces this).
- **Colors:** only use design tokens (`bg-surface`, `text-muted-foreground`, `var(--primary)`…). New token pairs must pass the WCAG AA contrast test in `src/theme/contrast.test.ts`.
- **Git:** write operations go through the git CLI; parse only machine-readable output (`--porcelain=v2 -z`, NUL-separated `--format`, `--json`).
- **Tests:** add or update tests with every behavior change. Rust integration tests that need a git server use the local test server instead of the network.
- **IPC changes:** after adding/changing a `#[tauri::command]`, run `pnpm rust:test` and commit the regenerated `bindings.ts` (CI fails on a stale file).

## Commits and pull requests

- Use [Conventional Commits](https://www.conventionalcommits.org/): `feat(accounts): add GitLab PKCE login`, `fix(graph): …`, `docs: …`, `chore(deps): …`. The changelog is generated from these.
- Keep PRs focused; one logical change per PR. Fill in the PR template, including screenshots for UI changes (light **and** dark theme).
- Make sure `pnpm check` passes locally.
- By submitting a contribution you agree that it is licensed under the [Apache License 2.0](LICENSE), the project's license (inbound = outbound). No CLA is required.

## Releasing (maintainers)

1. Update `CHANGELOG.md` and bump the version in `package.json`, `apps/desktop/package.json` and the `[workspace.package]` version in `Cargo.toml`.
2. Tag `vX.Y.Z` and push the tag. The `Release` workflow builds all installers into a draft GitHub Release with `SHA256SUMS.txt`.
3. Fill in the release notes, smoke-test the installers (checklist in PLAN §11), then publish.

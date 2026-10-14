# 0005. One small reqwest client for GitHub and GitLab (instead of octocrab)

- Status: Accepted
- Date: 2026-10-14

## Context

PLAN section 2 listed `octocrab` for GitHub and a hand-written `reqwest` client for GitLab. Phase 2 needs, from both providers: current user, emails, a paginated repository list for the unified clone browser, one-repository permission checks, SSH key upload, and token validation that reads response headers (`X-OAuth-Scopes`, `github-authentication-token-expiration`). The OAuth device flow and PKCE flows (`crates/auth`) also need raw form posts to `/login/oauth/access_token` and `/oauth/token`.

Constraints: small installer (Tauri, PLAN 1.3), no OpenSSL (Linux builds, simple CI), MSRV 1.90, tokens never in URLs or logs, one error model the UI can translate for both providers, and mock-friendly tests (`wiremock`, PLAN 11).

## Decision

`crates/providers` talks to both GitHub and GitLab through one `reqwest` 0.13 client (`build_http_client`) with:

- `default-features = false` + `rustls-no-provider`, and `rustls` with the **ring** provider (installed as the process default if none is set). Certificates are checked with `rustls-platform-verifier`, i.e. the OS trust store, so corporate root CAs work. No `aws-lc-rs` (needs cmake/NASM on Windows, larger), no native-tls/OpenSSL.
- HTTP/1.1 only (no `h2`), `json`/`form`/`query` features, `system-proxy` for corporate proxies, 30 s request / 10 s connect timeouts, `User-Agent: NANOMultipleGitGUI/<version>`.
- A thin `ApiClient` (auth header marked sensitive, requests restricted to the configured base URLs, pagination reduced to a page number from `Link` / `X-Next-Page`) and per-provider DTO mapping in `github.rs` / `gitlab.rs` behind the `Provider` trait.
- One typed `ProviderError` (unauthorized, forbidden + hint such as OAuth App restriction / SAML SSO / missing scopes, not found, rate limited with reset time, network, timeout, ...) shared by both providers and reused by `crates/auth`.

`octocrab` is not used.

## Consequences

- Smaller dependency tree and binary: octocrab brings its own hyper/tower client stack, its own TLS feature set, JWT/GitHub App machinery, snafu errors and a large generated model we would mostly not use; with reqwest the HTTP stack is the one already in the workspace lockfile (tauri 2.12 depends on reqwest 0.13 on some targets) and is shared with the auth flows.
- Both providers behave the same for timeouts, proxies, TLS, redirects, cancellation (`CancellationToken`) and error mapping; the UI translates one error enum.
- Response headers are directly available (token scopes/expiry, rate-limit reset), which octocrab's typed API hides.
- We maintain the DTOs and endpoints ourselves. The surface is small (≈10 endpoints per provider) and covered by `wiremock` tests; Phase 4 (PR/MR, issues, CI) adds endpoints the same way. If GitHub GraphQL becomes necessary, it is one more `POST /graphql` through the same client.
- ring as crypto provider: if another crate in the app installs a different rustls provider first, that one is used — both are fine for client TLS.

## Evidence

- `cargo tree -p nanomgg-providers -e normal` contains no `aws-lc-*`, `openssl*`, `native-tls` or `h2`.
- Tests: `crates/providers/tests/{github,gitlab}.rs` and `crates/auth/tests/*.rs` cover pagination, permissions, SSH keys, error mapping, device flow, PKCE loopback, refresh and PAT validation without real network.

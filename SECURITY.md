# Security Policy

NANOMultipleGitGUI handles access tokens, SSH keys and credentials for several GitHub/GitLab accounts, so we take security reports seriously.

## Reporting a vulnerability

**Please do not open a public issue for security problems.**

Report privately through GitHub: open the repository's **Security** tab → **Report a vulnerability** (GitHub private vulnerability reporting), or email the maintainer, Cong Luu, at cong.luu.it@gmail.com. Include:

- affected version / commit and operating system,
- steps to reproduce or a proof of concept,
- the impact you expect (e.g. token disclosure, wrong account used for a push).

You can write in English or Vietnamese. We aim to acknowledge reports within **3 working days** and to agree on a fix and disclosure timeline with you. We are happy to credit reporters in the release notes unless you prefer otherwise.

## Supported versions

Until v1.0 only the **latest released version** receives security fixes.

## Scope

Especially relevant areas:

- credential handling: keychain storage, the `nanomgg-credential` helper and its IPC channel, host checks before handing out tokens (including Git LFS endpoints),
- token leakage through logs, URLs, process arguments, crash reports or `.git/config`,
- SSH key generation/import and `GIT_SSH_COMMAND` injection,
- commits or pushes performed with a different account than the one shown in the UI,
- the update mechanism and release artifacts.

Out of scope: vulnerabilities in Git, Git LFS, WebView2/WebKit or the operating system itself (report them upstream), and issues that require an already compromised user account.

## Design commitments

- Secrets are stored only in the OS keychain (Windows Credential Manager, macOS Keychain, Secret Service), never in plain text files or SQLite.
- Logs pass through a redaction filter for known token formats, `Authorization` headers and URL credentials.
- The credential helper only answers requests whose host matches the account's provider.

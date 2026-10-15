# Privacy / Quyền riêng tư

**Tóm tắt:** NANOMultipleGitGUI chạy hoàn toàn trên máy của bạn. Dự án **không vận hành máy chủ nào** và **không thu thập dữ liệu** của bạn. Telemetry **mặc định tắt**.

**Summary:** NANOMultipleGitGUI runs entirely on your computer. The project **operates no servers** and **collects none of your data**. Telemetry is **off by default**.

## What stays on your device

| Data                                                                                                                                                                     | Where                                                          |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------- |
| Settings, account metadata (username, display name, avatar URL, colors, labels), repository list, account bindings and rules, custom themes, personal notes, AI settings | Local SQLite database in the app data directory                |
| Access tokens, refresh tokens, SSH key passphrases, AI API keys                                                                                                          | Operating-system keychain only                                 |
| SSH private keys created by the app                                                                                                                                      | App data directory with user-only permissions, or the keychain |
| Logs (with secrets redacted)                                                                                                                                             | App log directory, rotated daily, 14 files kept                |

Uninstalling the app does not delete these by default; you can remove the app data and log directories manually (paths are listed in the install guides).

## Network connections

The app only connects to services **you** configure or trigger:

- the Git remotes of your repositories (fetch, pull, push, clone, Git LFS),
- the APIs of the GitHub / GitLab instances you sign in to (repositories, pull/merge requests, CI status, avatars),
- GitHub Releases of this project, when checking for updates (auto-update can be disabled),
- the official Git LFS / MinGit download locations, only when you ask the app to install them,
- the AI service you choose for commit message suggestions (off by default): when you click ✨, the staged changes, branch name and recent commit titles are sent to that service (OpenAI, Anthropic, Google Gemini or the OpenAI-compatible address you entered). With Ollama the request goes to your own computer (`127.0.0.1`), and downloading Ollama or its models only happens when you ask for it.

Export and backup files are encrypted with the password you choose and saved only where you put them.

Requests to providers are made with the account you selected and are subject to those providers' own privacy policies.

## Telemetry

There is currently **no telemetry**. If anonymous usage statistics or crash reports are ever added they will be **opt-in**, documented here before release, and will never include repository contents, file names, remote URLs, tokens or personal identifiers.

## Contact

Privacy questions can be raised in a GitHub issue, or privately through the channel in [SECURITY.md](SECURITY.md).

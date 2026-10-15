<p align="center">
  <img src="apps/desktop/public/logo.png" width="96" height="96" alt="NANOMultipleGitGUI logo" />
</p>

<h1 align="center">NANO Multiple Git GUI</h1>

<p align="center">
  A free, open-source Git app for Windows, macOS and Linux that handles <b>many GitHub and GitLab accounts at the same time</b> — without mixing them up.
</p>

<p align="center">
  <a href="https://github.com/congluuit/NANOMultipleGitGUI/releases/latest"><img alt="Download" src="https://img.shields.io/github/v/release/congluuit/NANOMultipleGitGUI?label=download&color=237A35" /></a>
  <img alt="License" src="https://img.shields.io/badge/license-Apache--2.0-E8590C" />
  <img alt="Platforms" src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-1A68B0" />
  <img alt="Tauri" src="https://img.shields.io/badge/Tauri-2-FF8A3D" />
</p>

<p align="center">
  <a id="english"></a><b>English</b> · <a href="#tiếng-việt">Tiếng Việt</a>
</p>

---

## Why NANOMultipleGitGUI?

Many people use more than one Git account: one for work, one for personal projects, maybe one for a client. Most Git apps only let you sign in to one account per service, which leads to everyday problems:

- commits made with your personal email in a company repository (or the other way round),
- pushes rejected because the wrong account was used,
- hours spent editing SSH or credential settings by hand.

**NANOMultipleGitGUI remembers which account each repository belongs to** and uses the right name, email, password/token and SSH key for every action — clone, commit, push and large files. You always see "who you are" before you commit or push, and the app warns you before a mistake happens.

## Download

Get the latest version from **[GitHub Releases](https://github.com/congluuit/NANOMultipleGitGUI/releases/latest)**.

| Your computer                             | File to download                         |
| ----------------------------------------- | ---------------------------------------- |
| Windows 10 / 11                           | `NANOMultipleGitGUI_x.y.z_x64-setup.exe` |
| Windows (company / IT deployment)         | `NANOMultipleGitGUI_x.y.z_x64_en-US.msi` |
| macOS 11 or later (Apple Silicon & Intel) | `NANOMultipleGitGUI_x.y.z_universal.dmg` |
| Linux                                     | `.AppImage`, `.deb` or `.rpm`            |

**First launch:** the app is free and not yet code-signed, so your system may ask for confirmation once.

- **Windows:** if "Windows protected your PC" appears, click **More info → Run anyway**. Step-by-step: [docs/INSTALL-Windows.md](docs/INSTALL-Windows.md).
- **macOS:** right-click the app → **Open**, or allow it in **System Settings → Privacy & Security**. Step-by-step: [docs/INSTALL-macOS.md](docs/INSTALL-macOS.md).

You need [Git](https://git-scm.com/downloads) installed. If it is missing, the first-run setup checks for it and can install it for you.

## Features

### Many accounts, zero mix-ups

- **Unlimited accounts** on GitHub (github.com and GitHub Enterprise) and GitLab (gitlab.com and self-hosted servers), all signed in at once.
- **Each repository knows its account** — chosen automatically by rules you set (for example "everything in `D:/Work` or under `github.com/my-company` uses my work account") or picked by hand.
- **The right name and email on every commit**, set automatically per repository.
- **Switch accounts in one click**, or use another account for a single commit or push only.
- **Commit Guard and Push Guard** warn you before committing or pushing with the wrong identity, an unverified email, or an account without access.
- **Sign in safely**: through your browser or with a personal access token. Passwords and tokens are kept in your system's secure keychain, never in plain text.
- **SSH keys per account**: create, import and upload keys from the app — no manual configuration files.
- **Move your accounts to another computer** with an encrypted export file (Account Manager → **Export settings… / Import settings…**). Sign-in tokens are left out unless you choose to include them.

### Everyday Git, made clear

- Visual **commit history** (graph) with branches, tags, author avatars and fast search — smooth even with 100,000+ commits.
- **Stage and commit** whole files, single blocks of changes, or individual lines; amend the last commit; undo changes.
- **Double-click a branch** in the left panel to switch to it. Unsaved work is set aside automatically (stashed) and brought back when you return to that branch, so nothing is lost.
- Create, rename, merge, rebase (including drag-and-drop interactive rebase), cherry-pick, revert, reset and tag — from menus or by dragging branches.
- **Undo / Redo** for recent Git actions.
- **Fetch, pull and push** with progress and a cancel button; ↑/↓ counters show what still needs to be sent or received; background fetch keeps them up to date.
- **Conflict editor** that shows "mine", "theirs" and the result side by side, including for images and binary files.
- **File history, blame** (who changed each line) and "view this file at any commit".
- Stashes, submodules, worktrees, commit signing (GPG / SSH) with a "Verified" badge.

### Large files without surprises

- **Large File Guard** warns before you commit a file that GitHub or GitLab would reject, and moves it to **Git LFS** in one click.
- Push checks catch large files that were already committed but not yet pushed.
- LFS uploads and downloads show progress, can be cancelled and resume where they stopped.

### GitHub and GitLab built in

- **Pull requests / merge requests**: list, read, comment, review, create, check out and merge.
- **Issues**: browse, create, and create a branch from an issue.
- **Unified clone**: search the repositories of all your accounts in one list.
- **Fork & clone** with the original repository added automatically.
- Optional CI status (GitHub Actions, GitLab pipelines).

### Work smarter

- **AI commit messages** (optional, off by default): click ✨ next to the commit summary to get a suggested message. Use your own key for OpenAI, Anthropic (Claude), Google Gemini or any OpenAI-compatible service — or run a free model **on your own computer** with Ollama, which the app can install and set up for you (small recommended models included).
- **My Work** gathers the pull requests waiting for your review, your own pull requests and the issues assigned to you, across every account.
- **My notes** (inside My Work): a small personal notebook with colored tags (Default, Urgent, WIP, Review, Bug, Idea, Done or your own), priorities, pins and due dates. Export to a file to back them up or move them to another computer.
- **Built-in terminal** that already uses the repository's account (`` Ctrl+` ``).
- **Command palette** (`Ctrl+K` / `Cmd+K`) to reach any action by typing.

### Make it yours

- **Light, Dark or follow your system** — plus **custom themes**: two High Contrast themes are included, and you can design your own colors with a live preview and readability checks, then share them as a file.
- English and Vietnamese interface.
- Resizable, collapsible panels that adapt to small screens.

### Back up and move to a new computer

- **Full backup**: Settings → **Backup & restore** saves everything in the app (settings, accounts, repository list, themes, notes and, if you want, sign-in tokens and SSH keys) into one password-protected file. Store it on Google Drive, OneDrive or any cloud. On a new computer, restore it and you are ready to go.
- **Storage management**: choose where logs, cache and app data are kept, clear the cache, and delete your themes or notes from Settings → **Storage**.

### Help when you need it

- A short **getting-started guide** on first launch checks your tools and helps you add accounts.
- A **Full Guide** in Settings explains every feature in plain language, with a search box.
- Error messages say what happened and what to click to fix it.

## Privacy and security

- Everything runs on your computer. The project has **no servers** and collects **no data**; there is no telemetry.
- Tokens, refresh tokens and SSH passphrases are stored only in your system keychain (Windows Credential Manager, macOS Keychain, Linux Secret Service).
- The app only connects to the Git servers and GitHub/GitLab accounts you use, to GitHub Releases for updates, and — only if you turn on AI suggestions with a cloud service — to that AI service, sending your staged changes when you click ✨. With Ollama, your code never leaves your computer.
- Export and backup files are encrypted with your password (Argon2id + XChaCha20-Poly1305).

Details: [PRIVACY.md](PRIVACY.md) · Reporting a vulnerability: [SECURITY.md](SECURITY.md).

## For developers

Built with [Tauri 2](https://tauri.app) (Rust) and React 19 + TypeScript. Git operations use the system Git CLI so hooks, LFS, signing and credentials behave exactly like the command line.

### Requirements

| Tool    | Version                                                                                    |
| ------- | ------------------------------------------------------------------------------------------ |
| Node.js | 20.19 or later                                                                             |
| pnpm    | 9.x (`npm install -g pnpm@9` or `corepack enable`)                                         |
| Rust    | stable 1.90 or later ([rustup](https://rustup.rs))                                         |
| Git     | 2.35 or later, with Git LFS 3.0 or later                                                   |
| Windows | Microsoft C++ Build Tools ("Desktop development with C++") and WebView2 (built into 10/11) |
| macOS   | Xcode Command Line Tools (`xcode-select --install`)                                        |
| Linux   | System packages from the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)  |

### Run, build, check

```bash
pnpm install
pnpm dev          # run the app with hot reload
pnpm build        # build the installers for your OS (output: target/release/bundle/)
pnpm check        # format, lint, typecheck, tests, clippy and Rust tests
```

Optional build settings (OAuth client IDs, update signing) are described in [.env.example](.env.example). Releasing a new version: [docs/RELEASING.md](docs/RELEASING.md).

### Project layout

```
apps/desktop/          Tauri app
  src/                   React + TypeScript frontend (features/, components/, theme/, i18n/)
  src-tauri/             Rust backend of the app (commands, state, bundle settings)
crates/                Shared Rust crates
  git-engine/            runs Git per account, reads history, lays out the graph
  credentials/           local credential server   credential-helper/  `nanomgg-credential` sidecar
  accounts/ auth/ providers/ ssh-keys/ lfs/ storage/ logging/ import/ app-paths/ backup/
tools/test-git-server/ multi-user Git + LFS server used by tests
docs/                  install guides, release guide, architecture decisions (docs/adr)
```

### Tech stack

| Layer             | Technology                                                         |
| ----------------- | ------------------------------------------------------------------ |
| Desktop shell     | Tauri 2                                                            |
| Backend           | Rust (Cargo workspace)                                             |
| Frontend          | React 19, TypeScript, Vite                                         |
| UI                | Tailwind CSS, Radix UI (shadcn/ui), design tokens in CSS variables |
| State             | Zustand, TanStack Query                                            |
| IPC               | Typed Tauri commands, TypeScript types generated with tauri-specta |
| Git               | System Git CLI (machine-readable output), Git LFS CLI              |
| Storage           | SQLite; secrets in the OS keychain (`keyring`)                     |
| Editor / Terminal | Monaco Editor, xterm.js + `portable-pty`                           |
| i18n              | i18next (English, Vietnamese)                                      |

Contributions are welcome — see [CONTRIBUTING.md](CONTRIBUTING.md). Design notes and roadmap: [PLAN.md](PLAN.md) (Vietnamese) and [docs/adr](docs/adr/).

## Support the project

If you like this idea, please support me via (Nếu bạn thấy thích ý tưởng này, hãy ủng hộ tôi bằng):

<p align="center">
  <img src="./QR_BIDV_Donate.jpg" width="260" alt="Donate via BIDV QR" />
</p>

## License

[Apache License 2.0](LICENSE) — free to use, modify and share, including commercially. See also [NOTICE](NOTICE).

---

<a id="tiếng-việt"></a>

# Tiếng Việt

<p align="center">
  <a href="#english">English</a> · <b>Tiếng Việt</b>
</p>

**NANOMultipleGitGUI** là ứng dụng Git miễn phí, mã nguồn mở cho Windows, macOS và Linux, dùng được **nhiều tài khoản GitHub và GitLab cùng lúc** mà không bị lẫn lộn.

## Vì sao nên dùng?

Nhiều người dùng hơn một tài khoản Git: một cho công việc, một cho dự án cá nhân, có khi thêm một cho khách hàng. Phần lớn ứng dụng Git chỉ cho đăng nhập một tài khoản cho mỗi dịch vụ, dẫn tới những rắc rối hằng ngày:

- commit vào repo công ty bằng email cá nhân (hoặc ngược lại),
- push bị từ chối vì dùng nhầm tài khoản,
- mất hàng giờ tự sửa cấu hình SSH hoặc mật khẩu.

**NANOMultipleGitGUI ghi nhớ mỗi repo thuộc tài khoản nào** và tự dùng đúng tên, email, token và SSH key cho mọi thao tác — clone, commit, push và file lớn. Bạn luôn thấy rõ "mình đang dùng tài khoản nào" trước khi commit hay push, và app cảnh báo trước khi sai sót xảy ra.

## Tải về

Tải bản mới nhất tại **[GitHub Releases](https://github.com/congluuit/NANOMultipleGitGUI/releases/latest)**.

| Máy của bạn                              | File cần tải                             |
| ---------------------------------------- | ---------------------------------------- |
| Windows 10 / 11                          | `NANOMultipleGitGUI_x.y.z_x64-setup.exe` |
| Windows (cài hàng loạt trong công ty)    | `NANOMultipleGitGUI_x.y.z_x64_en-US.msi` |
| macOS 11 trở lên (Apple Silicon & Intel) | `NANOMultipleGitGUI_x.y.z_universal.dmg` |
| Linux                                    | `.AppImage`, `.deb` hoặc `.rpm`          |

**Lần mở đầu tiên:** app miễn phí và chưa được ký số, nên hệ điều hành có thể hỏi xác nhận một lần.

- **Windows:** nếu hiện "Windows protected your PC", bấm **More info → Run anyway**. Hướng dẫn chi tiết: [docs/INSTALL-Windows.md](docs/INSTALL-Windows.md).
- **macOS:** chuột phải vào app → **Open**, hoặc cho phép trong **System Settings → Privacy & Security**. Hướng dẫn chi tiết: [docs/INSTALL-macOS.md](docs/INSTALL-macOS.md).

Máy cần có [Git](https://git-scm.com/downloads). Nếu chưa có, trình hướng dẫn lần đầu sẽ kiểm tra và có thể cài giúp bạn.

## Tính năng

### Nhiều tài khoản, không nhầm lẫn

- **Không giới hạn số tài khoản** GitHub (github.com và GitHub Enterprise) và GitLab (gitlab.com và máy chủ tự dựng), đăng nhập cùng lúc.
- **Mỗi repo tự biết tài khoản của mình** — chọn tự động theo quy tắc bạn đặt (ví dụ "mọi thứ trong `D:/Work` hoặc thuộc `github.com/cong-ty` dùng tài khoản công ty") hoặc chọn tay.
- **Đúng tên và email trên mọi commit**, tự đặt theo từng repo.
- **Đổi tài khoản bằng một cú bấm**, hoặc dùng tài khoản khác chỉ cho một lần commit hay push.
- **Commit Guard và Push Guard** cảnh báo trước khi commit hoặc push sai danh tính, dùng email chưa xác minh, hoặc tài khoản không có quyền.
- **Đăng nhập an toàn** qua trình duyệt hoặc bằng token cá nhân. Mật khẩu và token nằm trong kho bảo mật của hệ điều hành, không bao giờ lưu dạng chữ thường.
- **SSH key theo từng tài khoản**: tạo, nhập và tải key lên ngay trong app — không phải sửa file cấu hình.
- **Chuyển tài khoản sang máy khác** bằng file xuất được mã hóa (Quản lý tài khoản → **Xuất cấu hình… / Nhập cấu hình…**). Token đăng nhập không được đưa vào file trừ khi bạn chọn.

### Git hằng ngày, rõ ràng và dễ hiểu

- **Lịch sử commit** dạng sơ đồ với nhánh, tag, ảnh đại diện tác giả và tìm kiếm nhanh — vẫn mượt với hơn 100.000 commit.
- **Stage và commit** cả file, từng khối thay đổi hoặc từng dòng; sửa commit vừa tạo (amend); hoàn tác thay đổi.
- **Nhấp đúp vào nhánh** ở panel trái để chuyển sang nhánh đó. Phần việc chưa commit được cất tạm tự động (stash) và trả lại khi bạn quay về nhánh cũ, nên không mất gì.
- Tạo, đổi tên, merge, rebase (có cả rebase tương tác kéo-thả), cherry-pick, revert, reset và tag — bằng menu hoặc kéo-thả nhánh.
- **Hoàn tác / Làm lại** các thao tác Git gần đây.
- **Fetch, pull và push** có tiến độ và nút hủy; số ↑/↓ cho biết còn bao nhiêu commit cần gửi đi hoặc cần lấy về; app tự fetch nền để số luôn mới.
- **Trình giải quyết xung đột** hiển thị song song "của tôi", "của họ" và kết quả, kể cả với ảnh và file nhị phân.
- **Lịch sử file, blame** (ai sửa từng dòng) và "xem file tại bất kỳ commit nào".
- Stash, submodule, worktree, ký commit (GPG / SSH) với nhãn "Verified".

### File lớn không còn bất ngờ

- **Large File Guard** cảnh báo trước khi bạn commit file mà GitHub hoặc GitLab sẽ từ chối, và chuyển file đó sang **Git LFS** bằng một cú bấm.
- Kiểm tra trước khi push phát hiện file lớn đã lỡ commit nhưng chưa push.
- Tải lên / tải về LFS có tiến độ, hủy được và tiếp tục từ chỗ đã dừng.

### Tích hợp sẵn GitHub và GitLab

- **Pull request / merge request**: xem danh sách, đọc, bình luận, review, tạo mới, checkout và merge.
- **Issue**: xem, tạo, và tạo nhánh từ issue.
- **Clone hợp nhất**: tìm repo của mọi tài khoản trong một danh sách.
- **Fork & clone**, tự thêm repo gốc làm `upstream`.
- Trạng thái CI (GitHub Actions, GitLab pipeline) — bật khi cần.

### Làm việc thông minh hơn

- **Gợi ý commit message bằng AI** (tùy chọn, mặc định tắt): bấm ✨ cạnh ô tóm tắt commit để nhận gợi ý. Dùng key của bạn cho OpenAI, Anthropic (Claude), Google Gemini hoặc dịch vụ tương thích OpenAI — hoặc chạy model miễn phí **ngay trên máy bạn** với Ollama; app có thể cài đặt và thiết lập giúp (kèm gợi ý các model nhẹ).
- **Việc của tôi** gom pull request đang chờ bạn review, pull request của bạn và issue giao cho bạn, trên mọi tài khoản.
- **Ghi chú của tôi** (trong Việc của tôi): sổ tay nhỏ cho riêng bạn với tag màu (Default, Urgent, WIP, Review, Bug, Idea, Done hoặc tag tự tạo), mức ưu tiên, ghim và hạn chót. Xuất ra file để sao lưu hoặc chuyển sang máy khác.
- **Terminal tích hợp** đã dùng sẵn tài khoản của repo (`` Ctrl+` ``).
- **Bảng lệnh** (`Ctrl+K` / `Cmd+K`) để gõ tìm và chạy mọi thao tác.

### Theo cách của bạn

- **Sáng, Tối hoặc theo hệ thống** — cùng **theme tùy chỉnh**: có sẵn 2 theme Tương phản cao, và bạn có thể tự phối màu với xem trước trực tiếp và kiểm tra độ dễ đọc, rồi chia sẻ dưới dạng file.
- Giao diện tiếng Anh và tiếng Việt.
- Các panel kéo giãn, thu gọn được, tự thích nghi với màn hình nhỏ.

### Sao lưu và chuyển sang máy mới

- **Sao lưu toàn bộ**: Cài đặt → **Sao lưu & khôi phục** lưu mọi thứ trong app (cài đặt, tài khoản, danh sách repo, theme, ghi chú và — nếu bạn muốn — token đăng nhập và SSH key) vào một file có mật khẩu. Cất file lên Google Drive, OneDrive hay bất kỳ dịch vụ đám mây nào. Ở máy mới, khôi phục là dùng được ngay.
- **Quản lý lưu trữ**: chọn nơi lưu log, cache và dữ liệu app, xóa cache, xóa theme hoặc ghi chú tại Cài đặt → **Lưu trữ**.

### Luôn có hướng dẫn

- **Hướng dẫn ban đầu** ngắn gọn khi mở app lần đầu: kiểm tra công cụ và giúp thêm tài khoản.
- **Hướng dẫn đầy đủ** trong Cài đặt giải thích mọi tính năng bằng lời dễ hiểu, có ô tìm kiếm.
- Thông báo lỗi luôn nói rõ chuyện gì xảy ra và cần bấm gì để sửa.

## Quyền riêng tư và bảo mật

- Mọi thứ chạy trên máy của bạn. Dự án **không có máy chủ** và **không thu thập dữ liệu**; không có telemetry.
- Token, refresh token và passphrase SSH chỉ nằm trong kho bảo mật của hệ điều hành (Windows Credential Manager, macOS Keychain, Linux Secret Service).
- App chỉ kết nối tới các máy chủ Git và tài khoản GitHub/GitLab bạn dùng, tới GitHub Releases để kiểm tra cập nhật, và — chỉ khi bạn bật gợi ý AI bằng dịch vụ đám mây — tới dịch vụ AI đó, gửi phần thay đổi đã stage khi bạn bấm ✨. Với Ollama, code không rời khỏi máy bạn.
- File xuất và file sao lưu được mã hóa bằng mật khẩu của bạn (Argon2id + XChaCha20-Poly1305).

Chi tiết: [PRIVACY.md](PRIVACY.md) · Báo lỗ hổng bảo mật: [SECURITY.md](SECURITY.md).

## Dành cho lập trình viên

App xây dựng bằng [Tauri 2](https://tauri.app) (Rust) và React 19 + TypeScript. Thao tác Git dùng Git CLI của hệ thống nên hook, LFS, ký commit và credential hoạt động y như dòng lệnh.

### Yêu cầu

| Công cụ | Phiên bản                                                                                  |
| ------- | ------------------------------------------------------------------------------------------ |
| Node.js | 20.19 trở lên                                                                              |
| pnpm    | 9.x (`npm install -g pnpm@9` hoặc `corepack enable`)                                       |
| Rust    | stable 1.90 trở lên ([rustup](https://rustup.rs))                                          |
| Git     | 2.35 trở lên, kèm Git LFS 3.0 trở lên                                                      |
| Windows | Microsoft C++ Build Tools ("Desktop development with C++") và WebView2 (có sẵn trên 10/11) |
| macOS   | Xcode Command Line Tools (`xcode-select --install`)                                        |
| Linux   | Các gói hệ thống theo [hướng dẫn của Tauri](https://v2.tauri.app/start/prerequisites/)     |

### Chạy, build, kiểm tra

```bash
pnpm install
pnpm dev          # chạy app với hot reload
pnpm build        # build bộ cài cho hệ điều hành hiện tại (kết quả: target/release/bundle/)
pnpm check        # format, lint, typecheck, test, clippy và test Rust
```

Các tùy chọn khi build (OAuth client ID, khóa ký bản cập nhật) mô tả trong [.env.example](.env.example). Phát hành phiên bản mới: [docs/RELEASING.md](docs/RELEASING.md).

Cấu trúc thư mục và tech stack: xem phần [For developers](#for-developers) ở trên. Mọi đóng góp đều được hoan nghênh — xem [CONTRIBUTING.md](CONTRIBUTING.md). Thiết kế và lộ trình: [PLAN.md](PLAN.md).

## Ủng hộ

Nếu bạn thấy thích ý tưởng này, hãy ủng hộ tôi bằng:

<p align="center">
  <img src="./QR_BIDV_Donate.jpg" width="260" alt="Ủng hộ qua mã QR BIDV" />
</p>

## Giấy phép

[Apache License 2.0](LICENSE) — tự do sử dụng, chỉnh sửa và chia sẻ, kể cả cho mục đích thương mại. Xem thêm [NOTICE](NOTICE).

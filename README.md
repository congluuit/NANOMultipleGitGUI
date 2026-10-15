<p align="center">
  <img src="apps/desktop/public/logo.png" width="96" height="96" alt="NANOMultipleGitGUI logo" />
</p>

<h1 align="center">NANOMultipleGitGUI</h1>

<p align="center">
  Git GUI mã nguồn mở, quản lý <b>nhiều tài khoản GitHub/GitLab cùng lúc</b> — chạy trên Windows và macOS.<br />
  <i>Open-source multi-account Git GUI for GitHub &amp; GitLab — Windows and macOS.</i>
</p>

<p align="center">
  <img alt="License" src="https://img.shields.io/badge/license-Apache--2.0-E8590C" />
  <a href="https://github.com/congluuit/NANOMultipleGitGUI/releases/latest"><img alt="Version" src="https://img.shields.io/badge/version-v1.0.0-237A35" /></a>
  <img alt="Status" src="https://img.shields.io/badge/status-stable-237A35" />
  <img alt="Tauri" src="https://img.shields.io/badge/Tauri-2-FF8A3D" />
</p>

> **v1.0.0** — bản phát hành chính thức đầu tiên. Tải bộ cài tại [GitHub Releases](https://github.com/congluuit/NANOMultipleGitGUI/releases/latest); app tự kiểm tra và cài bản cập nhật.

---

## Giới thiệu

Đa số Git GUI chỉ cho gắn một tài khoản cho mỗi provider, khiến việc làm song song repo công ty và repo cá nhân rất dễ commit nhầm email, giẫm credential HTTPS hoặc phải tự chỉnh `~/.ssh/config`. **NANOMultipleGitGUI** giải quyết bằng cách để **mỗi repo tự biết nó thuộc tài khoản nào** và tự dùng đúng danh tính, credential, SSH key cho mọi thao tác clone / commit / push / LFS.

Mã ngắn dùng trong code, binary và biến môi trường: `nanomgg`.

## Tính năng chính

- **Đa tài khoản không giới hạn** cho GitHub (github.com, Enterprise Server) và GitLab (gitlab.com, self-hosted), dùng song song.
- **Gắn tài khoản theo repo**: tự nhận diện qua rule (host/owner, thư mục, regex URL) hoặc gán thủ công; chuyển tài khoản nhanh bằng account pill, command palette, phím tắt.
- **Identity đi theo tài khoản**: `user.name`/`user.email` tự đúng theo repo (mặc định ghi vào `.git/config` của repo, có thể tắt).
- **Commit Guard & Push Guard**: cảnh báo commit/push sai danh tính, email chưa verify, thiếu quyền push.
- **Credential an toàn**: token nằm trong keychain của hệ điều hành; credential helper riêng nên nhiều tài khoản trên cùng host không xung đột; SSH nhiều key không cần host alias.
- **Clone hợp nhất**: duyệt/tìm repo trên mọi tài khoản cùng lúc, tự chọn tài khoản và thư mục đích.
- **Git LFS & file nặng**: Large File Guard trước khi commit, kiểm tra blob nặng trước khi push, upload/download LFS có tiến độ, hủy và tiếp tục.
- **Đầy đủ tính năng Git**: commit graph, staging theo hunk/dòng, diff, branch, merge/rebase (gồm interactive rebase), stash, conflict editor 3-way, PR/MR, terminal tích hợp.
- **Theme Light / Dark / System**, giao diện Tiếng Việt và English.

Chi tiết thiết kế và lộ trình: xem [PLAN.md](PLAN.md).

## Tech stack

| Lớp               | Công nghệ                                                             |
| ----------------- | --------------------------------------------------------------------- |
| Desktop shell     | [Tauri 2](https://tauri.app)                                          |
| Backend           | Rust (Cargo workspace)                                                |
| Frontend          | React 19 + TypeScript + Vite                                          |
| UI                | Tailwind CSS + Radix UI (shadcn/ui), design tokens bằng CSS variables |
| State             | Zustand + TanStack Query                                              |
| IPC               | Command Tauri có kiểu, sinh type TypeScript bằng tauri-specta         |
| Git engine        | Git CLI hệ thống (output máy đọc), `git-lfs` CLI; gitoxide khi cần    |
| Lưu trữ           | SQLite; secret trong keychain hệ điều hành (`keyring`)                |
| Provider API      | `octocrab` (GitHub), client GitLab dùng `reqwest`                     |
| Editor / Terminal | Monaco Editor, xterm.js + `portable-pty`                              |
| i18n              | i18next (vi, en)                                                      |

## Cấu trúc thư mục

```
apps/desktop/          # Ứng dụng Tauri
  src/                 #   Frontend React/TS (features/, components/, theme/, i18n/, ipc/)
  src-tauri/           #   Backend Rust của app (command, state, cấu hình bundle)
crates/                # Các crate Rust dùng chung
  git-engine/          #   chạy git CLI theo tài khoản, đọc lịch sử, bố cục graph
  credentials/         #   server credential qua IPC cục bộ
  credential-helper/   #   binary sidecar `nanomgg-credential`
  storage/  logging/   #   SQLite + migrations, log có lọc token
  lfs/  accounts/  auth/  ssh-keys/  providers/
tools/test-git-server/ # server Git + LFS nhiều user, chỉ dùng cho test
scripts/               # script build/ký số, sinh repo mẫu
docs/                  # hướng dẫn cài đặt, ADR
```

## Hướng dẫn phát triển

### Yêu cầu

| Công cụ | Phiên bản                                                                                         |
| ------- | ------------------------------------------------------------------------------------------------- |
| Node.js | ≥ 20.19                                                                                           |
| pnpm    | 9.x (`npm install -g pnpm@9` hoặc `corepack enable`)                                              |
| Rust    | stable ≥ 1.90 (cài qua [rustup](https://rustup.rs))                                               |
| Git     | ≥ 2.35, kèm `git-lfs` ≥ 3.0                                                                       |
| Windows | Microsoft C++ Build Tools ("Desktop development with C++") + WebView2 (có sẵn trên Windows 10/11) |
| macOS   | Xcode Command Line Tools (`xcode-select --install`)                                               |
| Linux   | Các gói hệ thống theo [hướng dẫn của Tauri](https://v2.tauri.app/start/prerequisites/)            |

### Chạy ở chế độ dev

```bash
pnpm install
pnpm dev          # mở app Tauri với hot-reload
pnpm dev:web      # chỉ chạy frontend trên trình duyệt (http://localhost:1420), không có backend
```

### Build bộ cài

```bash
pnpm build                          # build theo cấu hình bundle cho OS hiện tại
pnpm tauri build --bundles nsis     # chỉ build bộ cài .exe (Windows)
pnpm tauri build --bundles dmg      # chỉ build .dmg (macOS)
```

File đầu ra nằm trong `target/release/bundle/`.

### Kiểm tra chất lượng code

```bash
pnpm check        # chạy toàn bộ các bước dưới đây
pnpm lint         # ESLint
pnpm typecheck    # TypeScript
pnpm test         # Vitest (frontend)
pnpm format       # Prettier (format:check để chỉ kiểm tra)
pnpm rust:check   # cargo fmt --check + clippy -D warnings
pnpm rust:test    # cargo test --workspace (đồng thời sinh lại src/ipc/bindings.ts)
```

### Đo hiệu năng & test nặng

```bash
pnpm fixture:graph                          # sinh repo mẫu 50k commit vào target/fixtures/graph-50k
pnpm bench:graph target/fixtures/graph-50k  # đo đọc lịch sử + bố cục graph
pnpm test:lfs-200mb                         # test push LFS 200 MB bằng 2 tài khoản (server cục bộ)
```

Mở app trực tiếp vào một repo: `nanomgg <đường-dẫn-repo>`; thêm `--bench-graph` để tự đo tốc độ cuộn.

Hướng dẫn đóng góp chi tiết: [CONTRIBUTING.md](CONTRIBUTING.md). Các quyết định kiến trúc: [docs/adr](docs/adr/).

## Cài đặt (người dùng)

Tải bộ cài mới nhất tại [GitHub Releases](https://github.com/congluuit/NANOMultipleGitGUI/releases/latest): `.exe` / `.msi` (Windows 10/11), `.dmg` (macOS 11+, Apple Silicon & Intel), `.AppImage` / `.deb` / `.rpm` (Linux). Cài xong là dùng được: đăng nhập GitHub bằng trình duyệt, app tự cập nhật. Hướng dẫn từng bước: [macOS](docs/INSTALL-macOS.md) · [Windows](docs/INSTALL-Windows.md).

Bảo mật: [SECURITY.md](SECURITY.md) · Quyền riêng tư: [PRIVACY.md](PRIVACY.md).

---

## English

**NANOMultipleGitGUI v1.0.0** — download it from [GitHub Releases](https://github.com/congluuit/NANOMultipleGitGUI/releases/latest) (Windows `.exe`/`.msi`, macOS `.dmg`, Linux). Install and use it right away: GitHub sign-in in the browser, automatic updates.

**NANOMultipleGitGUI** is an open-source Git GUI (Tauri 2 + Rust + React) for Windows and macOS whose main strength is handling **many GitHub/GitLab accounts at the same time**. Every repository knows which account it belongs to, so commits use the right identity and clone/push/LFS traffic uses the right credentials or SSH key — no profile switching, no `~/.ssh/config` host aliases, no credential clashes on the same host.

Quick start for contributors: install Node.js ≥ 20.19, pnpm 9, Rust stable ≥ 1.90 and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/), then run `pnpm install` and `pnpm dev`. Use `pnpm check` before opening a pull request. See [PLAN.md](PLAN.md) for the design and roadmap.

## License

[Apache License 2.0](LICENSE). Xem thêm [NOTICE](NOTICE).

NANOMultipleGitGUI là dự án độc lập, không liên kết với GitKraken hay Axosoft.

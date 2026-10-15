# PLAN — NANOMultipleGitGUI

> **NANOMultipleGitGUI** — Git GUI open-source (Apache-2.0), đủ tính năng như GitKraken, chạy trên Windows (`.exe` installer) và macOS (`.dmg`), **mạnh nhất ở việc quản lý nhiều tài khoản GitHub/GitLab cùng lúc** và chuyển tài khoản linh hoạt khi clone / commit / push.
>
> Mã ngắn dùng trong code, binary và biến môi trường: **`nanomgg`**.

---

## 0. Tiến độ thực hiện

| Bước | Nội dung | Trạng thái |
|---|---|---|
| 0.1 | Scaffold monorepo (Tauri 2 + React/TS + pnpm + Cargo workspace), file pháp lý, README, `.gitignore`, icon | ✅ Xong (2026-10-14) |
| 0.2 | Design tokens Light/Dark/System, Tailwind + Radix/shadcn, layout chính (khung), i18n vi/en, Zustand + TanStack Query | ✅ Xong (2026-10-14) |
| 0.3 | Typed IPC (`tauri-specta`), crate `storage` (SQLite + migrations), thư mục dữ liệu app, logging có redact | ✅ Xong (2026-10-14) |
| 0.4 | CI (`ci.yml`) + release (`release.yml`: `.exe`/MSI chưa ký, `.dmg` universal ad-hoc, Linux), thử `signCommand`, `INSTALL-*.md` | ✅ Xong — chờ repo GitHub để chạy thật |
| 0.5 | Tài liệu cộng đồng (CONTRIBUTING, CODE_OF_CONDUCT, SECURITY, PRIVACY, CHANGELOG) + ADR-001..004, kiểm tra tên | ✅ Xong (2026-10-14) |
| 0.6 | Prototype kiểm chứng: `nanomgg-credential` + 2 tài khoản + file LFS 200 MB; graph Canvas 50k/100k commit | ✅ Xong — bản với tài khoản GitHub thật chờ người dùng |
| 1.1 | Mở/init repo thật, danh sách gần đây, tab repo thật (lưu/khôi phục), kiểm tra git/git-lfs, watcher, sidebar ref thật | ✅ Xong (2026-10-14) |
| 1.2 | Commit graph thật: chi tiết commit, tìm kiếm, refresh tăng dần, bấm ref ở sidebar để nhảy tới commit | ✅ Xong (2026-10-14) |
| 1.3 | Working tree & commit: status, stage/unstage file/hunk/dòng, diff Monaco, commit/amend, discard; **menu chuột phải trên commit** | ✅ Xong (2026-10-14) |
| 1.4 | Branch & remote: tạo/checkout/đổi tên/xóa nhánh, merge, cherry-pick/revert/reset/tag, fetch/pull/push có tiến độ + hủy | ✅ Xong (2026-10-14) — xem hạn chế hủy hook trên Windows |
| 1.5 | Stash & LFS cơ bản: stash, panel LFS, `git lfs install --local`, Large File Guard trước commit | ✅ Xong (2026-10-14) |
| 2.1 | Lõi tài khoản: model, SQLite v3, keychain, rule engine, resolver account context, credential server + sidecar đóng gói | ✅ Xong (2026-10-14) |
| 2.2 | Đăng nhập GitHub (device flow, PAT) / GitLab (PKCE, device, PAT, self-hosted) + client API provider | ✅ Xong — OAuth cần client id của dự án (xem 13.2) |
| 2.3 | SSH key (tạo/nhập/ACL/upload/kiểm tra) + wizard nhập cấu hình có sẵn (`.gitconfig`, `.ssh/config`, gh, glab) | ✅ Xong (2026-10-14) |
| 2.4 | Truyền file LFS (ước lượng, tiến độ theo file, hàng đợi, hủy/tiếp tục, dịch lỗi) + Windows Job Object | ✅ Xong (2026-10-14) |
| 2.5 | Terminal tích hợp (xterm.js + portable-pty, inject account context) | ✅ Xong (2026-10-14) |
| 2.6 | UI tài khoản (quản lý, đăng nhập, account pill/switcher, command palette, phím tắt) + nối resolver vào mọi lệnh git | ✅ Xong (2026-10-14) |
| 2.7 | Clone browser đa tài khoản, Commit Guard & Push Guard | ✅ Xong (2026-10-14) |
| 2.8 | Kiểm thử end-to-end đa tài khoản trên bản cài release | ✅ Xong (2026-10-14) — v0.1 chờ repo GitHub + OAuth app (việc của người dùng) |
| 2.9 | Sửa sau v0.1: build tự đọc `.env`, scope `workflow` mặc định, lỗi push có hướng dẫn sửa + nút hành động | ✅ Xong (2026-10-14) |
| 3.1 | Rebase + interactive rebase kéo-thả, kéo-thả nhánh để merge/rebase, Undo/Redo theo reflog | ✅ Xong (2026-10-15) |
| 3.2 | Conflict editor 3-way (gồm binary/LFS), file history, blame, xem file tại commit, image diff | ✅ Xong (2026-10-15) |
| 3.3 | Submodule, worktree, LFS nâng cao (lock, danh sách file, prune/fsck), signing GPG/SSH + Verified | ✅ Xong (2026-10-15) — lock LFS chưa thử với GitHub/GitLab thật |
| 3.4 | Tích hợp + kiểm chứng end-to-end trên bản release, sửa lỗi phát hiện khi kiểm chứng | ✅ Xong (2026-10-15) |
| 3B.1 | Tự quét & thêm tài khoản từ `gh` / `glab` CLI (lấy token khi người dùng bấm Thêm) | ✅ Xong (2026-10-15) |
| 3B.2 | Onboarding 4 bước lần đầu chạy (giới thiệu, kiểm tra + cài công cụ, tài khoản, hoàn tất) | ✅ Xong (2026-10-15) |
| 3B.3 | Thư mục log / cache / dữ liệu tùy chỉnh, di chuyển có tiến độ, dọn dẹp, cảnh báo ổ đầy | ✅ Xong (2026-10-15) |
| 3B.4 | Check for Update theo GitHub Releases (stable/beta) | ✅ Xong (2026-10-15) — chờ repo GitHub + khóa ký minisign (việc của người dùng) |
| 3B.5 | Layout co giãn: kéo chia panel, thu gọn về icon, toolbar "⋯", dialog cuộn | ✅ Xong (2026-10-15) |
| 3B.6 | Tích hợp + kiểm chứng trên bản release | ✅ Xong (2026-10-15) |
| 3B.7 | Sửa theo góp ý: `.env.example`, nút Stage/Unstage/Hoàn tác dạng chữ, tự làm mới token GitHub 8 giờ, cột graph kéo đổi độ rộng + tooltip, English trước | ✅ Xong (2026-10-15) |
| 4.1 | PR / MR: danh sách, chi tiết, bình luận, review, tạo, checkout (cả từ fork), merge | ✅ Xong (2026-10-15) |
| 4.2 | Issues theo repo, tạo nhánh từ issue, `#123` trong commit, panel "Việc của tôi" gộp mọi tài khoản | ✅ Xong (2026-10-15) |
| 4.3 | Trạng thái CI (graph, chi tiết commit, status bar, PR), Fork & clone, fork repo đang mở | ✅ Xong (2026-10-15) |
| 4.4 | Tích hợp + kiểm chứng trên bản release với tài khoản GitHub thật (chỉ thao tác đọc) | ✅ Xong (2026-10-15) — thao tác ghi (tạo PR, merge, fork…) chưa thử thật |
| 5.x | Phase 5 — Hoàn thiện v1.0 | ⏭️ Tiếp theo |

Chi tiết từng bước và nhật ký: xem mục 10 (Phase 0, Phase 1).

**Việc người dùng cần làm** (không chặn Phase 1):
1. Tạo tài khoản/tổ chức GitHub cho dự án (gợi ý: org `nanomgg` — đang còn trống, khớp bundle identifier tạm) và push code; sau đó CI/Release chạy thật được.
2. Điền kênh liên hệ trong `CODE_OF_CONDUCT.md` (`[INSERT CONTACT METHOD]`) và thay `OWNER/REPO` trong `.github/release-notes.md`.
3. Nộp hồ sơ SignPath Foundation khi có bản release đầu tiên; cấu hình biến/secret theo hướng dẫn đầu file `release.yml`.
4. Bật tự cập nhật (3B.4): `pnpm tauri signer generate -w ~/.tauri/nanomgg-updater.key` → secret `TAURI_SIGNING_PRIVATE_KEY` (+ `_PASSWORD`), variable `NANOMGG_UPDATER_PUBKEY`; build cục bộ thêm `NANOMGG_GITHUB_REPO=owner/repo` (+ pubkey) vào `.env`. Chi tiết trong `docs/RELEASING.md`.
5. (Tùy chọn) Chạy thử prototype với 2 tài khoản GitHub thật — đã kiểm chứng đầy đủ bằng server cục bộ (mục 10, bước 0.6).

---

## 1. Tầm nhìn & điểm khác biệt

### 1.1 Vấn đề với GitKraken (và đa số Git GUI hiện nay)
- Mỗi profile chỉ gắn **1 tài khoản cho mỗi integration** (GitHub, GitLab...). Muốn dùng tài khoản khác phải đổi cả profile: tab, workspace và setting đều đổi theo.
- Git identity (`user.name` / `user.email`) tách rời khỏi tài khoản đăng nhập, nên rất dễ **commit nhầm email cá nhân vào repo công ty** (hoặc ngược lại).
- Credential HTTPS do Git Credential Manager lưu **mỗi host một bộ**, nên 2 tài khoản `github.com` sẽ giẫm lên nhau.
- SSH nhiều key phải tự chỉnh tay `~/.ssh/config` với host alias (`github.com-work`), sau đó phải sửa URL remote.
- Clone thì phải biết trước repo thuộc tài khoản nào; không có chỗ nào duyệt/tìm repo của tất cả tài khoản cùng lúc.
- File nặng: chỉ biết bị từ chối khi push xong (GitHub chặn file > 100 MB), lúc đó commit đã nằm trong lịch sử và rất khó gỡ.

### 1.2 Giải pháp của NANOMultipleGitGUI
| Khía cạnh | GitKraken | NANOMultipleGitGUI |
|---|---|---|
| Số tài khoản cùng provider dùng đồng thời | 1 mỗi profile | Không giới hạn, dùng song song |
| Gắn tài khoản theo repo | Không | Có (tự nhận diện + gán thủ công) |
| Identity commit đi theo tài khoản | Thủ công | Tự động, theo repo |
| Cảnh báo commit/push sai danh tính | Không | Có (Commit Guard & Push Guard) |
| HTTPS nhiều tài khoản trên cùng host | Xung đột | Credential helper riêng, chọn theo tài khoản |
| SSH nhiều key | Tự cấu hình | Tạo/nhập key theo tài khoản, tự inject, không cần host alias |
| Clone | Theo 1 tài khoản | Trình duyệt repo hợp nhất mọi tài khoản, tự chọn thư mục đích theo tài khoản |
| File nặng / LFS | Hỗ trợ LFS cơ bản | Large File Guard trước khi commit, kiểm tra trước khi push, upload LFS có tiến độ/hủy/tiếp tục, LFS đi đúng tài khoản |
| Terminal tích hợp | Dùng config global | Tự inject danh tính của repo đang mở |
| Self-hosted GitLab / GitHub Enterprise | Bản trả phí | Miễn phí, nhiều instance |

### 1.3 Nguyên tắc thiết kế
1. **Repo luôn biết mình thuộc tài khoản nào.** Mọi thao tác mạng (kể cả LFS) và commit đều được giải quyết theo "account context" của repo đó.
2. **Không phá cấu hình global của người dùng.** Mặc định inject cấu hình qua biến môi trường cho từng lệnh git; chỉ ghi vào `~/.gitconfig` hoặc `~/.ssh/config` khi người dùng chủ động bật.
3. **Bí mật nằm trong keychain của hệ điều hành**, không bao giờ lưu plaintext.
4. **Có thể đoán trước:** luôn hiển thị rõ "bạn đang thao tác với tư cách ai" và "sắp upload bao nhiêu dữ liệu" trước khi commit/push/clone.

---

## 2. Tech stack (đã chốt: Tauri 2)

| Lớp | Lựa chọn | Lý do |
|---|---|---|
| Desktop shell | **Tauri 2** | Bộ cài nhỏ (~10–20 MB so với ~150 MB của Electron), ít RAM, build sẵn ra NSIS `.exe` / MSI / `.dmg`, có updater và sidecar binary |
| Backend | **Rust** (stable, MSRV 1.90 — theo yêu cầu của tauri 2.12) | Hiệu năng cho graph/diff lớn, an toàn bộ nhớ, có crate cho keychain/SSH/HTTP |
| Frontend | **React 19 + TypeScript 6 + Vite 8** | Hệ sinh thái lớn, dễ thu hút contributor. TS ghim `~6.0` vì typescript-eslint chưa hỗ trợ TS 7 |
| UI kit | Tailwind CSS + Radix UI (shadcn/ui), theme bằng CSS variables | Accessible, dễ làm Light/Dark/System |
| State | Zustand (UI state) + TanStack Query (dữ liệu từ backend/API) | |
| Commit graph | Canvas 2D tự vẽ + virtualization (nâng lên WebGL nếu cần) | Mượt với repo 100k+ commit |
| Diff / Merge editor | Monaco Editor (diff editor có sẵn) | Có split/inline, syntax highlight; dùng cho cả 3-way merge |
| Terminal tích hợp | xterm.js + `portable-pty` (Rust) | |
| Git engine | **Git CLI hệ thống** cho thao tác ghi + **gitoxide (`gix`)** cho đọc nhanh (log/graph/status) | CLI đảm bảo tương thích hooks, LFS, signing, credential; gix giúp đọc nhanh mà không phải parse output |
| Git LFS | **`git-lfs` CLI** (≥ 3.0) | Chuẩn chung, GitHub/GitLab đều hỗ trợ; dùng chung credential helper của app |
| Lưu trữ | SQLite (`rusqlite` / `sqlx`) | accounts, repos, bindings, rules, settings |
| Secret | crate `keyring` (Windows Credential Manager, macOS Keychain, Linux Secret Service) | |
| API provider | `octocrab` (GitHub REST/GraphQL), client GitLab tự viết bằng `reqwest` | |
| SSH key | crate `ssh-key` (tạo ed25519), `ssh-agent` khi cần | |
| Typed IPC | `tauri-specta` sinh type TS từ Rust command | Tránh lệch kiểu giữa frontend và backend |
| i18n | `i18next` — Tiếng Việt + English từ đầu | |

### 2.1 Git & Git LFS binary
- Windows: phát hiện Git for Windows (đã kèm `git-lfs`); nếu thiếu thì đề nghị tải **MinGit** (bản portable, ~30 MB) vào thư mục app.
- macOS: dùng git của Xcode Command Line Tools hoặc Homebrew; nếu thiếu thì hướng dẫn `xcode-select --install`. **macOS thường không có sẵn `git-lfs`** → app đề nghị tải bản release chính thức của git-lfs (license MIT) vào thư mục dữ liệu app, kiểm tra SHA-256, hoặc hướng dẫn `brew install git-lfs`.
- Yêu cầu tối thiểu: git ≥ 2.35 (cần `--porcelain=v2`, `GIT_CONFIG_COUNT`, `safe.directory`), git-lfs ≥ 3.0.
- Luôn parse output máy đọc: `--porcelain=v2 -z`, `--format=` với ký tự phân tách NUL, `git lfs ... --json`.

---

## 3. Kiến trúc tổng thể

```
┌──────────────────────────── Frontend (React/TS) ────────────────────────────┐
│  Repo tabs │ Commit graph │ Staging/Diff │ Account switcher │ Clone browser │
│  LFS panel │ Transfer queue │ PR/MR │ Conflict editor │ Terminal │ Settings │
└───────────────────────────────▲─────────────────────────────────────────────┘
                                │ Tauri IPC (typed commands + events)
┌───────────────────────────────┴──────────── Backend (Rust) ─────────────────┐
│ core::repo_manager    – mở repo, watcher (notify), cache                    │
│ core::git_engine      – runner git CLI + gix reader, hàng đợi thao tác      │
│ core::lfs             – phát hiện file nặng, track/migrate, transfer+tiến độ │
│ core::accounts        – Account, Identity, rule engine, binding repo↔account │
│ core::auth            – OAuth device flow/PKCE, PAT, refresh, kiểm tra scope │
│ core::credentials     – keychain, IPC server cho credential helper           │
│ core::ssh             – tạo/nhập key, known_hosts, GIT_SSH_COMMAND            │
│ core::providers       – GitHub / GitLab (repo list, PR/MR, issues, CI)      │
│ core::storage         – SQLite + migrations                                 │
│ core::terminal        – PTY, inject env theo repo                           │
└───────────────────────────────▲─────────────────────────────────────────────┘
                                │ named pipe (Win) / unix socket (mac) + token
┌───────────────────────────────┴─────────────────────────────────────────────┐
│ nanomgg-credential (sidecar nhỏ) – được git và git-lfs gọi làm credential helper/askpass │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 3.1 Cấu trúc monorepo
```
/
├─ apps/
│  └─ desktop/
│     ├─ src/                 # React app
│     │  ├─ features/         # graph, staging, diff, accounts, clone, lfs, pr, settings...
│     │  ├─ components/       # UI dùng chung
│     │  ├─ theme/            # design tokens, light/dark, theme Monaco/xterm
│     │  ├─ ipc/              # bindings sinh bởi tauri-specta
│     │  └─ i18n/
│     └─ src-tauri/           # Tauri app (Rust), đăng ký commands
├─ crates/
│  ├─ git-engine/             # runner CLI + gix, parser
│  ├─ lfs/                    # large-file guard, LFS transfer, locks
│  ├─ accounts/               # model, rule engine, binding
│  ├─ auth/                   # OAuth, PAT, token lifecycle
│  ├─ credentials/            # keychain + IPC server
│  ├─ credential-helper/      # binary sidecar nanomgg-credential
│  ├─ ssh-keys/
│  ├─ providers/              # github, gitlab (trait chung `Provider`)
│  ├─ storage/                # SQLite + migrations
│  └─ logging/                # tracing + bộ lọc redact token (thêm ở bước 0.3)
├─ tools/
│  └─ test-git-server/        # server git smart-HTTP + LFS nhiều user, chỉ dùng cho test (bước 0.6)
├─ scripts/                   # sign-windows.ps1 (signCommand), gen-history-fixture.mjs (repo mẫu lớn)
├─ docs/                      # ADR, hướng dẫn cài đặt
├─ fixtures/                  # repo mẫu nhỏ cho test (repo lớn sinh bằng script vào target/fixtures/)
└─ .github/                   # workflows CI/release, issue/PR templates
```

---

## 4. Thiết kế Multi-Account (trọng tâm)

### 4.1 Mô hình dữ liệu
```
Provider      { id, kind: github|github_enterprise|gitlab|gitlab_self_hosted,
                base_url, api_url, oauth_client_id?,
                large_file_warn_mb, large_file_block_mb? }  # GitHub: 50 / 100 · GitLab: 50 / không chặn

Account       { id, provider_id, username, display_name, avatar_url,
                auth_method: oauth|pat, scopes[], token_expires_at,
                color, label ("Công ty", "Cá nhân"...), default_clone_dir }

Identity      { id, account_id, name, email,                # 1 account có thể có nhiều email
                signing: none|gpg|ssh, signing_key_ref }

SshKey        { id, account_id, type, fingerprint, public_key,
                private_key_ref (keychain hoặc file 0600), uploaded_to_provider: bool }

Repo          { id, path, display_name, last_opened_at }

RepoBinding   { repo_id, remote_name?, account_id, identity_id, transport: https|ssh,
                source: auto_rule|manual|clone, locked: bool }

Rule          { id, priority, match: { host?, owner_or_group?, path_glob?, remote_regex? },
                account_id, identity_id }

Workspace     { id, name, repo_ids[], default_account_id? }  # nhóm repo, không phải "profile"
```
Secret (token, refresh token, private key passphrase) lưu trong keychain với key `nanomgg:<account_id>:<kind>`. SQLite chỉ giữ tham chiếu.

### 4.2 Đăng nhập & quản lý tài khoản
- **GitHub.com:** OAuth **Device Flow** (không cần client secret, hợp với app open-source). Phương án dự phòng: Fine-grained PAT hoặc classic PAT.
- **GitHub Enterprise Server:** PAT, hoặc OAuth app do admin đăng ký (người dùng nhập client id).
- **GitLab.com:** OAuth Authorization Code + **PKCE** qua loopback redirect `http://127.0.0.1:<port>/callback`, hoặc Device Authorization Grant nếu instance hỗ trợ. Dự phòng: PAT.
- **GitLab self-hosted:** nhập base URL rồi chọn PAT hoặc OAuth app riêng. Hỗ trợ nhiều instance cùng lúc.
- **Import cấu hình có sẵn** (wizard lần đầu chạy):
  - Đọc `~/.gitconfig` (bao gồm các khối `includeIf "gitdir:..."`) để đề xuất Identity và Rule.
  - Đọc `~/.ssh/config` (host alias + IdentityFile) để đề xuất SshKey ↔ Account.
  - Đọc `gh` CLI `hosts.yml` và `glab` config để đề xuất tài khoản (vẫn yêu cầu người dùng xác nhận).
- **Sức khỏe token:** hiển thị hạn dùng và scope còn thiếu (ví dụ thiếu `repo` / `read_api`), tự refresh với GitLab OAuth, nhắc trước 7 ngày khi PAT sắp hết hạn.
- **Lưu ý GitHub org:** org có bật "OAuth App access restrictions" sẽ chặn OAuth app cho tới khi admin duyệt. UI cần phát hiện trường hợp này (API trả 403 hoặc thiếu repo) và gợi ý dùng fine-grained PAT. Về sau cân nhắc chuyển sang **GitHub App** để quyền chi tiết hơn.

### 4.3 Giải quyết account context cho một repo
Thứ tự ưu tiên khi mở repo:
1. `RepoBinding` có `locked = true` (người dùng đã chốt thủ công).
2. Binding được tạo lúc clone qua app.
3. **Rule engine**, khớp theo thứ tự priority:
   - host + owner/group của remote (ví dụ `github.com/acme-corp/*` → tài khoản công ty),
   - glob đường dẫn thư mục (ví dụ `D:/Work/**` → tài khoản công ty),
   - regex trên URL remote.
4. Heuristic: tài khoản nào có quyền truy cập repo (gọi API kiểm tra) — nếu chỉ một tài khoản có quyền thì đề xuất tài khoản đó.
5. Không xác định được: hiện banner **"Repo này chưa gắn tài khoản — chọn tài khoản"** và không chặn các thao tác chỉ đọc.

Remote khác nhau có thể gắn tài khoản khác nhau (ví dụ `origin` → GitLab công ty, `upstream` → GitHub cá nhân), nên binding thực tế được lưu ở mức `(repo, remote)`, với giá trị mặc định ở mức repo.

### 4.4 Inject credential khi chạy git (không đụng config global)
Mọi lệnh git chạy qua `GitRunner`, nhận `AccountContext`. Cấu hình được inject bằng **`GIT_CONFIG_COUNT` / `GIT_CONFIG_KEY_n` / `GIT_CONFIG_VALUE_n`** (git ≥ 2.31) thay vì `-c`, vì biến môi trường được **mọi process con kế thừa** — bao gồm `git-lfs` (được gọi qua hook pre-push và filter smudge/clean). Nhờ vậy upload/download LFS cũng đi đúng tài khoản.

**Danh tính commit**
```
user.name=<identity.name>   user.email=<identity.email>
[commit.gpgsign=true  gpg.format=ssh  user.signingkey=<key>]
```

**HTTPS**
```
credential.helper=                                  # tắt helper khác (GCM, osxkeychain)
credential.helper=<path>/nanomgg-credential
credential.useHttpPath=true
env: NANOMGG_SOCKET=<pipe>  NANOMGG_SESSION=<token ngẫu nhiên>  NANOMGG_ACCOUNT=<account_id>
     GIT_TERMINAL_PROMPT=0
```
`nanomgg-credential get` kết nối về app qua named pipe/unix socket, xác thực bằng session token rồi nhận `username/password(token)` của đúng tài khoản. Nhờ vậy 2 tài khoản cùng `github.com` không xung đột, và token **không bao giờ xuất hiện trong URL, trong `.git/config` hay trong danh sách process**.

Helper **chỉ trả token khi host được yêu cầu khớp với host của provider** của tài khoản đó. Điều này ngăn token GitHub bị gửi nhầm tới một LFS server bên thứ ba (khi repo cấu hình `lfs.url` trỏ sang host khác); trường hợp đó app hỏi người dùng chọn credential riêng.

**SSH**
```
env: GIT_SSH_COMMAND='ssh -i "<key_path>" -o IdentitiesOnly=yes -o IdentityAgent=none'
```
Không cần host alias `github.com-work`, URL remote giữ nguyên chuẩn `git@github.com:org/repo.git`. `git-lfs` cũng dùng `GIT_SSH_COMMAND` khi xác thực qua SSH (`git-lfs-authenticate`). Key có passphrase thì app hỏi qua `SSH_ASKPASS`, sidecar trả lời bằng dữ liệu lấy từ keychain.

**Tùy chọn "Ghi vào repo"** (đã chốt: **mặc định bật cho identity**)
Ghi `user.name`, `user.email`, `core.sshCommand` vào `.git/config` của repo, để git ngoài app (VS Code, CLI) cũng dùng đúng danh tính. Mặc định bật cho identity (vô hại, hữu ích) và cho phép tắt trong Settings (toàn cục) hoặc theo từng repo; mặc định tắt cho `core.sshCommand` (opt-in).

**Terminal tích hợp**
Tự export các biến môi trường ở trên để người dùng gõ `git push` trong terminal của app cũng đi đúng tài khoản.

### 4.5 Chuyển tài khoản linh hoạt
- **Account pill** ở thanh trên/status bar: avatar + màu + nhãn của tài khoản đang dùng cho repo hiện tại. Bấm vào sẽ mở danh sách tài khoản kèm identity.
- Hai chế độ chuyển:
  - **"Dùng cho repo này"**: cập nhật RepoBinding (có thể tick "Ghi nhớ" để khóa).
  - **"Chỉ cho thao tác này"**: commit/push một lần bằng tài khoản khác mà không đổi binding.
- **Command palette** (`Ctrl/Cmd+K`): `Switch account…`, `Commit as…`, `Push as…`, `Clone as…`.
- **Phím tắt** `Ctrl/Cmd+Alt+1..9` để chọn nhanh tài khoản theo thứ tự.
- Màu tài khoản được dùng để tô viền cửa sổ/tab repo, giúp nhận biết ngay đang ở ngữ cảnh "công ty" hay "cá nhân".

### 4.6 Commit Guard & Push Guard
Kiểm tra trước khi commit:
- Email của identity không khớp rule của repo (ví dụ repo thuộc `acme-corp` nhưng email là `@gmail.com`) → hiện cảnh báo vàng với các nút [Đổi sang tài khoản X] / [Vẫn commit].
- Email chưa được verify trên tài khoản provider (lấy danh sách qua API) → cảnh báo rằng commit sẽ không liên kết với profile.
- Repo yêu cầu signed commit (branch protection) mà identity chưa cấu hình signing → cảnh báo.
- **Large File Guard** (chi tiết ở mục 6.2): có file vượt ngưỡng mà chưa track bằng LFS.

Kiểm tra trước khi push:
- Liệt kê các commit sắp push có author/committer email khác identity hiện tại.
- Gợi ý **"Sửa tác giả"** (amend commit cuối, hoặc rebase sửa author cho N commit chưa push — chỉ áp dụng cho commit chưa push).
- Tài khoản không có quyền push vào remote (kiểm tra permission qua API) → báo lỗi sớm thay vì đợi git trả 403.
- **Có blob vượt giới hạn của provider trong các commit chưa push** (chi tiết ở mục 6.3).

Hỗ trợ thêm trailer `Co-authored-by:` từ danh sách tài khoản/đồng đội.

### 4.7 Clone đa tài khoản
```
┌ Clone repository ───────────────────────────────────────────────────────────┐
│ 🔍 Tìm repo trên mọi tài khoản...                 [Tài khoản: Tất cả ▾]     │
│ ─────────────────────────────────────────────────────────────────────────── │
│ ● Công ty (GitLab acme)   acme/backend-api        ★ 12   cập nhật 2 giờ    │
│ ● Công ty (GitLab acme)   acme/game-assets   LFS 4.2 GB  cập nhật 1 ngày   │
│ ● Cá nhân (GitHub)        hoangnv/dotfiles               cập nhật 3 ngày   │
│ ● OSS (GitHub)            some-org/lib    (fork của bạn: hoangnv/lib)      │
│ ─────────────────────────────────────────────────────────────────────────── │
│ Hoặc dán URL: [ https://...                    ] → tự đoán tài khoản        │
│ Tài khoản:  [● Công ty (GitLab acme) ▾]   Giao thức: (•) HTTPS ( ) SSH      │
│ Thư mục:    [ D:/Work/acme/game-assets        ]  ← theo default_clone_dir   │
│ ☑ Ghi identity vào .git/config   ☐ Clone submodules   ☐ Shallow (depth 1)  │
│ File LFS: (•) Tải hết  ( ) Tải sau  ( ) Chỉ tải: [ Assets/UI/**     ]       │
│                                                        [Hủy]  [Clone]      │
└─────────────────────────────────────────────────────────────────────────────┘
```
- Danh sách repo hợp nhất từ mọi tài khoản (có cache, refresh nền), lọc theo tài khoản/org/group, gồm cả repo được share và repo của org.
- Dán URL thì rule engine đoán tài khoản; nếu có nhiều tài khoản có quyền thì cho chọn.
- Thư mục đích mặc định theo tài khoản, có template (`{clone_dir}/{owner}/{repo}`).
- Sau khi clone: tạo RepoBinding với `source=clone` và tự thêm vào workspace tương ứng.
- **Fork & clone**: fork repo về tài khoản đã chọn, clone fork, rồi tự thêm `upstream`.

### 4.8 Bảo mật
- Token/passphrase chỉ nằm trong keychain; RAM xóa sau khi dùng (`zeroize`).
- Sidecar IPC: socket/pipe chỉ user hiện tại truy cập được, session token ngẫu nhiên mỗi lần app khởi động, kiểm tra PID cha nếu được.
- Helper kiểm tra host trước khi trả token (xem 4.4).
- Không log token; log có bộ lọc redact (`ghp_…`, `github_pat_…`, `glpat-…`, `Authorization:`).
- Private key do app tạo được lưu ở thư mục dữ liệu app, quyền `0600` (macOS) / ACL chỉ user (Windows), hoặc chỉ trong keychain.
- Telemetry: **mặc định tắt**, opt-in, ghi rõ trong `PRIVACY.md`.

---

## 5. Tính năng Git ngang GitKraken

### 5.1 Repo & workspace
- Mở / init / clone repo, danh sách gần đây, **tab nhiều repo**, Workspace (nhóm repo, có thể fetch tất cả).
- Theo dõi thay đổi file (watcher) và tự refresh; auto-fetch theo chu kỳ (cấu hình được).
- Hỗ trợ submodule, **worktree**, Git LFS (chi tiết ở mục 6).

### 5.2 Commit graph
- Graph theo lane có màu, nhãn branch/tag/remote/HEAD, avatar tác giả.
- Virtualized, mục tiêu mở 100k commit trong < 2s lần đầu, cuộn 60fps.
- Tìm kiếm theo message/author/SHA/file; lọc theo branch; ẩn/hiện branch; "solo" một branch.
- **Kéo-thả** branch lên branch để merge/rebase; menu chuột phải đầy đủ thao tác.

### 5.3 Working tree / staging
- Stage/unstage theo file, theo **hunk**, theo **dòng**; discard theo file/hunk/dòng.
- Hiển thị dung lượng file và badge **"Nặng"** / **"LFS"** ngay trong danh sách file.
- Soạn commit message (summary + body, đếm ký tự, template), amend, sign-off.
- **Stash**: tạo (gồm cả untracked), apply, pop, drop, xem nội dung.

### 5.4 Diff viewer
- Inline / split, syntax highlight, word-diff, bỏ qua whitespace.
- Image diff (side-by-side, swipe, onion skin), phát hiện file binary; file LFS hiển thị được nội dung thật (tự tải object nếu cần) kèm thông tin pointer (oid, size).
- File history, **blame**, xem file tại một commit bất kỳ.

### 5.5 Branch & lịch sử
- Tạo / checkout / đổi tên / xóa branch (local & remote), set upstream.
- Merge (ff, no-ff, squash), rebase, **interactive rebase** dạng kéo-thả (pick/reword/squash/fixup/drop/edit).
- Cherry-pick, revert, reset (soft/mixed/hard), tag (lightweight/annotated, push tag).
- **Undo/Redo** dựa trên reflog cho các thao tác gần nhất.

### 5.6 Remote
- Fetch / pull (merge, rebase, ff-only) / push / **force-with-lease**, nhiều remote (mỗi remote có thể dùng tài khoản riêng).
- Tiến độ cho push/fetch thường (parse `--progress`: counting / compressing / writing objects), có nút hủy.
- Hiển thị ahead/behind, trạng thái CI của commit (GitHub Actions / GitLab CI).

### 5.7 Merge conflict
- Danh sách file xung đột; editor **3-way** (ours / theirs / kết quả), chọn theo khối, sửa tay; đánh dấu đã giải quyết; abort/continue merge/rebase/cherry-pick.
- Xung đột file binary/LFS: chọn "giữ bản của tôi" / "giữ bản của họ", có xem trước ảnh.

### 5.8 Tích hợp provider (theo tài khoản)
- **PR (GitHub) / MR (GitLab)**: danh sách, tạo PR từ branch, xem diff, comment, review (approve/request changes), checkout PR branch, merge.
- Issues: xem, tạo, liên kết vào commit/branch (`feature/123-...`).
- Panel "Của tôi": PR/MR cần review **trên tất cả tài khoản**, gom về một chỗ.

### 5.9 Khác
- Terminal tích hợp (inject account context), command palette, phím tắt tùy biến.
- Theme Light / Dark / System (mục 7.2), i18n Việt/Anh.
- Commit signing GPG & SSH; hiển thị trạng thái Verified.
- Hooks chạy bình thường (vì dùng git CLI), hiển thị output khi hook fail.
- Auto-update.

---

## 6. Git LFS & xử lý file nặng

Mục tiêu: người dùng **không bao giờ phải tự gỡ một file 300 MB khỏi lịch sử** chỉ vì push bị từ chối. App phát hiện sớm, đề xuất LFS bằng một cú bấm, và upload file nặng có tiến độ rõ ràng, hủy được, tiếp tục được, đi đúng tài khoản.

### 6.1 Phát hiện & thiết lập
- Khi mở repo: kiểm tra `git lfs version`, đọc `.gitattributes` để biết pattern nào đang `filter=lfs`, kiểm tra hook LFS đã cài trong repo chưa.
- Repo có `.gitattributes` dùng LFS mà máy chưa có git-lfs → banner "Repo này dùng Git LFS" kèm nút [Cài git-lfs] (mục 2.1).
- Repo dùng LFS nhưng chưa cài hook → chạy `git lfs install --local` (chỉ trong repo đó, không đụng global).
- GitLab project tắt LFS (server trả "LFS is disabled") → giải thích và chỉ đường tới Settings của project.

### 6.2 Large File Guard (trước khi commit)
- Ngưỡng theo provider, cấu hình được trong Settings và ghi đè được theo repo. Mặc định cho GitHub: **cảnh báo > 50 MB, chặn > 100 MB** (đúng giới hạn của GitHub). Mặc định cho GitLab (gitlab.com và self-hosted, đã chốt): **cảnh báo > 50 MB, không chặn cứng** (vì giới hạn khác nhau theo instance); người dùng có thể tự đặt ngưỡng chặn cho từng instance.
- Ngoài kích thước, app nhận diện thêm file binary theo đuôi phổ biến (`.psd`, `.zip`, `.mp4`, `.fbx`, `.unitypackage`, `.dll`...) và đề xuất LFS ngay cả khi file chưa vượt ngưỡng.
- Khi bấm Commit mà có file vượt ngưỡng, app hiện hộp thoại:
  ```
  ┌ 3 file vượt ngưỡng 50 MB ─────────────────────────────────────────┐
  │  Assets/intro.mp4        312 MB                                   │
  │  Design/home.psd          88 MB                                   │
  │  build/app.zip           140 MB   (có vẻ là file build)           │
  │                                                                   │
  │  Cách track: (•) Theo đuôi file (*.mp4, *.psd)  ( ) Theo đường dẫn │
  │  [Track bằng LFS (khuyến nghị)]  [Thêm vào .gitignore]  [Vẫn commit]│
  └───────────────────────────────────────────────────────────────────┘
  ```
- "Track bằng LFS": chạy `git lfs track "<pattern>"`, stage `.gitattributes`, rồi stage lại file để chúng thành pointer. File đã từng được git track thì dùng `git add --renormalize`.
- "Vẫn commit" bị vô hiệu với file vượt ngưỡng chặn của provider (vì chắc chắn push sẽ thất bại), trừ khi người dùng tắt hẳn guard cho repo.

### 6.3 Kiểm tra trước khi push (file nặng đã lỡ commit)
- Quét các object trong khoảng commit chưa push:
  `git rev-list --objects <remote>/<branch>..HEAD` → `git cat-file --batch-check` để tìm blob vượt giới hạn.
- Nếu có: hiển thị file nào, ở commit nào, và đề xuất **"Chuyển sang LFS"**:
  `git lfs migrate import --include="<paths>" --include-ref=refs/heads/<branch> --exclude-ref=refs/remotes/<remote>/<branch>`
  → chỉ viết lại các commit **chưa push**, không đụng lịch sử đã chia sẻ.
- Trước khi viết lại, luôn tạo ref dự phòng `refs/nanomgg/backup/<timestamp>` và cho phép Undo.
- Nếu blob nặng nằm trong commit **đã push** thì app chỉ giải thích và hướng dẫn, không tự viết lại lịch sử chung.

### 6.4 Upload file nặng (push LFS)
Push được tách làm 2 bước để hiển thị tiến độ chính xác:
1. **Ước lượng**: `git lfs push --dry-run <remote> <ref>` để biết danh sách object và tổng dung lượng → hiển thị trước: "Sẽ upload 12 file LFS · 1.8 GB · bằng tài khoản ● Công ty".
2. **Upload LFS**: `git lfs push <remote> <ref>` với `GIT_LFS_PROGRESS=<file tạm>`. App đọc file tiến độ (mỗi dòng có số file đã xong/tổng, số byte đã gửi/tổng, tên file) để vẽ:
   - thanh tiến độ tổng + từng file, tốc độ, thời gian còn lại;
   - **hàng đợi transfer** ở status bar, chạy nền, không khóa UI; có thể chạy song song nhiều repo.
3. **Push git**: `git push` như bình thường. Hook pre-push của LFS thấy object đã có trên server nên bỏ qua, push xong rất nhanh.

Hủy & tiếp tục:
- Nút **Hủy** dừng process. LFS lưu object theo hash nội dung, nên những file đã upload xong vẫn nằm trên server; lần push sau chỉ upload phần còn thiếu → tiếp tục ở mức từng file.
- Mất mạng thì git-lfs tự retry; app thêm nút [Thử lại] và tự retry khi có mạng trở lại.

Tinh chỉnh (Settings → LFS, ghi đè được theo repo): số luồng song song (`lfs.concurrenttransfers`), số lần retry (`lfs.transfer.maxretries`), timeout (`lfs.activitytimeout`).

Dịch lỗi sang ngôn ngữ dễ hiểu:
| Lỗi từ server | App hiển thị |
|---|---|
| 401 / 403 | "Tài khoản ● Cá nhân không có quyền ghi LFS vào repo này" + nút [Đổi tài khoản] |
| Vượt quota storage/bandwidth LFS | "Đã hết dung lượng LFS của tài khoản/org" + link tới trang billing của provider đó |
| Object vượt kích thước tối đa cho phép | Tên file + giới hạn + gợi ý tách nhỏ hoặc dùng nơi lưu khác |
| LFS bị tắt trên project | Hướng dẫn bật trong Settings của project (GitLab) |
| Lỗi mạng / timeout | Nút [Thử lại], gợi ý giảm số luồng song song |

### 6.5 Tải file LFS (clone / pull / checkout)
- Lúc clone có 3 lựa chọn (xem hộp thoại ở mục 4.7):
  - **Tải hết**: như mặc định của git-lfs nhưng có tiến độ.
  - **Tải sau**: clone với `GIT_LFS_SKIP_SMUDGE=1` (clone rất nhanh), sau đó chạy `git lfs pull` ở nền với tiến độ.
  - **Chỉ tải một phần**: đặt `lfs.fetchinclude` / `lfs.fetchexclude` theo pattern (hữu ích cho repo game/thiết kế hàng chục GB).
- Kiểm tra dung lượng ổ đĩa còn trống trước khi tải và cảnh báo nếu không đủ.
- File chưa tải được hiển thị là "pointer" kèm nút **[Tải file này]** (`git lfs pull --include=<path>`).

### 6.6 LFS Lock (làm việc nhóm với file binary)
- Hiển thị khóa: `git lfs locks --json`, kèm tên người đang giữ khóa ngay trong danh sách file.
- Lock / Unlock / Force unlock (nếu có quyền) bằng chuột phải.
- Pattern đánh dấu `lockable` trong `.gitattributes`: file sẽ chỉ-đọc cho tới khi người dùng lock; app cảnh báo nếu sửa file đang bị người khác khóa.
- Lock gọi API dưới danh nghĩa **tài khoản đang gắn với repo**.

### 6.7 LFS panel & bảo trì
- Danh sách pattern đang track (thêm/xóa/đánh dấu lockable), danh sách file LFS kèm dung lượng và trạng thái (đã tải / pointer / bị khóa).
- Tổng dung lượng LFS cục bộ; **`git lfs prune`** để giải phóng ổ đĩa (hiển thị trước dung lượng sẽ giải phóng).
- `git lfs fsck` để kiểm tra toàn vẹn; `git lfs fetch --all` khi cần đầy đủ dữ liệu.

---

## 7. Giao diện

### 7.1 Bố cục chính (phác thảo)
```
┌─[● Công ty ▾]──[backend-api ×][game-assets ×][+]──────────── 🔍 Ctrl+K ─────┐
│ Fetch  Pull▾  Push▾  Branch  Stash  Pop  │ Undo Redo │          Terminal ⚙   │
├───────────────┬─────────────────────────────────────────┬────────────────────┤
│ LOCAL         │  ●─┐  feat: add login     hoangnv  2h   │ Commit / WIP       │
│  ▸ main  ↑1   │  │ ●  fix: token refresh  hoangnv  3h   │ Unstaged (3)       │
│  ▸ feature/x  │  ●─┘  Merge branch ...    lan     1d   │  M src/a.ts        │
│ REMOTE        │  ●    chore: deps         bot     2d   │  A intro.mp4 [LFS] │
│  ▸ origin     │  ...  (virtualized graph)               │ Staged (1)         │
│ PR / MR       │                                         │  A src/b.ts        │
│ TAGS          │                                         │ ┌────────────────┐ │
│ STASHES       │                                         │ │ Summary        │ │
│ LFS           │                                         │ │ Body...        │ │
│ SUBMODULES    │                                         │ └────────────────┘ │
│ WORKTREES     │                                         │ Commit as:         │
│               │                                         │ ● Hoang <hoang@acme>│
│               │                                         │ [ Commit (Ctrl+⏎) ]│
├───────────────┴─────────────────────────────────────────┴────────────────────┤
│ ● Công ty · GitLab acme · HTTPS │ main ↑1 ↓0 │ ✓ CI │ ⇅ LFS 42% · 760 MB/1.8 GB │
└──────────────────────────────────────────────────────────────────────────────┘
```
Viền trên của cửa sổ/tab được tô theo màu tài khoản đang dùng. Mục "Commit as" luôn hiển thị danh tính sẽ được dùng ngay cạnh nút Commit. Status bar hiển thị hàng đợi upload/download LFS.

### 7.2 Theme: Light / Dark / System
- Ba lựa chọn trong Settings và trên command palette (`Theme: Light/Dark/System`). **System** theo `prefers-color-scheme` và sự kiện đổi theme của hệ điều hành (Tauri `onThemeChanged`), đổi ngay không cần khởi động lại.
- Toàn bộ màu được định nghĩa bằng **design tokens** (CSS variables) trong `src/theme/`; Monaco, xterm.js và graph canvas đọc cùng bộ token nên luôn đồng bộ.
- Đạt tương phản WCAG AA cho chữ; có kế hoạch thêm theme High Contrast sau v1.0.

**Định hướng màu — khác hẳn GitKraken.** GitKraken dùng nền xanh than lạnh với điểm nhấn xanh ngọc/teal. NANOMultipleGitGUI đi theo hướng ngược lại: **nền xám ấm (stone/sand) + điểm nhấn cam hổ phách (ember)**, không dùng teal làm màu chủ đạo.

| Token | Light | Dark |
|---|---|---|
| `--bg` (nền app) | `#FAF8F5` | `#16130F` |
| `--surface` (panel) | `#FFFFFF` | `#1F1B17` |
| `--surface-2` (hover, sidebar) | `#F2EEE8` | `#2A251F` |
| `--border` | `#E4DED5` | `#3A332B` |
| `--text` | `#1F1B16` | `#F1ECE4` |
| `--text-muted` | `#6B635A` | `#A39A8F` |
| `--primary` (nút chính, focus) | `#C2410C` | `#FF8A3D` |
| `--primary-hover` | `#A63E00` | `#FFA463` |
| `--primary-fg` (chữ trên nút chính) | `#FFFFFF` | `#1F1B16` |
| `--success` | `#237A35` | `#51CF66` |
| `--warning` | `#8F5B00` | `#FCC419` |
| `--danger` | `#C92A2A` | `#FF6B6B` |
| `--info` | `#1A68B0` | `#4DABF7` |

- **Đã chốt ở bước 0.2** (nguồn duy nhất: `apps/desktop/src/theme/tokens.css`, kiểm tra tự động bởi `contrast.test.ts`, xem trực quan ở Settings → "Xem bảng màu"). So với bản khởi đầu, bản Light đổi sang sắc đậm hơn vì bản cũ không đạt AA: chữ trắng trên `#E8590C` chỉ 3,58:1; success/warning/info dưới 4,5:1 trên nền. Bản Dark giữ nguyên, thêm `--primary-fg` tối vì chữ trắng trên `#FF8A3D` chỉ 2,35:1.
- Quy tắc tương phản được test: chữ thường và chữ phụ ≥ 4,5:1 trên `bg`/`surface`/`surface-2`; chữ trên nút chính ≥ 4,5:1; màu primary/ngữ nghĩa ≥ 4,5:1 trên `bg`/`surface` và ≥ 3:1 trên `surface-2`; màu lane ≥ 3:1 (đối tượng đồ họa).
- **Màu lane của graph** (8 màu, xoay vòng) — Dark: cam `#FF8A3D`, xanh dương `#4DABF7`, tím `#B197FC`, xanh lá `#69DB7C`, hồng `#F783AC`, vàng `#FFD43B`, xanh lime `#A9E34B`, đỏ gạch `#E8590C`. Light (sắc đậm cùng tông): `#D9480F`, `#1971C2`, `#7048E8`, `#2F9E44`, `#C2255C`, `#B35F00`, `#5C940D`, `#C92A2A`.
- **Màu tài khoản** là bảng riêng (người dùng chọn), được kiểm tra để không trùng với màu ngữ nghĩa (success/danger) và đủ khác nhau giữa các tài khoản.
- Logo, icon app và bộ icon UI tự thiết kế (dùng bộ icon open-source như Lucide), không lấy hình con bạch tuộc hay bất kỳ asset nào của GitKraken.

---

## 8. Đóng gói & phát hành

### 8.1 Định dạng
| OS | Đầu ra | Ghi chú |
|---|---|---|
| Windows 10/11 x64 (+ ARM64 sau) | **NSIS `.exe` installer** (chính), MSI (cho doanh nghiệp), bản portable `.zip` | WebView2 được bootstrap tự động |
| macOS 11+ | **`.dmg`** chứa `.app` **universal** (Apple Silicon + Intel) | Không ký Developer ID, chỉ ký ad-hoc (xem 8.2) |
| Linux (bonus) | AppImage, `.deb`, `.rpm` | Tauri build sẵn, chi phí thấp |

### 8.2 macOS — không ký số, hướng dẫn người dùng cho phép cài
- Build với **chữ ký ad-hoc** (`bundle.macOS.signingIdentity: "-"`). Không có chữ ký nào thì trên Apple Silicon macOS sẽ báo "app bị hỏng" thay vì cho phép mở; ad-hoc thì miễn phí và tránh được lỗi này.
- Viết `docs/INSTALL-macOS.md` (có ảnh chụp từng bước, Việt + Anh) và link ngay trong README, trang Release:
  1. Mở file `.dmg`, kéo **NANOMultipleGitGUI** vào thư mục **Applications**.
  2. Mở app lần đầu. macOS sẽ chặn vì "không xác minh được nhà phát triển".
     - **macOS 14 trở xuống:** chuột phải vào app → **Open** → bấm **Open** lần nữa.
     - **macOS 15 trở lên:** vào **System Settings → Privacy & Security**, kéo xuống phần Security, bấm **Open Anyway**, nhập mật khẩu, rồi mở lại app.
  3. Nếu vẫn báo **"app is damaged and can't be opened"**, mở Terminal và chạy:
     `xattr -cr /Applications/NANOMultipleGitGUI.app`
- Chỉ cần làm một lần. Bản cập nhật tải qua auto-updater của app không bị gắn cờ quarantine nên không phải cho phép lại.
- Lưu ý: vì chữ ký ad-hoc thay đổi sau mỗi bản build, macOS có thể **hỏi lại quyền truy cập Keychain** sau khi cập nhật. Hướng dẫn người dùng chọn "Always Allow"; ghi rõ trong FAQ.
- Về sau nếu dự án có tài trợ thì có thể mua Apple Developer (99 USD/năm) để ký và notarize — không bắt buộc.

### 8.3 Windows — ký số miễn phí qua SignPath Foundation
- Đăng ký chương trình **SignPath Foundation** dành cho dự án open-source. Điều kiện chính: license OSI (Apache-2.0 đạt), repo public, build hoàn toàn trên CI (GitHub Actions), không có malware. Hồ sơ cần thời gian duyệt — nộp ngay khi có bản release đầu tiên.
- Publisher hiển thị trên chứng chỉ sẽ là **SignPath Foundation** (đặc điểm của chương trình OSS).
- Tích hợp: dùng action `signpath/github-action-submit-signing-request`. Cần ký cả file `.exe` của app bên trong lẫn file installer NSIS bên ngoài: build app → gửi SignPath ký exe → đóng gói NSIS → gửi SignPath ký installer. Kiểm chứng luồng này bằng Tauri `bundle.windows.signCommand` ngay ở Phase 0.
- **Trong lúc chờ duyệt**, phát hành bản chưa ký kèm `docs/INSTALL-Windows.md`: khi SmartScreen hiện "Windows protected your PC", bấm **More info → Run anyway**.
- Kể cả khi đã ký, SmartScreen vẫn có thể cảnh báo ở vài bản đầu cho tới khi chứng chỉ tích lũy đủ "uy tín" — ghi chú điều này trong FAQ.

### 8.4 Auto-update
- `tauri-plugin-updater` đọc `latest.json` trên GitHub Releases.
- Gói cập nhật được ký bằng khóa **minisign** của Tauri (miễn phí, độc lập với việc ký số OS); private key lưu trong GitHub Secrets.
- Hai kênh: `stable` và `beta` (chọn trong Settings).

### 8.5 CI/CD (GitHub Actions)
- `ci.yml` (mỗi PR): `cargo fmt --check`, `clippy -D warnings`, `cargo test`, `pnpm lint`, `pnpm typecheck`, `vitest`; chạy matrix Windows/macOS/Linux.
- `release.yml` (khi push tag `v*`): dùng `tauri-apps/tauri-action` build Windows x64, macOS universal (ad-hoc), Linux; ký Windows qua SignPath; tạo **GitHub Release** dạng draft kèm `latest.json`, file hướng dẫn cài đặt và checksum SHA-256.
- Tùy chọn sau này: winget manifest, Scoop bucket, Homebrew cask.

---

## 9. Open-source & cộng đồng
- **License: Apache-2.0** (đã chốt). File `LICENSE` + `NOTICE`; header license ngắn ở đầu file nguồn (tùy chọn).
- Các file cần có: `README` (Việt + Anh, kèm ảnh chụp/GIF, link hướng dẫn cài macOS/Windows), `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md` (kênh báo lỗ hổng riêng), `PRIVACY.md`, `CHANGELOG.md` (Conventional Commits + release-please).
- Issue/PR templates, nhãn `good first issue`, GitHub Discussions, roadmap công khai bằng GitHub Projects.
- ADR (Architecture Decision Records) trong `docs/adr/` cho các quyết định lớn: Tauri, git CLI, credential helper, LFS transfer...
- **Pháp lý:** không dùng tên, logo, asset, bảng màu hay mã của GitKraken. Chỉ lấy cảm hứng về tính năng; UI và màu sắc tự thiết kế (mục 7.2). Trong branding nói "thay thế cho GitKraken", không nói "clone GitKraken".
- Kiểm tra tên "NANOMultipleGitGUI" chưa bị dùng trên GitHub, crates.io, npm và các tên miền dự định.
- OAuth client ID của GitHub/GitLab cho bản chính thức được đăng ký dưới tổ chức của dự án. Device flow và PKCE không cần client secret nên có thể public client ID.
- Bundle identifier: `io.github.<owner>.nanomultiplegitgui` (thay `<owner>` bằng tài khoản/tổ chức GitHub của dự án). **Hiện dùng tạm `io.github.nanomgg.nanomultiplegitgui`** trong `apps/desktop/src-tauri/tauri.conf.json`; phải đổi trước bản alpha đầu tiên vì identifier quyết định thư mục dữ liệu app.

---

## 10. Lộ trình (roadmap)

> Ước lượng cho 1–2 dev làm part-time/full-time; điều chỉnh theo thực tế.

### Phase 0 — Nền móng (1–2 tuần)
- Scaffold monorepo Tauri 2 + React + TS, pnpm, Cargo workspace.
- CI lint/test và **pipeline release ra `.exe`/`.dmg` ngay từ đầu** (kể cả khi app chưa có gì): macOS ad-hoc, Windows chưa ký; thử nghiệm luồng `signCommand` cho SignPath.
- Design tokens Light/Dark/System + mockup bảng màu; khung SQLite + migrations, khung IPC typed, i18n, layout chính.
- ✅ Kết quả: cài được app rỗng trên Windows và macOS theo đúng hướng dẫn cài đặt.

#### Các bước của Phase 0
**0.1 — Scaffold monorepo ✅ (2026-10-14)**
- Root: `LICENSE` (Apache-2.0, bản gốc từ apache.org), `NOTICE`, `README.md`, `.gitignore` (gồm `.agent/` — thư mục riêng cho agent, xóa khi hoàn tất dự án), `.gitattributes` (LF), `.editorconfig`, Prettier, `rust-toolchain.toml` (stable + rustfmt + clippy), `.vscode/extensions.json`.
- `pnpm-workspace.yaml` (`apps/*`) + `package.json` gốc với script: `dev`, `dev:web`, `build`, `tauri`, `lint`, `typecheck`, `format(:check)`, `rust:check`, `rust:test`, `check` (chạy tất cả).
- Cargo workspace (`resolver = "3"`, edition 2024, MSRV 1.90, `[workspace.lints]`, profile release tối ưu kích thước). 9 crate stub theo mục 3.1: `nanomgg-git-engine`, `-lfs`, `-accounts`, `-auth`, `-credentials`, `-credential-helper` (binary `nanomgg-credential`), `-ssh-keys`, `-providers`, `-storage`.
- `apps/desktop`: React 19 + TS 6 (strict, `noUncheckedIndexedAccess`) + Vite 8 + ESLint 10 (typescript-eslint, react-hooks, react-refresh). Màn hình splash tạm gọi command Rust `app_info` để kiểm chứng IPC.
- `apps/desktop/src-tauri`: crate `nanomgg` (lib `nanomgg_lib`), binary `nanomgg`, productName `NANOMultipleGitGUI`, cửa sổ 1280×800 (tối thiểu 960×600), CSP chặt (không `csp: null`), bundle NSIS (cài theo user) / MSI / app / dmg (ad-hoc `signingIdentity: "-"`, macOS ≥ 11) / AppImage / deb / rpm, version lấy từ `package.json`.
- Logo/icon: ban đầu là bản tự vẽ (`logo.svg`). **Ở bước 1.3 đã thay bằng ảnh của dự án** (`icon-generated.png`): cắt theo ô bo góc, nền ngoài trong suốt, master 1024×1024 tại `apps/desktop/src-tauri/icons/app-icon.png`; bộ icon sinh lại bằng `pnpm tauri icon` (ico 16–256, icns, png), logo trong app là `apps/desktop/public/logo.png`. Bộ icon cũ và `logo.svg` đã xóa.
- Kiểm chứng trên Windows 10: `pnpm format:check`, `lint`, `typecheck`, `cargo fmt --check`, `clippy -D warnings`, `cargo test` đều qua; `pnpm dev` mở được app; `tauri build` ra `NANOMultipleGitGUI_0.0.1_x64-setup.exe` (1,3 MB) và `.msi` (2,0 MB); bản release chạy được và IPC trả đúng version/OS. Chưa kiểm chứng `.dmg` (cần máy macOS hoặc CI ở bước 0.4).

**0.2 — Giao diện nền ✅ (2026-10-14)**
- Tailwind CSS v4 (`@tailwindcss/vite`) + `tw-animate-css`. Token tại `src/theme/tokens.css`, ánh xạ sang tên class của shadcn trong `src/index.css`; dark mode theo `[data-theme]` nên có thể đặt theme lồng nhau (dùng cho trang bảng màu).
- Theme Light/Dark/System (`src/theme/useThemeSync.ts`): theo `prefers-color-scheme` + Tauri `onThemeChanged`, đồng bộ cả thanh tiêu đề native (`window.setTheme`), đổi ngay không cần khởi động lại; theme áp dụng trước lần vẽ đầu tiên để không nháy màu.
- shadcn/ui (style new-york, gói `radix-ui`) chép vào `src/components/ui`: Button, Dialog, DropdownMenu, Tooltip, Select, ToggleGroup, Separator, ScrollArea; icon Lucide.
- Layout khung theo mục 7.1 (`src/features/shell`): account pill (đổi tài khoản theo repo, dữ liệu mẫu), tab repo tô màu theo tài khoản, viền trên cửa sổ theo màu tài khoản, toolbar, sidebar (Local/Remote/PR/Tags/Stashes/LFS/Submodules/Worktrees), panel Commit (badge "Nặng"/"LFS", đếm 72 ký tự, khối "Commit với tư cách"), status bar (tài khoản · provider · giao thức, ahead/behind, CI, tiến độ LFS).
- Dialog Settings: chọn theme, ngôn ngữ, xem phiên bản; trang "Bảng màu & design tokens" hiển thị song song Light/Dark kèm bảng kiểm tra tương phản.
- i18next vi/en có kiểu khóa (`i18next.d.ts`); test bảo đảm 2 ngôn ngữ cùng bộ khóa và cùng biến nội suy.
- Zustand (`stores/settings.ts`, `stores/ui.ts`) + TanStack Query. Vitest + Testing Library: 65 test tương phản + test theme/i18n/format + smoke test App (render layout, đổi theme, đổi ngôn ngữ, mở Settings).
- **Đã chốt bảng màu** (mục 7.2): bản Light đổi sang sắc đậm hơn để đạt WCAG AA.
- Tham số URL chỉ có trong dev (`?theme=dark&lang=en&view=palette`) để chụp màn hình nhanh khi chạy `pnpm dev:web`.

**0.3 — IPC typed & lưu trữ ✅ (2026-10-14)**
- `tauri-specta =2.0.0-rc.25` (pre-1.0, ghim chính xác), chế độ lỗi Throw: binding sinh vào `src/ipc/bindings.ts` mỗi lần chạy `cargo test` hoặc bản debug; CI báo lỗi nếu binding đã commit bị cũ. Lỗi command có kiểu `AppError { kind, message }` (message đã qua redact).
- Crate `storage`: `rusqlite` (bundled SQLite), WAL, migrations theo `PRAGMA user_version` trong 1 transaction, từ chối DB của bản app mới hơn; bảng `settings` (giá trị JSON, `STRICT`). File DB: `%APPDATA%/<identifier>/nanomgg.db`.
- Settings (theme, ngôn ngữ) lưu ở SQLite qua `get_settings`/`update_settings`; localStorage chỉ còn là cache để lần vẽ đầu có sẵn theme. Đã kiểm chứng: ghi `theme=dark`, `language=en` thẳng vào DB → app mở lên tối + tiếng Anh, thanh tiêu đề native cũng tối.
- Crate mới `logging`: `tracing` + file xoay vòng hằng ngày (giữ 14 file) tại `%LOCALAPPDATA%/<identifier>/logs`, console ở bản debug, lọc bằng `NANOMGG_LOG`. Mọi dòng log đi qua `redact()`: token GitHub (`ghp_/gho_/ghu_/ghs_/ghr_`, `github_pat_`), GitLab (`glpat-`, `gloas-`, `gldt-`…), `Authorization:`, `Bearer/Basic/token …`, `password=` (giao thức credential), mật khẩu trong URL.
- Windows: test binary link plugin dialog cần manifest Common-Controls v6 → `build.rs` tự nhúng manifest cho mọi target (thay vì để tauri-build chỉ nhúng vào binary chính).

**0.4 — CI/CD & hướng dẫn cài ✅ (2026-10-14, chờ repo GitHub để chạy thật)**
- `ci.yml`: job frontend (format, lint, typecheck, test, build) + job Rust matrix Ubuntu/Windows/macOS (fmt, clippy `-D warnings`, test, kiểm tra binding không bị cũ).
- `release.yml` (tag `v*` hoặc chạy tay): tạo draft Release → build macOS universal (ad-hoc) + Linux (AppImage/deb/rpm) bằng `tauri-action@v1` → Windows chưa ký (mặc định) **hoặc** Windows ký qua SignPath (bật bằng biến `SIGNPATH_ENABLED=true`) → `SHA256SUMS.txt`. Đã kiểm tra bằng actionlint 1.7.12: không lỗi.
- Luồng ký SignPath cho OSS gồm 2 vòng: build exe (`tauri build --no-bundle`) → ký exe → đóng gói NSIS/MSI quanh exe đã ký (`tauri bundle`) → ký installer. Đã chạy thử 2 pha này trên máy: `tauri bundle` không biên dịch lại.
- `scripts/sign-windows.ps1` gắn vào `bundle.windows.signCommand`. Kiểm chứng: Tauri gọi hook **12 lần/build** (exe chính, plugin NSIS, uninstaller, installer NSIS, extension WiX, MSI). Script tự bỏ qua DLL bên thứ ba (NSIS/WiX), không ký khi thiếu token SignPath. Lưu ý: SignPath Foundation yêu cầu xác minh nguồn qua GitHub Actions, nên bản phát hành chính thức dùng action SignPath (job `windows-signed`), còn hook này dành cho gói SignPath trả phí/tự quản.
- `docs/INSTALL-macOS.md`, `docs/INSTALL-Windows.md` (Việt + Anh, chờ ảnh chụp); đường dẫn cài NSIS theo user là `%LOCALAPPDATA%\NANOMultipleGitGUI`.

**0.5 — Tài liệu cộng đồng & ADR ✅ (2026-10-14)**
- `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md` (Contributor Covenant 2.1 bản gốc — còn chờ kênh liên hệ), `SECURITY.md` (báo cáo riêng qua GitHub Security Advisories), `PRIVACY.md`, `CHANGELOG.md` (Keep a Changelog), issue template dạng form (bug/feature) + PR template, `.github/release-notes.md`.
- ADR trong `docs/adr/`: 0001 Tauri, 0002 git CLI (+ gix khi cần), 0003 credential helper sidecar, 0004 LFS transfer 2 bước — đều kèm số liệu từ prototype.
- Kiểm tra tên (2026-10-14): không có repo/user/org GitHub nào tên `NANOMultipleGitGUI` / `nanomgg`; crates.io và npm còn trống `nanomgg`, `nanomultiplegitgui`, `nanomgg-credential`; các tên miền `nanomultiplegitgui.{com,dev,app}` và `nanomgg.{com,dev,app,io}` chưa có bản ghi DNS (cần xác nhận lại ở nhà đăng ký tên miền).

**0.6 — Prototype kiểm chứng rủi ro ✅ (2026-10-14)**
- **Đa tài khoản HTTPS (kể cả LFS)** — đã kiểm chứng toàn bộ thiết kế mục 4.4:
  - `crates/credentials`: server IPC (named pipe trên Windows, unix socket trong thư mục `0700`), session token ngẫu nhiên, so sánh hằng thời gian, kiểm tra host (chỉ HTTPS; HTTP chỉ cho loopback), token lưu trong `Zeroizing`.
  - `nanomgg-credential`: đọc giao thức credential của git, hỏi app qua socket; khi bị từ chối thì in `quit=1` để git không thử helper khác hay hỏi mật khẩu.
  - `git-engine::AccountContext` + `GitRunner`: inject identity + `credential.helper=` (reset) + helper + `useHttpPath` qua `GIT_CONFIG_COUNT`, kèm `GIT_TERMINAL_PROMPT=0` và `GIT_SSH_COMMAND` cho SSH.
  - `tools/test-git-server`: server smart-HTTP (`git http-backend`) + LFS Batch API nhiều user, mỗi repo chỉ chủ sở hữu truy cập được, ghi lại ai làm gì.
  - Kết quả integration test (có helper "mồi" trả mật khẩu sai trong config global):
    - alice và bob push song song vào repo của mình trên cùng host, kèm file LFS, đúng danh tính commit;
    - bob push vào repo của alice nhận 403 và không clone được;
    - clone "tải sau" + `git lfs pull` tải LFS đúng bằng tài khoản alice;
    - helper không trả token cho `evil.example.com`, `lfs.thirdparty.io` hay `http://github.com`;
    - **file LFS 200 MB push trong 5,0 s (~40 MiB/s, cục bộ)**; push lại không upload gì thêm (tiếp tục ở mức file);
    - mutation test: bỏ dòng reset `credential.helper=` thì cả 4 test fail.
  - Chưa làm (để Phase 2): thử với 2 tài khoản GitHub thật, DACL chặt cho named pipe, keychain thật, `SSH_ASKPASS`, đóng gói sidecar (`externalBin`).
- **Commit graph Canvas**:
  - `scripts/gen-history-fixture.mjs` sinh repo lịch sử phức tạp (nhánh feature/release, merge, tag, tác giả tiếng Việt) bằng `git fast-import`: 50k commit trong khoảng 7 giây.
  - `git-engine::graph`: đọc `git log --all --topo-order -z --decorate=full`, tính lane theo từng hàng độc lập (tái dùng lane trống, 8 màu) để vẽ bất kỳ đoạn nào cũng được.
  - UI `features/graph`: canvas sticky + spacer, tải dữ liệu theo trang 500 hàng (prefetch 600), nhãn ref/tag/HEAD, node rỗng cho merge, hover/chọn hàng, màu đọc từ token và cập nhật khi đổi theme; nút "Mở repo…" (plugin dialog), mở từ dòng lệnh `nanomgg <path>` / `--open`, đo bằng `--bench-graph`.
  - **Số đo (Windows 10, bản release, WebView2):**

    | Repo | git log + parse | Tính lane | Vẽ lần đầu | Cuộn 600 hàng/s |
    |---|---|---|---|---|
    | 50k commit | 309–578 ms | 21–40 ms | 372–456 ms | 143 fps, p95 7,1 ms, vẽ trung bình 1,3–1,8 ms |
    | 100k commit | 690–835 ms | 44–71 ms | 780 ms | 143 fps, p95 7,1 ms, 0 khung chậm |

    Đạt mục tiêu mục 5.2 ("100k commit < 2 s, cuộn 60 fps") bằng git CLI, chưa cần gix (ADR 0002).
- Lệnh tiện ích: `pnpm fixture:graph`, `pnpm bench:graph target/fixtures/graph-50k`, `pnpm test:lfs-200mb`.

### Phase 1 — Git core MVP (6–8 tuần)

#### Các bước của Phase 1 (đề xuất)
**1.1 — Mở repo thật ✅ (2026-10-14)**
- **git-engine:**
  - `version`: đọc `git --version` / `git lfs version` (định dạng Windows/macOS/Linux), so với mức tối thiểu git 2.35 / git-lfs 3.0.
  - `repo`: `discover` (mở từ bất kỳ thư mục con nào, trả về root + git dir tuyệt đối), `init -b <nhánh>`; lỗi có kiểu: không phải repo / thư mục không tồn tại / repo bare (chưa hỗ trợ) / `safe.directory` (dubious ownership).
  - `refs`: đọc nhánh local (upstream, ahead/behind, upstream đã bị xóa), remote + URL, tag (annotated được peel về commit), stash, trạng thái HEAD (bình thường / chưa có commit / tách rời) bằng `for-each-ref` tách NUL.
  - `watch`: watcher `notify` (debounce 300 ms) phân loại thay đổi thành **refs** (HEAD, refs, packed-refs, config, merge/rebase), **index** hoặc **worktree**; bỏ qua `objects/`, `logs/`, `*.lock`, repo lồng nhau; hỗ trợ worktree có git dir nằm ngoài.
  - `GitRunner` luôn đặt `GIT_OPTIONAL_LOCKS=0` để lệnh đọc không ghi `.git/index` (tránh tự đánh thức watcher thành vòng lặp).
- **storage:** migration v2 thêm bảng `repos` (`path_key` so sánh không phân biệt hoa/thường trên Windows/macOS để 1 thư mục chỉ có 1 dòng); test nâng cấp DB từ v1.
- **App (Rust):**
  - Command `tool_status`, `repo_open`, `repo_init`, `repo_close`, `workspace_state`, `workspace_set_active`, `recent_repos`, `recent_remove`, `repo_refs`. Thao tác chặn (chạy git, đọc đĩa) chạy trên thread riêng, không chặn IPC.
  - Graph theo từng repo (`graph_open(repoId)`, `graph_rows(repoId, …)`).
  - Event có kiểu `RepoChanged` phát từ watcher.
  - Danh sách tab và tab đang chọn lưu ở settings (`workspace.tabs`, `workspace.active`). Khi khởi động, các tab được khôi phục và đường dẫn được kiểm tra lại; thư mục mất thì tab vẫn giữ nhưng đánh dấu `missing`.
  - URL remote được redact trước khi gửi lên UI.
- **App (UI):**
  - Tab repo thật: đóng bằng nút X hoặc chuột giữa; menu "+" gồm Mở repo / Tạo repo mới / danh sách gần đây.
  - Màn hình chào mừng có danh sách gần đây (đánh dấu repo mất, xóa khỏi danh sách).
  - Hộp thoại xác nhận khi mở thư mục chưa phải repo hoặc tạo repo mới (kiểm tra tên nhánh).
  - Màn hình khôi phục khi thư mục repo bị mất (Thử mở lại / Đóng tab / Xóa khỏi danh sách).
  - Banner khi thiếu hoặc quá cũ: git chặn thao tác, git-lfs chỉ cảnh báo (ẩn được); thanh thông báo lỗi.
  - Sidebar đọc ref thật: Local (HEAD đậm, ↑/↓, upstream bị xóa thì gạch ngang, nhánh chưa có commit, HEAD tách rời), Remote nhóm theo remote, Tags, Stashes, ô lọc. PR/LFS/Submodules/Worktrees ghi "sắp có".
  - Status bar: remote chính + giao thức (HTTPS/SSH/LOCAL), nhánh, ahead/behind, đường dẫn repo. Đã bỏ dữ liệu mẫu gây hiểu nhầm (CI, tiến độ LFS, file thay đổi giả).
  - Graph tự tải lại khi watcher báo ref đổi. Commit nội bộ của stash không còn hiện trong graph (`--exclude=refs/stash`). Cột graph có độ rộng tối thiểu.
  - Account pill vẫn là dữ liệu mẫu, gắn theo repo id và chỉ giữ trong bộ nhớ, chờ Phase 2.
- **Test:** 67 test Rust (gồm repo thật có remote/tag/stash/HEAD tách rời và watcher trên file thật) + 127 test frontend (welcome, tab, thư mục mất, hộp thoại init, thông báo lỗi, helper ref/transport/tên nhánh/công cụ).
- **Kiểm chứng trên app thật (Windows 10, bản release):**
  - Mở repo demo từ dòng lệnh: sidebar, status bar, graph đúng.
  - Tạo commit + nhánh + tag bằng terminal bên ngoài: app tự cập nhật sau khoảng 1 giây, không cần làm mới.
  - Khởi động lại thì tab được khôi phục.
  - Đổi tên thư mục repo thì hiện màn hình khôi phục.
  - Mở thư mục trống thì hiện hộp thoại "chưa phải repo"; không tự tạo gì.
  - Mở 2 tab (demo + 50k commit), màn hình chào mừng hiện danh sách gần đây.
  - Phát hiện và sửa trong lúc làm: selector Zustand trả mảng mới gây vòng lặp render (test bắt được); commit stash lẫn vào graph; tiêu đề cột graph xuống dòng.
  - Một lần thấy graph 50k mở ra đã cuộn sẵn, không tái hiện được sau 3 lần thử (nghi do con lăn chuột khi cửa sổ bật lên dưới con trỏ) — theo dõi ở 1.2.
- Chưa làm (để bước sau): watcher chưa áp `.gitignore` (thư mục build lớn sẽ sinh nhiều event worktree — xử lý ở 1.3 khi dùng tới); graph tải lại toàn bộ mỗi khi ref đổi (tối ưu tăng dần ở 1.2); chưa có nút "Relocate" cho repo bị di chuyển.

**1.2 — Commit graph thật ✅ (2026-10-14)**
- **git-engine `history::RepoGraph`:**
  - Giữ lịch sử đã tải cùng bố cục lane và nhãn ref. Nhãn lấy từ `for-each-ref` (không dùng `%D` nữa), nên đổi nhãn không cần đọc lại log.
  - `refresh()` chọn 1 trong 4 cách:
    - **Unchanged** — tip không đổi;
    - **LabelsOnly** — chỉ nhãn đổi, ví dụ thêm tag vào commit đã có;
    - **Incremental** — chỉ đọc commit mới bằng `git log --all --stdin` với `^tip-cũ` (qua stdin, không vướng giới hạn độ dài dòng lệnh của Windows), rồi ghép lên đầu;
    - **Full** — có commit bị mất khỏi lịch sử (xóa nhánh, `reset`, rebase), phát hiện bằng `rev-list --count <tip-mất> ^<tip-hiện-tại>` vốn chỉ duyệt vùng thay đổi.
  - `find` (hash đầy đủ hoặc tiền tố ≥ 4 ký tự), `search` (subject / tác giả / email / tiền tố SHA, không phân biệt hoa/thường, có dấu tiếng Việt).
- **git-engine `commit`:** chi tiết commit gồm tác giả / committer / thời gian, message đầy đủ, file thay đổi (A/M/D/R/C/T, tên cũ khi đổi tên, +/− dòng, nhận biết file nhị phân). Merge commit so với parent đầu tiên; commit gốc so với cây rỗng. Hash được kiểm tra chặt (chỉ chấp nhận hex 4–64 ký tự), nên không thể chèn tham số kiểu `--output=…` vào lệnh git.
- **App:**
  - Command `graph_open` (lần đầu đọc toàn bộ, các lần sau làm mới; lấy graph ra khỏi map trong lúc chạy git để không chặn việc tải dòng), `graph_find`, `graph_search`, `commit_details`.
  - `GraphSummary` thêm `version` / `refresh` / `added` / `loadMs`.
- **UI:**
  - Chọn commit bằng chuột hoặc bàn phím (↑/↓, PageUp/PageDown, Home/End, Esc để bỏ chọn); dòng đang chọn có vạch màu chính bên trái.
  - Panel "Chi tiết commit" ở bên phải: summary, body, SHA kèm nút sao chép, commit cha (bấm để nhảy tới), tác giả/committer, danh sách file kèm +/−.
  - Ô tìm kiếm trên graph: Ctrl/Cmd+F, Enter / Shift+Enter, bộ đếm "i/N", tô sáng các dòng khớp.
  - Bấm nhánh local / remote / tag / HEAD tách rời ở sidebar để nhảy tới commit tương ứng.
  - Khi lịch sử đổi, commit đang chọn được giữ theo hash và dòng trên cùng được giữ làm neo cuộn. Tìm kiếm chạy lại nhưng không tự nhảy đi nơi khác.
- **Số đo (repo 50k commit, bản release):** đọc toàn bộ khoảng 400 ms; làm mới tăng dần khi có commit mới khoảng 130 ms. Bản đầu dùng `rev-list --count --all` mất khoảng 300 ms, đã tối ưu.
- **Test:** 75 test Rust (gồm làm mới khi thêm commit / thêm tag / di chuyển nhánh / xóa nhánh / `reset`, so sánh với lần đọc từ đầu; chi tiết commit gốc / thường / merge / đổi tên / nhị phân; từ chối chèn tham số) + 135 test frontend.
- **Kiểm chứng trên app thật bằng Windows UI Automation** (không chiếm chuột/bàn phím; app chạy ở trạng thái thu nhỏ):
  - Bấm `main` ở sidebar: graph chọn đúng commit và mở panel chi tiết.
  - Tìm "commit" ra 4 kết quả; "kết quả sau" tới 3/4 thì panel hiện đúng file.
  - Bấm commit cha: nhảy đúng tới cha.
  - Tạo commit từ terminal bên ngoài: commit đang chọn và từ khóa được giữ, bộ đếm cập nhật 2/3 → 3/4, log ghi `refresh: Incremental, added: 1`.
- **Đã sửa trong lúc kiểm chứng:** tìm kiếm chạy lại sau khi làm mới từng tự nhảy về kết quả đầu tiên (làm mất commit đang chọn).
- Bộ cài đã build lại: `target/release/bundle/nsis/NANOMultipleGitGUI_0.0.1_x64-setup.exe` (2,9 MB), `target/release/bundle/msi/…msi` (4,0 MB).
- Chưa làm (để bước sau): tìm trong phần thân message (cần `git log --grep`); xem diff của từng file khi bấm (bước 1.3, cùng Monaco); stash chưa hiện thành node trong graph; ảnh đại diện tác giả (Phase 4).

**1.3 — Working tree & commit ✅ (2026-10-14)**
- **git-engine:**
  - `status`: parse `git status --porcelain=v2 -z --branch -uall --find-renames` → staged / unstaged (gồm untracked) / conflicted, kèm nhánh, upstream, ahead/behind, HEAD chưa có commit; dung lượng file trong working tree (cho badge "Nặng").
  - `diff`: diff một file ở 4 phía (chưa stage, đã stage, file mới, trong một commit so với cha đầu) với `--no-ext-diff --no-textconv`, prefix cố định, `--literal-pathspecs`; parse thành hunk + số dòng cũ/mới, nhận biết file nhị phân, file mới/xóa/đổi tên/đổi quyền, giới hạn kích thước. Nội dung 2 phía (index/HEAD/working tree/commit) cho Monaco.
  - **Stage/unstage/discard theo hunk hoặc từng dòng**: dựng patch một phần từ byte gốc (giữ nguyên CRLF/encoding) rồi `git apply --cached [-R]` hoặc `git apply -R` cho working tree. Mỗi diff có `fingerprint`; thao tác trên diff đã cũ bị từ chối thay vì áp sai chỗ. Đã kiểm chứng với `core.autocrlf=true`.
  - `index`: stage/unstage/discard theo file qua `--pathspec-from-file` (không vướng giới hạn dòng lệnh Windows, pathspec literal nên tên như `[ab].txt` an toàn); unstage chạy được cả khi chưa có commit; file untracked xóa bằng `git clean` (không bao giờ đụng file tracked/ignored).
  - `commit_write`: commit/amend với message qua stdin (`--cleanup=whitespace` nên giữ dòng bắt đầu bằng `#`), sign-off; lỗi gom cả stdout/stderr để hiện output của hook; đọc identity mà git sẽ ghi.
  - Watcher bỏ qua thay đổi chỉ nằm trong file bị `.gitignore` (`git check-ignore --stdin`).
- **App:** command `worktree_status`, `file_diff`, `file_contents`, `stage_files`, `unstage_files`, `stage_all`, `unstage_all`, `discard_files`, `apply_lines`, `commit_create`, `commit_identity`, `head_message`. Sửa race có từ 1.2: hai lần `graph_open` đồng thời (watcher + UI) khiến lần sau đọc lại toàn bộ và reset `version` → nay khóa theo từng repo.
- **UI:**
  - Hàng **WIP** trên đầu graph (số file, số đã stage, số xung đột); không chọn commit thì panel phải là **"Thay đổi"**: Xung đột / Chưa stage / Đã stage, chọn nhiều file (Ctrl/Shift), nút hover Stage/Bỏ stage/Hủy, menu chuột phải, xác nhận trước khi hủy.
  - **Diff viewer** ở vùng giữa: chế độ **Hunk** (bấm dòng +/− để chọn, Shift để chọn liên tiếp, Stage/Bỏ stage/Hủy hunk hoặc dòng đã chọn) và **Inline / Song song** bằng Monaco (bundle cục bộ, 1 editor worker, tokenizer cơ bản, theme đọc từ design token, ẩn vùng không đổi, bỏ qua khoảng trắng). Bấm file trong "Chi tiết commit" cũng mở diff của commit đó.
  - Ô commit: tóm tắt (đếm 72 ký tự) + mô tả, Amend (tự điền message cũ), Sign-off, Ctrl+Enter, hiện "Commit với tư cách" theo git config thật, chặn khi chưa stage hoặc còn xung đột.
  - **Menu chuột phải trên commit** (graph): Checkout (HEAD tách rời), Tạo nhánh/tag tại đây, Cherry-pick, Revert, Reset nhánh hiện tại (Soft/Mixed/Hard có xác nhận), Amend (commit HEAD), sao chép SHA/tiêu đề, và submenu cho từng nhánh local/remote/tag trên commit đó. Sidebar có cùng menu cho nhánh, nhánh remote, tag.
  - Banner thao tác dở (merge/rebase/cherry-pick/revert): số file còn xung đột, Hủy / Tiếp tục.
- **Kiểm chứng trên app thật** (Windows 10, Vite dev + bản release, điều khiển qua CDP của WebView2 — `.agent/tools/cdp.mjs`, không chiếm chuột/bàn phím): stage đúng 1 dòng `+line 30 changed` (git xác nhận `-line 30` vẫn là context); Monaco song song; commit 3 file qua UI (message giữ dòng `#12`); menu chuột phải → cherry-pick; tạo tag annotated; Push hiện tiến độ "Writing objects 100%" và nút Hủy; cherry-pick gây xung đột → banner → giải quyết, "Đánh dấu đã giải quyết", Tiếp tục → commit tạo đúng; xóa nhánh chưa merge từ sidebar → hỏi xóa cưỡng bức.
  - Trên **bản cài release** (`tauri build`): Stash (hộp thoại, gồm untracked) rồi Pop trả lại đúng thay đổi; "Amend this commit…" từ menu tự điền message và đổi nút thành "Amend commit"; hộp thoại Reset hard; Hủy thay đổi xóa file untracked sau khi xác nhận; panel LFS hiện git-lfs 3.5.1.
  - Sửa sau kiểm chứng: tên file bị co mất trên thanh diff khi bật "Bỏ qua khoảng trắng"; bỏ amend (vào từ menu) không xóa message cũ; tiếng Anh hiện "1 files" (thêm dạng số ít `_one`); dòng phiên bản git-lfs bị xuống dòng.
  - Bộ cài: `target/release/bundle/nsis/NANOMultipleGitGUI_0.0.1_x64-setup.exe` (4,1 MB), `…/msi/…msi` (6,4 MB), icon mới.

**1.4 — Branch & remote ✅ (2026-10-14)** (làm song song bởi sub-agent)
- **git-engine:** `branch` (tạo tại commit/ref, checkout, checkout nhánh remote thành nhánh tracking, HEAD tách rời, đổi tên, xóa có `force`, set/unset upstream), `ops` (tag lightweight/annotated, cherry-pick/revert với `-m 1` cho merge commit, reset soft/mixed/hard, phát hiện merge/rebase/cherry-pick/revert/bisect/am kèm abort/continue), `merge` (ff-only/ff/no-ff/squash), `remote` (fetch/pull merge|rebase|ff-only/push, force-with-lease, push tag, xóa nhánh remote), `progress` (parse `--progress`, `CancelToken`, kill cây process).
- Lỗi nhận diện được trả về dạng `Outcome` (`done` / `rejected{FailureKind}` / `cancelled`) với 19 loại (not fully merged, local changes, non-fast-forward, stale lease, auth…); xung đột là kết quả có cấu trúc, không phải lỗi. Chống chèn tham số: `check-ref-format`, `--end-of-options`, remote phải là remote đã cấu hình.
- App: 22 command + event `TransferProgress` (~10/giây), `transfer_cancel` theo `opId`. UI: nút Fetch / Pull▾ (merge, rebase, ff-only) / Push▾ (tự đặt upstream cho nhánh mới, force-with-lease có xác nhận) / Branch; tiến độ + nút Hủy ở status bar.
- **Hạn chế:** chưa có rebase/interactive rebase, chưa xóa tag trên remote. (Hủy không giết được tiến trình con của hook MSYS — đã sửa ở bước 2.4 bằng Windows Job Object.)

**1.5 — Stash & LFS cơ bản ✅ (2026-10-14)** (làm song song bởi sub-agent)
- **Stash:** tạo (message, gồm untracked, giữ phần đã stage, `--staged`, theo path), apply/pop/drop theo `stash@{n}`, xem file (gồm untracked ở parent thứ 3); pop khi xung đột trả danh sách file và giữ entry. UI: nút Stash (hộp thoại) / Pop, menu chuột phải ở mục Stashes, hộp thoại xem file.
- **Crate `nanomgg-lfs`:** `detect` (phiên bản git-lfs, pattern `filter=lfs`/`lockable` từ mọi `.gitattributes`, hook LFS theo `core.hooksPath`, config `filter.lfs`), `setup` (`install --local`, track/untrack, renormalize, re-stage thành pointer), `guard` (Large File Guard: GitHub 50/100 MiB, GitLab 50 MiB không chặn; đo blob đã stage, nhận biết pointer, file nhị phân/build, gợi ý pattern theo đuôi hoặc đường dẫn).
- UI: panel LFS ở sidebar (phiên bản, pattern, track/untrack, nút cài hook cho repo); trước khi commit chạy guard — hộp thoại theo mục 6.2, "Track bằng LFS" rồi commit tiếp, "Vẫn commit" bị khóa khi vượt giới hạn cứng. "Thêm vào .gitignore" và ngưỡng chỉnh theo repo để sau.
- **Test (cả 3 bước):** Rust — git-engine 81, app 14, LFS 17 (gồm repo thật có git-lfs), toàn workspace qua; clippy `-D warnings` và `cargo fmt --check` sạch. Frontend — 190 test (23 file); typecheck, lint, `prettier --check` sạch.

Nội dung gốc của Phase 1:
- `GitRunner` (CLI) + reader `gix`; mở/init repo, tab repo, watcher.
- Commit graph virtualized (đọc log, tính lane), nhãn ref.
- Staging theo file/hunk/dòng, diff viewer (Monaco), commit/amend.
- Branch: tạo/checkout/xóa/đổi tên; merge cơ bản; fetch/pull/push có tiến độ (credential tạm dùng của hệ thống).
- Stash cơ bản.
- **LFS mức cơ bản**: phát hiện git-lfs/.gitattributes, cài hook local, **Large File Guard** trước commit (track bằng LFS một cú bấm).
- ✅ Kết quả: dùng hằng ngày được cho repo đơn tài khoản.

### Phase 2 — Multi-account core + LFS transfer (6–8 tuần) ⭐ điểm khác biệt
- Model Account/Identity/SshKey/Rule/RepoBinding; lưu secret bằng keychain.
- Đăng nhập GitHub (device flow + PAT), GitLab.com (PKCE + PAT), GitLab self-hosted, GitHub Enterprise (PAT).
- Sidecar `nanomgg-credential` + IPC; inject HTTPS/SSH/identity qua `GIT_CONFIG_COUNT` cho mọi lệnh git **và git-lfs**; kiểm tra host trước khi trả token.
- Tạo/nhập SSH key, upload public key lên provider qua API.
- Rule engine + tự nhận diện tài khoản khi mở repo; account pill; switcher; command palette.
- Commit Guard và Push Guard (gồm kiểm tra blob nặng trước khi push).
- **Upload/download LFS**: ước lượng dung lượng, tiến độ theo file, hàng đợi transfer nền, hủy/tiếp tục, dịch lỗi quota/quyền.
- Clone browser hợp nhất đa tài khoản (có tùy chọn tải LFS: hết / sau / một phần), clone theo URL có đoán tài khoản, thư mục theo tài khoản.
- Wizard import `~/.gitconfig` / `~/.ssh/config` / `gh` / `glab`.
- Terminal tích hợp có inject account context.
- Nộp hồ sơ SignPath Foundation.
- 🚀 **Phát hành v0.1 (public alpha)**.

#### Các bước của Phase 2 (làm song song bằng nhiều sub-agent, mỗi agent sở hữu nhóm file riêng)
**2.1 — Lõi tài khoản ✅** — crate `nanomgg-accounts` (model §4.1, parse URL remote gồm SSH/port/subgroup GitLab, rule engine, thứ tự giải quyết §4.3 theo `(repo, remote)`, bảng màu tài khoản kiểm ΔE so với success/danger); SQLite v3 (providers, accounts, identities, ssh_keys, rules, repo_bindings, workspaces — STRICT, foreign key, cascade, trigger chặn identity khác tài khoản); keychain hệ điều hành (`keyring`, mục `nanomgg:<id>:token|refresh_token|ssh_passphrase`), `KeychainSource` cho credential server; askpass cho passphrase SSH qua cùng socket + session token; resolver `accounts::context_for/context_with` (identity + HTTPS helper + `GIT_SSH_COMMAND` từ `nanomgg_ssh_keys`); sidecar `nanomgg-credential` đóng gói bằng `externalBin` (`build.rs` tự build lồng). 24 command quản lý provider/tài khoản/identity/rule/binding; ghi identity vào `.git/config` mặc định bật, `core.sshCommand` opt-in.
**2.2 — Đăng nhập & API provider ✅** — `nanomgg-auth`: device flow RFC 8628 (GitHub/GHE/GitLab), GitLab PKCE qua loopback cổng cố định theo provider (mặc định 47821), refresh token GitLab (task nền 5 phút, luôn lưu refresh token mới), kiểm tra PAT (scope + hạn, phân loại classic/fine-grained/OAuth), `token_health`. `nanomgg-providers`: một client reqwest (rustls + ring, không OpenSSL; ADR 0005 thay octocrab) cho github.com/GHE/gitlab.com/self-hosted — user, email đã verify, list repo hợp nhất có phân trang/hủy, quyền theo remote, SSH key, lỗi có kiểu (gợi ý OAuth App restriction / SAML / thiếu scope / rate limit). Thiếu OAuth client id → UI hướng dẫn tạo PAT đúng scope.
**2.3 — SSH key & nhập cấu hình ✅** — `nanomgg-ssh-keys`: tạo ed25519 (passphrase chuẩn OpenSSH), nhập key OpenSSH, fingerprint khớp `ssh-keygen`, lưu `<appdata>/ssh/<id>` ghi nguyên tử + ACL chỉ user hiện tại (`icacls` theo SID, kiểm lại), `git_ssh_command` an toàn với dấu cách/`%`, ghim host key GitHub/GitLab. UI: tạo/nhập/upload lên provider/đặt mặc định/kiểm tra kết nối (`ssh -T` cho biết key xác thực là ai). `nanomgg-import` (chỉ đọc): `.gitconfig` + `includeIf` → identity + rule, `.ssh/config` → key theo host (alias không còn cần), gh/glab → tài khoản (không bao giờ đọc token); wizard tự mở lần đầu khi chưa có tài khoản.
**2.4 — LFS transfer + Job Object ✅** — Job Object (crate `win32job`, không `unsafe`, chỉ API chuẩn) thay `taskkill`: hủy giết cả cây tiến trình kể cả con của hook MSYS. `nanomgg-lfs::transfer`: ước lượng (`push --dry-run`, `ls-files --json`, ổ trống bằng `fs4`), upload/download đọc `GIT_LFS_PROGRESS` (theo file, tốc độ, ETA), tham số `lfs.*`, dịch lỗi quyền/quota/kích thước/LFS tắt/mạng kèm link billing. UI hàng đợi transfer ở status bar (`⇅ LFS 41% · 760 MB/1.8 GB`), popover theo repo, Hủy/Thử lại; push repo LFS = ước lượng → xác nhận theo tài khoản → upload → `git push`. Hủy rồi push lại chỉ upload phần thiếu (test server xác nhận).
**2.5 — Terminal tích hợp ✅** — portable-pty (ConPTY), Git Bash/pwsh/cmd hoặc `$SHELL -l`, output qua `Channel`, nhiều tab mỗi repo, theme từ token; môi trường = env của tài khoản repo (`NANOMGG_*`, `GIT_CONFIG_*`, `GIT_SSH_COMMAND`).
**2.6 — UI tài khoản & nối resolver ✅** — Account manager (thẻ tài khoản + sức khỏe token, wizard thêm tài khoản device/PKCE/PAT/self-hosted, identity, rule có "Thử", server, tùy chọn); account pill thật (Dùng cho repo này + Ghi nhớ/khóa, Chỉ cho thao tác này, theo từng remote), viền cửa sổ + tab theo màu tài khoản, banner "chưa gắn tài khoản", `Ctrl/Cmd+Alt+1..9`; command palette `Ctrl/Cmd+K` (~30 lệnh + Switch account / Commit as / Push as / Clone as). Mọi lệnh ghi/mạng chạy theo resolver; tham số `asAccount` cho commit/push/pull/fetch/LFS (override một lần: dùng xong là hết, chỉ trả lại khi người dùng hủy). Thêm loại lỗi `permissionDenied` (HTTP 403).
**2.7 — Clone browser & Guard ✅** — Clone: list repo mọi tài khoản (cache 10 phút, lỗi riêng từng tài khoản), dán URL tự đoán tài khoản (rule → tài khoản cùng host, ưu tiên username trùng owner), mẫu thư mục `{clone_dir}/{owner}/{repo}`, ổ trống, HTTPS/SSH, submodule, shallow, LFS tải hết/tải sau/một phần; tạo binding `source=clone`. Commit Guard (email lệch rule/tài khoản, webmail trên repo tổ chức, email chưa verify, gpgsign thiếu key, chưa gắn) và Push Guard (tác giả lệch + Sửa tác giả, quyền push qua API, blob nặng + Chuyển sang LFS chỉ cho commit chưa push; luôn có backup `refs/nanomgg/backup/<ts>` và Undo).
**2.8 — Kiểm chứng end-to-end ✅ (bản cài release, Windows 10, điều khiển qua CDP — không chiếm chuột/bàn phím)** — test server cục bộ 2 user (`cargo run -p nanomgg-test-git-server --example serve`):
- Lần chạy đầu tự mở wizard nhập cấu hình (đọc được identity + 2 tài khoản gh thật, không sửa gì).
- Gắn repo vào "Công ty" (alice) qua pill → viền/tab màu, "Commit với tư cách Alice", `user.email` ghi vào repo; token nằm trong Windows Credential Manager.
- Commit qua UI → tác giả Alice; Push Guard phát hiện commit "Seed" khác tác giả → "Sửa tác giả" (có backup ref) → Push: server ghi `alice GitWrite` (qua sidecar của bản cài).
- Palette "Push as… → Cá nhân (bob)": server ghi `bob Forbidden` (403) → override đúng; sửa nhãn lỗi sai ("không kết nối") thành "không có quyền" và override không còn treo sau lần chạy bị từ chối.
- Terminal tích hợp: `NANOMGG_ACCOUNT` = alice, `git ls-remote` đi qua sidecar bằng alice.
- Clone qua dialog bằng tài khoản bob → server `bob GitRead`, tab mới, `user.email` của bob.
- Test tự động cuối: Rust toàn workspace (app 74, git-engine 103, accounts 31, auth 34, providers 28, lfs 36, ssh-keys 21, import 22, storage 16, credentials 19, multi-account 4…) + 415 test frontend; clippy `-D warnings`, rustfmt, eslint, prettier sạch.
- Ghi nhận khi thử: trong terminal tích hợp, lệnh git thiếu credential sẽ làm Git for Windows bật cửa sổ `git-askpass` (hành vi mặc định của Git for Windows trong terminal tương tác).

**2.9 — Sửa sau v0.1 ✅ (2026-10-14)**
- **`.env` khi build:** `apps/desktop/src-tauri/build.rs` đọc `<gốc repo>/.env` (đã git-ignore) và nhúng `NANOMGG_GITHUB_OAUTH_CLIENT_ID` / `NANOMGG_GITLAB_OAUTH_CLIENT_ID` nếu môi trường chưa đặt. Biến môi trường thật (CI) vẫn được ưu tiên. Sửa `.env` thì lần build sau tự nhúng lại.
- **Push bị từ chối vì thiếu scope `workflow`** (GitHub chặn mọi push đụng `.github/workflows/` khi token OAuth/PAT không có scope này): GitHub nay xin mặc định `repo workflow read:user user:email`, kể cả link tạo PAT. Tài khoản đã đăng nhập trước đó cần **đăng nhập lại một lần**.
- **Lỗi có hướng dẫn sửa (cho người mới):** thêm 5 loại lỗi push — `workflowScope`, `protectedBranch` (GH006, ruleset, GitLab protected branch), `emailPrivacy` (GH007), `secretDetected` (push protection), `fileTooLarge` (GH001). Thanh thông báo hiện: tên lỗi, **cách sửa bằng lời dễ hiểu cho cả 25 loại lỗi**, nút hành động khi app tự làm được ("Đăng nhập lại <tài khoản>" mở thẳng wizard đăng nhập lại, "Mở quản lý tài khoản", "Mở cài đặt email GitHub"), và log gốc của git thu gọn trong "Chi tiết kỹ thuật". Test bảo đảm loại lỗi nào cũng có nhãn và hướng dẫn ở cả vi/en.

### Phase 3 — Git nâng cao (6–8 tuần)
- Interactive rebase kéo-thả, cherry-pick, revert, reset, tag.
- Conflict editor 3-way (gồm xung đột file binary/LFS); abort/continue cho mọi thao tác dở dang.
- Undo/redo bằng reflog; kéo-thả branch để merge/rebase.
- File history, blame, image diff (gồm ảnh LFS); submodule, worktree.
- **LFS nâng cao**: `lfs migrate` cho commit chưa push (có backup ref), LFS lock, LFS panel, prune/fsck, tải từng file pointer.
- Signing GPG/SSH theo identity.
- 🚀 v0.2 / v0.3 (beta).

#### Các bước của Phase 3 (3 sub-agent song song, mỗi agent sở hữu nhóm file riêng; báo cáo chi tiết ở `.agent/phase3/3.x.md`)
**3.1 — Rebase, interactive rebase, Undo/Redo, kéo-thả nhánh ✅ (2026-10-15)**
- **git-engine `rebase`:** rebase nhánh hiện tại hoặc nhánh khác (tự `switch` trước) lên ref/commit; chặn HEAD tách rời, working tree bẩn (trừ khi bật autostash), và commit đã có trên remote khi chưa xác nhận (`FailureKind::pushedCommits`). Luôn tạo backup `refs/nanomgg/backup/<ts>`. Xung đột / dừng ở `edit` là kết quả có cấu trúc (bước i/n).
- **Interactive rebase không bao giờ mở editor:** todo và message mới được lưu thành blob git; `GIT_SEQUENCE_EDITOR='git cat-file blob <oid> >"$1"'` cài todo, reword/squash/edit dùng `exec git cat-file blob … | git commit --amend -F -`. Không có đường dẫn file trong lệnh nên an toàn với dấu cách và tiếng Việt (có test trong thư mục `thư mục có dấu cách`); rebase dừng giữa chừng vẫn tiếp tục được từ terminal. Chặn khoảng có merge commit (kèm giải thích), tối đa 2000 commit.
- **Undo/Redo:** journal `.git/nanomgg/undo.json` theo worktree; nguồn chính là reflog HEAD (nên thao tác từ terminal cũng undo được): commit, amend, reset, checkout, merge, rebase (cả cụm là 1 bước), cherry-pick, revert, pull. App tự ghi thêm tạo/xóa/đổi tên nhánh và fast-forward nhánh không checkout (giữ SHA + upstream để khôi phục nhánh đã xóa). Commit/amend undo bằng `reset --soft`, còn lại `reset --keep` (không bao giờ ghi đè thay đổi chưa commit); chỉ chạy khi repo đúng trạng thái "sau"; thao tác đã push phải xác nhận.
- **UI:** hộp thoại rebase (danh sách commit sẽ viết lại, cảnh báo đã push/bẩn); hộp thoại interactive rebase (kéo tay nắm hoặc Alt+↑/↓, chọn pick/reword/squash/fixup/drop/edit, sửa message tại dòng, cột "Kết quả" xem trước); banner "Rebase bước i/n" với Hủy / Bỏ qua commit này / Tiếp tục; Undo/Redo trên toolbar có tooltip mô tả, `Ctrl/Cmd+Z` / `Ctrl/Cmd+Shift+Z` (không bắt trong ô nhập, Monaco, terminal, dialog); kéo nhánh (sidebar hoặc nhãn trên graph — hit-test dùng chung code vẽ) thả lên nhánh khác → Fast-forward / Merge / Rebase. Kéo bằng pointer events vì HTML5 DnD bị Tauri chặn trên Windows. Mục menu + palette "Rebase … lên đây", "Interactive rebase từ đây…".

**3.2 — Conflict editor, file history, blame, image diff ✅ (2026-10-15)**
- **git-engine:** `conflict` (đọc stage 1/2/3 + file có marker, gồm `diff3`/`zdiff3`; xác định ours/theirs đúng theo merge / squash / rebase / cherry-pick / revert / stash / `am`; ghi kết quả giữ CRLF/BOM rồi `add`; lấy nguyên một phía bằng `checkout --ours/--theirs` hoặc `git rm` khi phía đó đã xóa), `blob` (đọc blob nhị phân có giới hạn; LFS đọc thẳng kho object cục bộ — không gọi mạng, không bật hỏi mật khẩu), `file_history` (`--follow`, phân trang), `blame` (`--porcelain`, tại revision hoặc working tree), `diff::image_pair`.
- **UI:** conflict editor 3 khung (Của tôi / Của họ chỉ đọc, Kết quả sửa được) với nút theo khối (lấy của tôi / của họ / cả hai theo 2 thứ tự / base), điều hướng + đếm khối, cảnh báo còn marker khi lưu, tự mở file xung đột tiếp theo; nhãn giải thích rõ ours/theirs bị **đảo** khi rebase. Xung đột nhị phân/LFS và modify/delete: 2 thẻ với dung lượng, xem trước ảnh, nút giữ một phía. Image diff: Song song / Trượt / Chồng mờ, kích thước pixel + dung lượng, ảnh LFS hiện nội dung thật + thông tin pointer + "Tải file này" khi thiếu object. File history, blame (ảo hóa, tô màu theo tuổi, nhóm dòng theo commit, bấm để nhảy tới commit, "blame tại commit cha"), xem file tại commit. Lối vào: menu chuột phải file ở working tree và chi tiết commit, palette.
- Hạn chế: chỉ nhận marker 7 ký tự mặc định; file không phải UTF-8 chỉ giải quyết bằng cách lấy nguyên một phía; blame chưa tô màu cú pháp.

**3.3 — Submodule, worktree, LFS nâng cao, signing ✅ (2026-10-15)**
- **Submodule:** sidebar liệt kê cả lồng nhau với trạng thái (chưa tải / đúng commit / lệch commit / có thay đổi / xung đột) và tooltip commit ghi nhận vs đang checkout; tải/cập nhật (tiến độ + Hủy), checkout đúng commit ghi nhận, sync URL, thêm, deinit, xóa, mở thành tab. Tài khoản chọn **theo URL của từng submodule** (URL tương đối → tài khoản của repo cha → rule/binding → tài khoản cùng host → mặc định, kèm cảnh báo khác host).
- **Worktree:** thêm (nhánh có sẵn / nhánh mới, gợi ý thư mục), mở thành tab, xóa (hỏi `--force` khi còn thay đổi), dọn (xem trước), khóa/mở khóa. 4 `FailureKind` mới có nhãn + hướng dẫn: `checkedOutElsewhere`, `dirtyWorktree`, `worktreeLocked`, `signingFailed`.
- **LFS:** hộp thoại "Git LFS" 4 tab — File (đã tải / pointer / chưa tải, tải 1 hoặc nhiều file), Khóa (lock / unlock / force unlock có xác nhận, lỗi dịch dễ hiểu; badge 🔒 trong danh sách file, đỏ khi file mình sửa bị người khác khóa), Pattern (bật/tắt `lockable`), Bảo trì (dung lượng kho cục bộ, prune có xem trước, fsck, fetch --all, lối vào Push Guard để chuyển file nặng chưa push sang LFS).
- **Signing:** resolver inject `commit.gpgsign` / `gpg.format` / `user.signingkey` từ identity; form identity chọn khóa GPG (liệt kê bằng gpg mà git dùng) hoặc khóa SSH của tài khoản / file `.pub`, nút **Thử ký**. Chi tiết commit có dòng Chữ ký (Đã xác minh / Khóa chưa tin cậy / Sai / …); SSH được kiểm bằng file `allowed_signers` app tự sinh từ các identity ký SSH. Tag annotated tạo trong app ký bằng `git tag -s` (không inject `tag.gpgSign` vì nó biến mọi tag thường thành tag cần message).
- Hạn chế: lock LFS mới test phần đọc/phân loại lỗi (test server không có lock API) — cần thử với GitHub/GitLab thật; chưa upload SSH signing key lên provider; submodule riêng tư ở host chưa có tài khoản sẽ lỗi đăng nhập (UI có cảnh báo).

**3.4 — Tích hợp & kiểm chứng end-to-end ✅ (2026-10-15)** (bản release, Windows 10, điều khiển qua CDP, repo thử `.agent/test-repos/p3-e2e` có submodule + remote; DB thật được sao lưu trước và khôi phục sau)
- Đã bấm thử trên app thật: image diff (Song song / Chồng mờ); commit → `Ctrl+Z` (thay đổi quay lại vùng Staged) → `Ctrl+Shift+Z` (trả lại đúng commit cũ, cùng hash); merge có xung đột → conflict editor → "Lấy cả hai" → Lưu → Tiếp tục → Undo merge; interactive rebase (đổi thứ tự + reword message tiếng Việt, có backup ref) → Undo; kéo `topic` thả lên `main` → Rebase; sidebar Submodules/Worktrees; thêm worktree cho `conf` rồi checkout `conf` ở repo chính → thông báo "nhánh đang checkout ở worktree khác" kèm cách sửa; hộp thoại Git LFS; file history; blame; badge chữ ký SSH ("Khóa chưa tin cậy").
- **Lỗi phát hiện và đã sửa khi kiểm chứng:**
  - Toolbar **nhân đôi nhóm nút** Fetch/Pull/Push/Branch mỗi lần chuyển tab: `RemoteButtons` và `UndoButtons` (mới) cùng cấp và cùng `key = repo id`. Đổi sang key có tiền tố, và sửa luôn cặp trùng có từ trước trong `AppShell` (`DiffView`/`WorkingTreePanel`). Có test `Toolbar.test.tsx`.
  - **Màn hình trắng** khi mở conflict editor: CSS bản release bị minify `#FFFFFF` → `#fff`, Monaco không nhận hex rút gọn nên sập khi dựng theme. Lỗi này làm sập **mọi** editor Monaco ở theme Light trên bản release, không riêng conflict editor. Đã chuẩn hóa màu (`features/diff/color.ts`, có test), và thêm `ViewErrorBoundary` cho vùng giữa: lỗi hiển thị giờ chỉ hiện thông báo + nút Đóng thay vì làm trắng cả cửa sổ.
  - Graph vẽ lại **commit cũ từ backup ref** (`refs/nanomgg/backup/*`) sau rebase, và lần refresh tăng dần còn chèn chúng lên đầu làm rối lane: `git log --all` giờ có `--exclude=refs/nanomgg/*` (có test).
  - Test `ImportWizard.test.tsx` chưa cập nhật theo trường `reauth` của bước 2.9.
- **Test cuối:** Rust toàn workspace qua (git-engine 163, app 80, lfs 35 + 2 tích hợp…), clippy `-D warnings`, rustfmt; frontend 491 test / 73 file, typecheck, eslint, prettier sạch. Bộ cài: `target/release/bundle/nsis/NANOMultipleGitGUI_0.0.1_x64-setup.exe` (6,5 MB), `…/msi/…msi` (10 MB).
- **Vấn đề có từ trước (chưa sửa):** test `terminal::tests::close_all_and_vanished_ui_end_sessions` treo vô thời hạn trên máy dev (binary build từ code trước Phase 3 cũng treo y hệt). Chạy riêng phần "UI biến mất" thì qua; chỉ treo khi ngay trước đó vừa đóng 2 phiên ConPTY: lần spawn thứ ba mất ~12 s và tiến trình con không bao giờ xuất output. Có thể ứng với lỗi thật (mở terminal mới sau khi đóng hết terminal có thể bị treo trên Windows 10) — cần điều tra riêng; tạm chạy test app với `--skip terminal::tests::close_all_and_vanished_ui_end_sessions`.
- Chưa kiểm chứng trên app thật: signing qua form identity (chỉ có test tự động: commit ký SSH → `G`), lock LFS với server thật, submodule qua mạng với tài khoản.

### Phase 3B — Trải nghiệm cho người mới (3–4 tuần)
App nhắm tới cả dev mới học git: mọi chỗ phải nói rõ "chuyện gì xảy ra" và "bấm gì để sửa", không bắt người dùng đọc log hay gõ lệnh.

**3B.1 — Tự quét & đồng bộ tài khoản từ `gh` / `glab` CLI**
- Hiện trạng (bước 2.3): wizard nhập cấu hình đọc *tên* tài khoản trong `hosts.yml` của gh/glab nhưng **không lấy token**, nên người dùng vẫn phải đăng nhập lại từng tài khoản.
- Làm thêm:
  - Quét khi khởi động (và nút "Quét lại" trong Quản lý tài khoản): liệt kê mọi tài khoản đã `gh auth login` / `glab auth login` trên máy, kể cả nhiều tài khoản cùng host (gh ≥ 2.40 hỗ trợ nhiều tài khoản/host).
  - Tài khoản chưa có trong app hiện thẻ "Tìm thấy N tài khoản đã đăng nhập bằng CLI — Thêm vào app" (chọn từng cái).
  - **Chỉ khi người dùng bấm đồng ý** mới lấy token: `gh auth token --hostname <h> --user <u>` / `glab config get token --host <h>` (token có thể nằm trong keyring hệ điều hành, CLI tự đọc). Token lấy về được kiểm tra scope như PAT (§4.2), lưu vào keychain của app (không sửa gì cấu hình của gh/glab), ghi nguồn `auth_method = imported_cli`.
  - Thiếu scope (ví dụ gh mặc định không có `workflow`) thì báo rõ và gợi ý `gh auth refresh -s workflow` hoặc đăng nhập lại bằng device flow trong app.
  - Đồng bộ hai chiều *không* làm: app không ghi ngược vào gh/glab. Token gh bị thu hồi thì app báo "cần đăng nhập lại" như mọi tài khoản khác.
- Sau khi thêm, tài khoản dùng được ngay trong account pill / switcher / `Ctrl+Alt+1..9`, thay vì phải gõ `gh auth switch`.

**3B.2 — Onboarding từng bước lần đầu chạy**
1. **Welcome:** logo, giới thiệu NANOMultipleGitGUI là dự án open-source (Apache-2.0) do **NANO Pret** phát triển, link repo GitHub, chọn ngôn ngữ + theme.
2. **Kiểm tra công cụ:** quét git (≥ 2.35), git-lfs (≥ 3.0), OpenSSH, (tùy chọn) gh/glab CLI, WebView2 (Windows). Mỗi mục hiện ✅/⚠️/❌ kèm phiên bản.
   - Thiếu thì có nút **[Mở trang tải chính thức]** (git-scm.com, git-lfs.com, cli.github.com…) và, nếu được, **[Cài giúp tôi]** 1 click: Windows dùng `winget install --id Git.Git -e` / `GitHub.GitLFS`, macOS dùng `xcode-select --install` hoặc `brew install git-lfs`. Cài xong bấm "Kiểm tra lại". Không cho qua bước sau khi thiếu git (bắt buộc); git-lfs chỉ cảnh báo.
3. **Tài khoản:** nhập từ CLI (3B.1) / `.gitconfig` / `.ssh` (wizard có sẵn) hoặc đăng nhập mới.
4. **Hoàn tất:** tóm tắt; nút [Mở repo] / [Clone] / [Đóng].
- Chạy lại được từ Settings → "Chạy lại hướng dẫn ban đầu". Bỏ qua được nhưng ghi nhớ bước nào chưa xong.
- Trình cài đặt NSIS/MSI giữ đơn giản (cài vào `%LOCALAPPDATA%`, cuối có "Mở NANOMultipleGitGUI ngay"); việc kiểm tra công cụ nằm trong app vì installer không nên tự cài phần mềm khác.

**3B.3 — Thư mục dữ liệu tùy chỉnh (log, cache, dữ liệu)**
- Settings → "Lưu trữ": liệt kê từng loại thư mục với **dung lượng đang dùng**, đường dẫn, nút [Mở thư mục] / [Đổi…] / [Về mặc định]:
  | Loại | Mặc định (Windows) | Xóa thì sao |
  |---|---|---|
  | Log | `%LOCALAPPDATA%\<id>\logs` | Không ảnh hưởng; chỉ mất lịch sử để gửi báo lỗi |
  | Cache (danh sách repo, avatar, graph) | `%LOCALAPPDATA%\<id>\cache` | Không mất dữ liệu; lần mở sau chậm hơn một chút vì tải lại |
  | Dữ liệu app (SQLite, SSH key do app tạo) | `%APPDATA%\<id>` | **Mất tài khoản, rule, binding, key** — chỉ cho đổi vị trí, không có nút xóa; token vẫn nằm trong keychain |
  | Thư mục clone mặc định | theo tài khoản | Là code của người dùng — app không bao giờ xóa |
- Đổi đường dẫn: kiểm tra quyền ghi + dung lượng trống, **di chuyển dữ liệu cũ** sang (có tiến độ), ghi vị trí mới vào một file trỏ nhỏ ở vị trí mặc định (để lần khởi động sau tìm thấy). Dữ liệu app chỉ đổi được khi app khởi động lại.
- Nút **[Xóa toàn bộ cache]** và **[Xóa log cũ]** có hộp thoại nói rõ hậu quả như bảng trên.
- Cảnh báo khi ổ chứa dữ liệu app còn < 1 GB.

**3B.4 — Check for Update theo bản phát hành trên GitHub**
- `tauri-plugin-updater` đọc `https://github.com/<owner>/<repo>/releases/latest/download/latest.json` (chỉ bản **đã Publish**, không tính draft/pre-release; kênh beta đọc file riêng).
- Kiểm tra khi khởi động (tối đa 1 lần/ngày) + menu "Kiểm tra cập nhật"; có bản mới thì hiện phiên bản, ghi chú phát hành, nút [Cập nhật & khởi động lại] / [Để sau] / [Bỏ qua bản này].
- Gói cập nhật ký bằng khóa minisign của Tauri (§8.4); `release.yml` bật `uploadUpdaterJson`.
- Hướng dẫn cho maintainer: `docs/RELEASING.md` (đã viết — các bước tạo khóa ký, đặt secret, đánh version, tag, publish).

**3B.5 — Layout co giãn ở mọi kích thước cửa sổ**
- Lỗi hiện tại: thu nhỏ cửa sổ thì một số nút bị che/mất, không chỉnh được bề rộng các panel.
- Làm: sidebar / graph / panel phải **kéo chia được** (resizable panels, nhớ kích thước theo cửa sổ), có thể thu gọn sidebar và panel phải về dạng icon; toolbar tự dồn nút thừa vào menu "⋯" khi hẹp; dialog có thanh cuộn thay vì tràn; kiểm tra tại kích thước tối thiểu 960×600 và ở 125–150% scale của Windows.
- Test: snapshot ở 3 kích thước (960×600, 1280×800, 1920×1080) bảo đảm mọi nút chính đều nhìn thấy và bấm được.

#### Nhật ký Phase 3B (5 sub-agent song song + điều phối; báo cáo chi tiết ở `.agent/phase3b/3B.x.md`)
**Nền chung:** `build.rs` nhúng thêm `NANOMGG_GITHUB_REPO` (`owner/repo`) và `NANOMGG_UPDATER_PUBKEY` từ env hoặc `.env`; `AppInfo.repositoryUrl` (null khi build không có repo → UI ẩn link, không hard-code owner).

**3B.1 — Tài khoản từ gh / glab ✅ (2026-10-15)**
- Quét `hosts.yml` của gh (gồm `users:` nhiều tài khoản/host) và config glab; so với tài khoản đã có (host + username, không phân biệt hoa thường). Command `cli_accounts_scan` / `cli_accounts_import` / `cli_accounts_dismiss`.
- Token chỉ lấy khi bấm Thêm: gọi thẳng `gh auth token --hostname H --user U` (gh cũ không có `--user` thì chỉ thử lại khi host có đúng 1 tài khoản) / `glab config get token --host H`, không qua shell, có timeout, xóa `GH_TOKEN`/`GITLAB_TOKEN`… khỏi env, token trong `Zeroizing`. Kiểm như PAT (username phải khớp, scope, hạn); host mới thì tạo provider GHE / GitLab self-hosted sau khi token hợp lệ. Không ghi gì vào cấu hình gh/glab (có test so từng byte).
- `AuthMethod::ImportedCli` (`imported_cli`); **SQLite v4** (dựng lại bảng `accounts` vì CHECK constraint, tắt foreign key quanh transaction để giữ identity/rule/binding, kiểm tham chiếu trước commit; test nâng cấp v3→v4).
- Kết quả từng tài khoản (đã thêm / thiếu scope / token sai / sai user / CLI không trả token / chưa cài / đã có / lỗi mạng) kèm lệnh sửa sao chép được (`gh auth refresh -h H -s workflow`, `gh auth switch`…) và nút đăng nhập lại bằng device flow.
- UI `CliAccountsPanel` ở đầu Quản lý tài khoản, trong ImportWizard và bước Tài khoản của onboarding; thẻ tài khoản ghi "Nhập từ gh CLI"; lệnh palette. Notice nhẹ khi khởi động (không hiện trong phiên onboarding tự mở, khi wizard đang mở hoặc đang có notice khác); "Bỏ qua" nhớ theo đúng tập tài khoản (`cliSync.dismissed`).

**3B.2 — Onboarding ✅ (2026-10-15)**
- 4 bước: Chào mừng (logo, open-source Apache-2.0 do NANO Pret phát triển, link repo nếu có, ngôn ngữ + theme đổi ngay) → Kiểm tra công cụ (git bắt buộc, git-lfs/OpenSSH khuyến nghị, gh/glab tùy chọn, WebView2; ✅/⚠️/❌ + phiên bản + đường dẫn) → Tài khoản → Hoàn tất (tóm tắt, Mở repo / Clone / Đóng).
- "Cài giúp tôi": whitelist cố định trong Rust (frontend chỉ gửi id): `winget install --id Git.Git|GitHub.GitLFS|GitHub.cli|GLab.GLab -e --source winget …`, macOS `xcode-select --install` / `brew install`; chạy nền, hủy được, timeout 30 phút, lỗi dịch dễ hiểu (UAC từ chối, cần admin, mạng, không có winget). Không thấy trên PATH thì tìm ở thư mục cài mặc định và báo cần khởi động lại (nút khởi động lại).
- Tiến độ lưu ở `onboarding.setup`; chỉ tự mở với người dùng mới (chưa tài khoản, chưa `importDone`, chưa hoàn tất/bỏ qua) — thay cho việc ImportWizard tự mở. Settings có mục "Hướng dẫn ban đầu" (nhắc bước chưa xong + chạy lại), lệnh palette.

**3B.3 — Thư mục dữ liệu ✅ (2026-10-15)**
- Crate mới `nanomgg-app-paths`; file trỏ `%LOCALAPPDATA%\<id>\locations.json` (tự xóa khi mọi thứ về mặc định). File hỏng / thư mục mất → dùng mặc định + notice, không chặn app mở; thư mục data mất thì không tạo DB rỗng.
- Log đổi ngay (logging giữ dòng mới trong RAM lúc copy rồi ghi tiếp ở chỗ mới); data chỉ đổi khi khởi động lại (`pendingData`, chuyển trước khi mở SQLite, áp lại ACL cho SSH key và sửa đường dẫn key trong DB). Di chuyển = copy → so từng byte → chuyển → xóa bản cũ; lỗi giữa chừng giữ nguyên bản cũ. Thư mục đã có file thì dùng thư mục con riêng; cấm chọn trong/chứa thư mục của app.
- Settings → Lưu trữ: dung lượng, đường dẫn, ổ trống, Mở / Đổi… / Về mặc định, Xóa toàn bộ cache, Xóa log cũ (giữ file hôm nay), cảnh báo ổ data < 1 GB (lúc mở + mỗi 30 phút). Không liệt kê `EBWebView` (bị khóa khi cửa sổ chạy và chứa localStorage). Hiện chưa có cache nào ghi xuống đĩa; "Xóa cache" xóa cả cache danh sách repo clone trong RAM.

**3B.4 — Check for Update ✅ (2026-10-15, chờ repo + khóa ký)**
- `tauri-plugin-updater` cấu hình lúc chạy (pubkey + endpoint `https://github.com/<repo>/releases/latest/download/latest.json`); thiếu repo hoặc pubkey → "Bản build này chưa bật tự cập nhật", không gọi mạng (`ensure_plugin_config` chèn `plugins.updater` rỗng để plugin không làm app sập). Kênh beta đọc thêm pre-release cố định `updater-beta` và lấy bản cao hơn.
- Kiểm tra khi khởi động tối đa 1 lần/ngày (sau 5 giây, chờ dialog khác đóng), Settings → Cập nhật, lệnh palette; hộp thoại có ghi chú phát hành (chữ thường, không HTML), tiến độ, [Cập nhật & khởi động lại] / [Để sau] / [Bỏ qua bản này]. Restart từ Rust, không cấp quyền updater cho JS.
- `release.yml`: bật `createUpdaterArtifacts` + `uploadUpdaterJson` chỉ khi có secret `TAURI_SIGNING_PRIVATE_KEY` và variable `NANOMGG_UPDATER_PUBKEY`; luồng SignPath tạo lại `.sig` từ installer đã ký; job `beta-channel` khi publish pre-release. `docs/RELEASING.md` viết lại. Đoạn `jq` gộp `latest.json` chưa chạy thử (máy dev không có jq).

**3B.5 — Layout co giãn ✅ (2026-10-15)**
- Tay nắm kéo chia tự viết bằng pointer events (min/max, ←/→, Home/End, double-click về mặc định), kích thước lưu localStorage (đọc trước lần vẽ đầu). Cửa sổ hẹp: panel co về tối thiểu rồi tự thu sidebar, sau đó panel phải về dạng icon (graph giữ ≥ 440 px); `Ctrl/Cmd+B`, `Ctrl/Cmd+Alt+B`, popover theo mục ở dạng icon; nội dung panel thu gọn vẫn giữ (bản nháp commit không mất).
- Toolbar dồn nút thừa vào "⋯" (không nhân đôi DOM, Pull/Push giữ menu con); TopBar tab co/cuộn ngang; StatusBar ẩn dần; `dialog.tsx` cuộn trong cửa sổ cho mọi dialog; graph ẩn cột Tác giả rồi Ngày khi hẹp.
- Kiểm bằng Edge headless trên bản web dev với mock `?mock=shell` (chỉ có trong dev, đã kiểm `dist` không chứa): 13 nút chính thấy và bấm được ở 960×600, 1280×800, 1920×1080, 150 %, 853×533, 640×520. `minWidth/minHeight` 960×600 là kích thước logic; màn 1280×800 ở 150 % (≈ 853×533) nhỏ hơn mức tối thiểu — layout vẫn chạy ở đó, có thể hạ min (vd. 800×500) nếu muốn.

**3B.6 — Tích hợp & kiểm chứng ✅ (2026-10-15)** (bản release, Windows 10, CDP; dữ liệu thật sao lưu ở `.agent/backup/pre-3b/` và đã khôi phục, so khớp từng byte)
- Nối `CliAccountsPanel` vào bước Tài khoản; notice gh/glab dùng cùng cờ tự mở của onboarding.
- Người dùng mới: onboarding tự mở → công cụ (Git 2.47, LFS 3.5.1, OpenSSH 9.9p1, gh 2.96, glab thiếu, WebView2 155) → bước Tài khoản liệt kê 2 tài khoản gh thật (không lấy token) → Hoàn tất lưu `completed`. Updater báo chưa cấu hình. Lưu trữ: đổi log sang thư mục khác rồi về mặc định (tức thì); chọn thư mục trong thư mục app bị chặn; đổi data → khởi động lại → DB ở chỗ mới, notice "Đã chuyển…", tiến độ onboarding còn nguyên → về mặc định → khởi động lại, file trỏ tự xóa.
- Người dùng cũ (bản sao DB thật, schema v3, qua chính file trỏ của 3B.3): migrate lên v4, tài khoản còn nguyên, onboarding không tự mở, notice "Tìm thấy 1 tài khoản" (tài khoản gh đã có trong app được nhận ra).
- Layout trên bản release 960×600 / 1280×800 / 1920×1080: các nút chính thấy, không tràn ngang; Settings và Lưu trữ cuộn trong cửa sổ.
- **Sửa khi kiểm chứng:** ở 960×600 khi có 2 banner (notice gh + "chưa gắn tài khoản") danh sách file của panel "Thay đổi" bị ép còn 0 px → danh sách giữ tối thiểu ~7rem, cả panel cuộn tới nút Commit. Lỗi lint (đọc ref trong render) ở phần nối notice.
- **Test cuối:** Rust toàn workspace qua (app 118, git-engine 163, app-paths 22, import 30, storage 17, accounts 31, auth 34, lfs 44, logging 10…), clippy `-D warnings`, rustfmt; frontend 567 test / 81 file, typecheck, eslint, prettier sạch. Bộ cài: `NANOMultipleGitGUI_0.0.1_x64-setup.exe` (6,9 MB), `…msi` (10,5 MB).
- **Chưa kiểm chứng trên app thật:** bấm "Thêm" tài khoản gh (sẽ chép token thật vào keychain — để người dùng tự thử), "Cài giúp tôi" (cài thật), luồng cập nhật thật (cần repo + khóa ký), kéo tay nắm / `Ctrl+B` trên cửa sổ thật (đã thử trên bản web dev), scale 125–150 % thật của Windows.
- Hạn chế: hủy cài đặt chỉ dừng winget/brew (trình cài đã nâng quyền UAC có thể chạy tiếp); lỗi winget nhận diện qua chữ tiếng Anh; macOS mở từ Finder có PATH tối thiểu nên công cụ cài bằng brew có thể không được thấy; chuyển data lúc khởi động không có thanh tiến độ; độ rộng panel lưu chung cho mọi repo.

### Phase 4 — Tích hợp provider (4–6 tuần)
- PR/MR: danh sách, tạo, review, comment, checkout, merge.
- Issues, trạng thái CI, panel "Cần review" gộp mọi tài khoản.
- Fork & clone, tự thêm upstream.
- 🚀 v0.5.

#### Các bước của Phase 4 (3 sub-agent song song; báo cáo ở `.agent/phase4/4.x.md`)
Nền chung (điều phối): `hosting.rs` (remote → dự án trên GitHub/GitLab + client API của đúng tài khoản resolver chọn, có override "chỉ cho thao tác này"), store `useHostingNav` để mở PR/MR hoặc issue từ bất kỳ đâu.
- **4.1 — PR / MR:** danh sách theo repo (mở / đóng / đã merge, lọc theo tác giả / reviewer / nhãn), xem chi tiết (mô tả, commit, file thay đổi với diff, trạng thái review, conflict / mergeable), comment chung + comment theo dòng, review (approve / request changes / comment), tạo PR/MR từ nhánh (chọn nhánh đích, draft, reviewer, nhãn; gợi ý push trước nếu nhánh chưa lên remote), checkout nhánh PR/MR (kể cả từ fork: `refs/pull/N/head`, `refs/merge-requests/N/head`), merge (merge / squash / rebase theo quyền của repo, xóa nhánh nguồn).
- **4.2 — Issues & panel "Việc của tôi":** issues theo repo (lọc, xem, comment, tạo, đóng / mở lại, gán nhãn / người), tạo nhánh từ issue (`feature/123-tieu-de`), liên kết issue trong commit message (`#123`, `Closes #123`); panel "Việc của tôi" gộp **mọi tài khoản**: PR/MR cần tôi review, PR/MR của tôi, issue gán cho tôi — mỗi dòng có màu tài khoản, bấm để mở.
- **4.3 — CI & Fork:** trạng thái CI của commit (GitHub check runs + commit statuses, GitLab pipelines): badge trên graph cho các commit gần đầu nhánh, chi tiết trong "Chi tiết commit" (job, trạng thái, thời gian, link log), `✓ CI` ở status bar cho HEAD, tự làm mới khi đang chạy; component tóm tắt checks dùng lại được cho PR/MR. Fork & clone: fork repo về tài khoản đã chọn (chọn org/namespace), chờ fork sẵn sàng, clone fork qua luồng clone có sẵn, tự thêm `upstream`; với repo đang mở: "Fork repo này" thêm remote fork và gắn tài khoản.

#### Nhật ký Phase 4
**4.1 — PR / MR ✅ (2026-10-15)** — crate providers `pulls.rs` (+ `pulls/github.rs`, `pulls/gitlab.rs`, type `Pull…`): list (GitHub lọc tác giả/reviewer/nhãn/đã merge qua `/search/issues`), detail, reviews, commits, files, comments, comment, review, create, merge, đóng/mở lại; GitLab dùng `iid`, thiếu `/approvals` → `approvalsUnavailable`, thiếu `/diffs` → `/changes`. App `pulls.rs` 14 command (lỗi `PullCommandError` kèm `accountId`); checkout qua remote của project PR với account context đúng (cùng repo → nhánh tracking; fork → `refs/pull/N/head` / `refs/merge-requests/N/head`, trùng tên → `-prN`); diff cục bộ khi provider không gửi patch. UI `features/pulls/`: mục PR/MR ở sidebar (cả rail), view giữa (Tổng quan / Commits / Files + bình luận theo dòng), hộp Review, Merge (method + xóa nhánh nguồn), hộp thoại tạo PR (Push trước khi chưa push; lối vào: sidebar, menu nhánh, palette, notice sau push nhánh mới), "với tư cách ● tài khoản ▾" cho mọi thao tác ghi. Nội dung provider hiển thị dạng chữ (chỉ link `https://`). Hạn chế: GitLab chưa có merge "rebase", chưa tạo MR từ fork; chưa resolve luồng / sửa-xóa comment.

**4.2 — Issues & "Việc của tôi" ✅ (2026-10-15)** — providers `issues.rs` (lọc bỏ PR khỏi issues GitHub, search khi có chữ, GitLab `assignee_ids`) + `mywork.rs` (3 truy vấn song song mỗi tài khoản). App `issues.rs` (theo repo qua `remote_client`, hoặc theo `accountId + fullName` để xem repo không mở; `issue_branch_create` → `feature/<số>-<slug không dấu>` qua `check-ref-format`, có Undo) + `mywork.rs` (mỗi tài khoản một task, lỗi riêng từng tài khoản, cache 60 s, hủy). UI: mục Issues ở sidebar, view issue, tạo issue, gợi ý `#` trong ô commit (Enter `#123`, Shift+Enter `Closes #123`), `#123`/`!12` trong chi tiết commit thành link, nút "Việc của tôi" trên toolbar (làm mới 5 phút) với 3 nhóm + lọc tài khoản.

**4.3 — CI & Fork ✅ (2026-10-15)** — providers `ci.rs` (GitHub check runs + combined status, GitLab pipeline + jobs) và `forks.rs` (tạo fork, namespace được phép, chờ sẵn sàng, tìm fork có sẵn). App `ci.rs` (`ci_statuses` tối đa 40 sha, 4 request song song, hỏi thêm project hosted khác khi remote đầu không có CI; cache 10 phút / 2 phút / 15 s) và `forks.rs` (fork & clone qua `clone_start` có sẵn rồi thêm `upstream` cùng giao thức; fork repo đang mở → remote `fork` hoặc đổi origin↔upstream, binding khóa cho tài khoản fork). UI `features/ci/` (badge, danh sách job, `CiSummary` dùng trong PR) + `features/forks/` (tùy chọn trong Clone dialog, menu remote ở sidebar, palette).

**4.4 — Tích hợp & kiểm chứng ✅ (2026-10-15)** (bản release, CDP; bản sao DB thật qua file trỏ của 3B.3 — DB thật so khớp từng byte sau khi xong; **chỉ thao tác đọc** trên `congluuit/NANOMultipleGitGUI`)
- Nối: `CiSummary` vào chi tiết PR; badge CI trên graph qua điểm móc `decorate` của `drawGraph` (đầu mỗi nhánh + 20 commit mới nhất, tooltip trạng thái khi rê lên badge); gỡ `allow(dead_code)` của `hosting.rs`.
- Đã thấy trên app thật: remote `origin` nhận đúng tài khoản; PR/Issues đọc được (repo chưa có PR/issue); CI của HEAD lấy đúng từ GitHub Actions — badge ✗ trên graph, danh sách job trong chi tiết commit, `✗ CI` ở status bar; "Việc của tôi" gộp 2 tài khoản và tìm thấy PR `congluuit/quanviahe#2`; bấm vào mở view PR (mô tả, timeline, khung CI, hộp Merge "với tư cách ● Sigrid").
- **Phát hiện:** CI GitHub Actions của repo dự án đang **thất bại** ở 3 job Rust (ubuntu / macOS / Windows), job Frontend qua.
- Chưa thử thật: tạo PR, comment, review, merge, checkout PR từ fork, tạo issue, fork & clone (đều có test wiremock / git thật).
- **Test cuối:** Rust toàn workspace qua (app 136, git-engine 163, providers + test mới của pulls/issues/ci/forks…), clippy `-D warnings`, rustfmt; frontend 626 test / 88 file, typecheck, eslint, prettier sạch. Bộ cài: `NANOMultipleGitGUI_0.0.1_x64-setup.exe` (7,4 MB), `…msi` (11,3 MB).

**3B.7 — Sửa theo góp ý của người dùng ✅ (2026-10-15)**
- `.env.example` song ngữ: 4 khóa `build.rs` đọc từ `.env` (đều không bắt buộc) + các biến chỉ có tác dụng qua môi trường (`TAURI_SIGNING_PRIVATE_KEY`…).
- Danh sách file: nút "Stage" / "Bỏ stage" / "Hoàn tác" dạng chữ thay cho icon `+`/`−`/`↶`; "Stage tất cả" / "Bỏ stage tất cả" có viền + icon.
- **Lỗi vừa đăng nhập lại vẫn bị đòi đăng nhập lại:** GitHub cấp token OAuth hết hạn sau 8 giờ (+ refresh token 6 tháng) nhưng app chỉ tự làm mới token GitLab, nên token GitHub luôn ở trạng thái "hết hạn hôm nay" kèm nút "Đăng nhập lại", và thật sự hết hạn sau 8 giờ. Nay app tự làm mới mọi token OAuth có refresh token (`refresh_github_token`, device flow không cần client secret; `bad_refresh_token` → yêu cầu đăng nhập lại). Kiểm trên app thật: tài khoản GitHub chuyển sang "tự làm mới".
- Graph: phần graph căn giữa cột "Nhánh / Graph"; kéo đổi độ rộng cột Graph / Tác giả / Thời gian (nhấp đúp về mặc định, lưu localStorage), mặc định cột commit message rộng nhất (≥ 1,5 lần cột khác); tooltip khi rê chuột lên ô bị cắt chữ (message, tác giả, thời gian; message kèm tác giả/ngày khi các cột đó bị ẩn vì cửa sổ hẹp). Bố cục cột dùng chung một hàm cho canvas, header và hit-test.
- Ngôn ngữ: English đứng trước Tiếng Việt trong lựa chọn. Giữ nguyên kích thước cửa sổ tối thiểu 960×600 (quyết định của người dùng).

### Phase 5 — Hoàn thiện v1.0 (4–6 tuần)
- Tối ưu hiệu năng (repo lớn như linux/chromium, repo LFS hàng chục GB), giảm RAM, startup < 1.5s.
- Accessibility, phím tắt tùy biến, onboarding.
- Auto-update stable/beta, Windows đã ký qua SignPath, tài liệu người dùng + FAQ cài đặt.
- 🚀 **v1.0**.

### Sau v1.0
- Bitbucket, Gitea/Forgejo, Azure DevOps.
- Theme High Contrast, cho phép người dùng tạo theme riêng.
- Hệ thống plugin/extension.
- Gợi ý commit message bằng AI (tùy chọn, dùng key của người dùng hoặc model local).
- Đồng bộ cấu hình tài khoản (không gồm secret) giữa nhiều máy qua file export mã hóa.

---

## 11. Chiến lược kiểm thử
- **Rust unit test:** parser output git, rule engine, lane layout của graph, redact log, parser file tiến độ `GIT_LFS_PROGRESS`.
- **Integration test:** tạo repo tạm (`tempfile`) và chạy thao tác thật (commit, rebase, conflict...) trên cả 3 OS trong CI.
- **Credential test:** dựng git server HTTP cục bộ (`git http-backend`) và SSH server test (container) với 2 user, xác nhận đúng tài khoản được dùng khi push.
- **LFS test:** dựng **Gitea** (hỗ trợ LFS, nhiều user) trong container trên CI; tạo file ngẫu nhiên 200 MB+; kiểm tra:
  - Large File Guard bắt được file vượt ngưỡng và track LFS đúng,
  - pre-push phát hiện blob nặng, `lfs migrate` chỉ viết lại commit chưa push,
  - upload LFS đi đúng tài khoản (user A không ghi được vào repo của user B),
  - hủy giữa chừng rồi push lại chỉ upload phần còn thiếu,
  - helper không trả token cho `lfs.url` trỏ sang host khác.
- **Provider API:** mock bằng `wiremock` cho GitHub/GitLab; thêm bộ test thủ công với tài khoản test thật trước mỗi release.
- **Frontend:** Vitest + Testing Library; E2E bằng WebdriverIO + `tauri-driver` (Windows/Linux) cho các luồng chính: clone, commit, đổi tài khoản, push, push file nặng.
- **Theme:** snapshot test giao diện ở cả Light và Dark; kiểm tra tương phản tự động cho các cặp token chữ/nền.
- **Hiệu năng:** benchmark mở repo lớn và cuộn graph, đặt ngưỡng để fail CI khi bị chậm đi.
- **Cài đặt:** checklist thủ công mỗi release — cài `.dmg` trên macOS 14 và 15 theo đúng `INSTALL-macOS.md`, cài `.exe` trên Windows 10 và 11.
- Bộ **fixture repo**: lịch sử phức tạp, nhiều merge, file lớn, repo LFS, tên file Unicode/tiếng Việt, CRLF, submodule.

---

## 12. Rủi ro & cách giảm thiểu
| Rủi ro | Giảm thiểu |
|---|---|
| Graph chậm với repo lớn | Đọc bằng gix, tính lane theo từng phần (incremental), virtualization, Canvas/WebGL, cache theo HEAD |
| Parse output git sai ở edge case | Chỉ dùng format máy đọc (`-z`, porcelain v2, `--json`), test fixture đa dạng, ghi rõ version tối thiểu |
| Khác biệt Windows (path, CRLF, quyền file, long path) | CI chạy trên Windows, bật `core.longpaths`, test tên file Unicode |
| Lộ token (kể cả qua LFS server bên thứ ba) | Keychain + sidecar IPC có token phiên, kiểm tra host trước khi trả token, redact log, review bảo mật trước v0.1 |
| macOS thiếu git-lfs | Tự phát hiện, tải bản chính thức có kiểm tra SHA-256, hoặc hướng dẫn Homebrew |
| Hết quota LFS / server giới hạn kích thước | Ước lượng dung lượng trước khi upload, dịch lỗi rõ ràng, link tới trang billing đúng tài khoản |
| Viết lại lịch sử khi migrate LFS gây mất dữ liệu | Chỉ áp dụng cho commit chưa push, luôn tạo backup ref, có Undo |
| Org GitHub chặn OAuth app | Phát hiện và hướng dẫn dùng fine-grained PAT; lộ trình chuyển sang GitHub App |
| GitLab self-hosted nhiều phiên bản khác nhau | Phát hiện version qua `/api/v4/version`, tắt dần tính năng không hỗ trợ |
| macOS không ký số khiến người dùng ngại cài | Hướng dẫn cài từng bước có ảnh, ký ad-hoc để tránh lỗi "damaged", FAQ về cảnh báo Keychain sau cập nhật |
| SignPath duyệt lâu hoặc từ chối | Phát hành bản chưa ký kèm hướng dẫn SmartScreen trong lúc chờ; dự phòng Azure Trusted Signing nếu cần |
| Scope creep (bám theo toàn bộ GitKraken) | Ưu tiên multi-account + LFS (Phase 2) trước các tính năng "nice to have"; phát hành sớm, lặp nhanh |
| Rủi ro thương hiệu / giống GitKraken | Tên, logo, bảng màu (nền ấm + cam ember) và icon tự thiết kế; không dùng asset của GitKraken |

---

## 13. Quyết định

### 13.1 Đã chốt
| Hạng mục | Quyết định |
|---|---|
| Tên dự án | **NANOMultipleGitGUI** (mã ngắn `nanomgg`) |
| License | **Apache-2.0** |
| Stack | **Tauri 2 + Rust + React/TypeScript** |
| Theme | **Light / Dark / System**, bảng màu nền xám ấm + cam ember, khác GitKraken |
| macOS | **Không ký Developer ID**, chỉ ký ad-hoc, kèm hướng dẫn cho phép cài |
| Windows | **Ký miễn phí qua SignPath Foundation** (chương trình OSS); bản chưa ký kèm hướng dẫn trong lúc chờ duyệt |
| Provider ở v1.0 | **Chỉ GitHub + GitLab** (gồm GitHub Enterprise và GitLab self-hosted). Bitbucket, Gitea/Forgejo, Azure DevOps thêm sau v1.0 |
| Ghi identity vào `.git/config` | **Mặc định có**, kèm tùy chọn tắt (toàn cục và theo repo); `core.sshCommand` vẫn opt-in |
| Large File Guard cho GitLab | **Cảnh báo > 50 MB, không chặn cứng** (giới hạn khác nhau theo instance; người dùng tự đặt ngưỡng chặn nếu muốn) |

| Bảng màu | **Đã chốt** ở bước 0.2 (mục 7.2), đạt WCAG AA, kiểm tra tự động |
| Đọc lịch sử | **git CLI** với output máy đọc; gix chỉ thêm khi đo thấy nghẽn (ADR 0002) |
| Hủy tiến trình trên Windows | **Windows Job Object** (2026-10-14): hủy fetch/pull/push/LFS giết cả cây tiến trình, kể cả con của hook qua MSYS. Được thêm dependency và cho phép `unsafe` trong đúng một module nếu cần, chỉ dùng API Job Object chuẩn (không gây nhận diện nhầm của Windows Defender) |

### 13.2 Còn cần chốt
1. Tài khoản/tổ chức GitHub sẽ chứa repo — người dùng sẽ tự tạo và upload sau. Khi có: đổi bundle identifier (đang tạm `io.github.nanomgg.nanomultiplegitgui`), điền `repository` trong Cargo/package.json, thay `OWNER/REPO` trong `.github/release-notes.md`, đăng ký OAuth app GitHub/GitLab.
2. Kênh liên hệ cho Code of Conduct (email hoặc kênh riêng của maintainer) — điền vào `[INSERT CONTACT METHOD]`.
3. **OAuth app** dưới tổ chức của dự án (cần cho đăng nhập bằng trình duyệt; PAT dùng được ngay không cần):
   - GitHub: Settings → Developer settings → OAuth Apps → New, **bật "Enable Device Flow"**, không cần client secret → điền Client ID vào provider GitHub trong app (tab Máy chủ) hoặc nhúng lúc build qua `NANOMGG_GITHUB_OAUTH_CLIENT_ID` (biến môi trường khi chạy `pnpm dev`/`tauri build`; trên CI là repo variable cùng tên, `release.yml` đã đọc sẵn). Khi lấy được mã, wizard tự chép mã và mở `github.com/login/device` (2026-10-14).
   - GitLab: User Settings → Applications, **bỏ "Confidential"**, Redirect URI đúng `http://127.0.0.1:47821/callback`, scope `read_user read_api read_repository write_repository` → điền Application ID (hoặc `NANOMGG_GITLAB_OAUTH_CLIENT_ID`).

---

## 14. Việc làm ngay (tuần 1)
- [x] Chốt các mục ở 13.2 (3/4 mục; còn tài khoản/tổ chức GitHub — người dùng tự upload sau).
- [x] Kiểm tra tên NANOMultipleGitGUI trên GitHub / crates.io / npm / tên miền. *(bước 0.5 — đều còn trống)*
- [ ] Tạo repo GitHub public *(người dùng làm)*. Đã có đủ: `LICENSE`, `NOTICE`, `README.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`, `PRIVACY.md`, `CHANGELOG.md`.
- [x] `pnpm create tauri-app` → chuyển sang cấu trúc monorepo như mục 3.1. *(bước 0.1)*
- [x] Thiết lập CI lint/test và workflow release ra `.exe` (chưa ký) + `.dmg` (ký ad-hoc). *(bước 0.4 — chạy thật khi có repo)*
- [x] Viết `docs/INSTALL-macOS.md` và `docs/INSTALL-Windows.md` (bản nháp, bổ sung ảnh chụp khi có bản build).
- [x] Làm mockup Light/Dark với bảng màu ở mục 7.2, chốt design tokens. *(bước 0.2)*
- [x] Viết ADR-001 (Tauri), ADR-002 (git CLI + gix), ADR-003 (credential helper sidecar), ADR-004 (LFS transfer 2 bước).
- [x] Prototype `nanomgg-credential` + push bằng 2 tài khoản trên cùng máy, **kèm 1 file LFS 200 MB** *(bước 0.6 — kiểm chứng bằng server cục bộ; thử với tài khoản GitHub thật khi có)*.
- [x] Prototype graph Canvas với repo ~50k commit để đo hiệu năng. *(bước 0.6 — cả 50k và 100k)*

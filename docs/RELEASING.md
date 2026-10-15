# Phát hành bản mới (dành cho maintainer)

Tài liệu này hướng dẫn cách đưa một bản mới lên GitHub Releases để người dùng tải về, và để app của người dùng tự thấy bản mới và cập nhật (**Check for Update**, PLAN 3B.4).

## Phát hành nhanh (tóm tắt)

Repo đã được thiết lập sẵn (mục A): khóa ký bản cập nhật, Client ID đăng nhập GitHub. Mỗi lần phát hành chỉ cần:

1. Đổi version và ghi `CHANGELOG.md` (mục B, bước 1–2). **Bản 1.0.0 đầu tiên đã sẵn sàng, bỏ qua bước này.**
2. Gắn tag và đẩy lên GitHub:
   ```bash
   git tag v1.0.0
   git push origin v1.0.0
   ```
3. Chờ khoảng 20–30 phút: GitHub tự build bộ cài Windows, macOS, Linux (xem tiến độ ở tab **Actions → Release**) và tạo một bản **nháp** (draft) trên trang **Releases**.
4. Mở trang [Releases](https://github.com/congluuit/NANOMultipleGitGUI/releases) → bản nháp → kiểm tra mục **Assets** có đủ file → bấm **Edit** → **Publish release**. Từ lúc này mọi người tải được, và app đã cài sẽ tự thấy bản mới.

**Khóa ký bản cập nhật** nằm ở `~/.tauri/nanomgg-updater.key` (mật khẩu trong `~/.tauri/nanomgg-updater.password.txt`). Hãy sao lưu cả hai vào trình quản lý mật khẩu: nếu mất, người dùng cũ sẽ không tự cập nhật được nữa và phải tải bộ cài mới bằng tay.

---

Nguyên tắc: **app chỉ thấy bản đã Publish.** Workflow `release.yml` luôn tạo bản _draft_; người dùng không thấy gì cho tới khi bạn bấm Publish trên GitHub. Kênh `stable` đọc `https://github.com/<owner>/<repo>/releases/latest/download/latest.json` — GitHub chỉ trỏ `latest` tới bản đã Publish và **không** phải pre-release, nên draft và bản beta không bao giờ lọt vào kênh stable.

---

## A. Thiết lập một lần

### A1. Biến cho build trên CI

Repo → **Settings → Secrets and variables → Actions**:

| Tab       | Tên                                  | Giá trị                                                  |
| --------- | ------------------------------------ | -------------------------------------------------------- |
| Variables | `NANOMGG_GITHUB_OAUTH_CLIENT_ID`     | Client ID của GitHub OAuth App (bật Device Flow)         |
| Variables | `NANOMGG_GITLAB_OAUTH_CLIENT_ID`     | Application ID của GitLab (không bắt buộc)               |
| Variables | `NANOMGG_UPDATER_PUBKEY`             | Public key ký gói cập nhật (bước A2) — không phải bí mật |
| Secrets   | `TAURI_SIGNING_PRIVATE_KEY`          | Nội dung file private key (bước A2)                      |
| Secrets   | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Mật khẩu của private key (bước A2)                       |

`NANOMGG_GITHUB_REPO` (`owner/repo`, dùng cho link "About" và nơi app tìm bản cập nhật) được CI tự đặt bằng `github.repository` — không cần khai báo.

### A2. Khóa ký gói cập nhật (bật Check for Update)

Tauri chỉ cài bản cập nhật có chữ ký khớp với public key nằm trong app — kể cả khi ai đó giả mạo file trên mạng. Khóa này (minisign) miễn phí và độc lập với việc ký số Windows/macOS.

1. Tạo cặp khóa (chỉ làm **một lần**, giữ kỹ private key — mất là người dùng cũ không cập nhật được nữa, phải tải bộ cài mới bằng tay):
   ```bash
   pnpm tauri signer generate -w ~/.tauri/nanomgg-updater.key
   ```
   Lệnh hỏi mật khẩu cho khóa, rồi in ra **public key** (một chuỗi base64 dài; cũng nằm trong `~/.tauri/nanomgg-updater.key.pub`).
2. Thêm vào repo (A1): secret `TAURI_SIGNING_PRIVATE_KEY` = nội dung file `~/.tauri/nanomgg-updater.key`, secret `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` = mật khẩu, variable `NANOMGG_UPDATER_PUBKEY` = nội dung file `.pub`.
3. Sao lưu private key + mật khẩu ở nơi an toàn (password manager).

Không cần sửa `tauri.conf.json`: app cấu hình updater **lúc chạy** từ hai giá trị được nhúng khi build (`NANOMGG_GITHUB_REPO`, `NANOMGG_UPDATER_PUBKEY`). Thiếu một trong hai (ví dụ build từ mã nguồn của người khác) thì app hiện "Bản build này chưa bật tự cập nhật" kèm link trang Releases, và **không gọi mạng**.

Khi secret + variable có mặt, `release.yml` tự:

- build với `bundle.createUpdaterArtifacts = true` (truyền bằng `--config`, không sửa file cấu hình) → thêm file `.sig` cạnh mỗi bộ cài / `.app.tar.gz` / `.AppImage`;
- bật `uploadUpdaterJson` của `tauri-action` → đính `latest.json` vào draft (ghi chú phát hành trong đó lấy từ mục `## [x.y.z]` của `CHANGELOG.md`).

Thiếu secret hoặc variable thì release vẫn build như cũ (không có `.sig`/`latest.json`), workflow ghi một dòng _notice_.

> **Windows ký qua SignPath** (`SIGNPATH_ENABLED=true`): bộ cài được SignPath ký **sau** khi bundle, làm đổi nội dung file, nên `.sig` được tạo lại từ bộ cài đã ký cuối cùng (`tauri signer sign`) rồi job `windows-updater-json` thêm mục `windows-x86_64` (NSIS), `windows-x86_64-nsis`, `windows-x86_64-msi` vào `latest.json` sau khi mọi job build xong. Nếu job này lỗi, xem mục C.

### A3. Thử trên máy (không bắt buộc)

File `.env` ở thư mục gốc (đã git-ignore) được `build.rs` đọc khi build cục bộ:

```dotenv
NANOMGG_GITHUB_OAUTH_CLIENT_ID=...
NANOMGG_GITHUB_REPO=owner/repo          # repo (hoặc fork) có Releases để thử
NANOMGG_UPDATER_PUBKEY=dW50cnVzdGVk...  # nội dung file .pub
```

- `pnpm tauri build` bình thường **không cần** private key: app build ra sẽ kiểm tra cập nhật trên `owner/repo` nhưng bản thân bộ cài không kèm `.sig`.
- Muốn tạo gói cập nhật có chữ ký trên máy (để tự dựng một release thử):
  ```bash
  export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/nanomgg-updater.key)"
  export TAURI_SIGNING_PRIVATE_KEY_PASSWORD='...'
  pnpm tauri build --config '{"bundle":{"createUpdaterArtifacts":true},"plugins":{"updater":{"pubkey":"<public key>"}}}'
  ```
- Kịch bản thử đầu-cuối: cài bản `0.1.0` (build có `.env` như trên), publish bản `0.1.1` trên repo thử (có `latest.json` + `.sig`), mở app → sau ~5 giây hộp thoại "Đã có phiên bản mới" hiện ra (hoặc Settings → Cập nhật → Kiểm tra cập nhật).
- Lần kiểm tra lúc khởi động bị giới hạn một lần/ngày (setting `updater.lastCheck` trong DB); bấm "Kiểm tra cập nhật" thì luôn kiểm tra.

---

## B. Mỗi lần phát hành

1. **Đánh version** (theo [SemVer](https://semver.org/): sửa lỗi tăng số cuối, thêm tính năng tăng số giữa):
   ```bash
   pnpm release:version 0.2.0        # hoặc 0.3.0-beta.1 cho bản beta
   ```
   Lệnh này sửa `package.json`, `apps/desktop/package.json` (Tauri lấy version từ đây) và version của Cargo workspace. App chỉ cập nhật lên version **lớn hơn** bản đang cài.
2. **Ghi CHANGELOG:** trong `CHANGELOG.md`, đổi `## [Unreleased]` thành `## [0.2.0] - 2026-10-20` và thêm lại một mục `## [Unreleased]` trống ở trên. Nội dung mục này là **ghi chú phát hành người dùng thấy trong hộp thoại cập nhật** (hiện dạng chữ thuần, không render HTML) — viết ngắn, dễ hiểu. Phải ghi trước khi tag: `latest.json` được tạo lúc build, sửa trang Release sau đó không đổi được ghi chú trong app.
3. **Kiểm tra:** `pnpm check` (format, lint, typecheck, test frontend + Rust).
4. **Commit và tag:**
   ```bash
   git commit -am "chore(release): v0.2.0"
   git tag v0.2.0
   git push origin main v0.2.0
   ```
   > Push file trong `.github/workflows/` cần token có quyền `workflow`. Trong NANOMultipleGitGUI, tài khoản GitHub đăng nhập từ bản 2.9 trở đi đã có sẵn quyền này; tài khoản cũ hãy "Đăng nhập lại" một lần.
5. **Chờ CI:** tab **Actions → Release** build Windows, macOS, Linux (khoảng 15–25 phút) rồi đính kèm file vào bản **draft** `NANOMultipleGitGUI 0.2.0`.
6. **Kiểm tra draft:** tải thử bộ cài, xem `SHA256SUMS.txt`; khi đã bật updater (A2) phải có `latest.json` và các file `.sig`. Mở `latest.json` xem có đủ `windows-x86_64`, `darwin-aarch64`, `darwin-x86_64`, `linux-x86_64` và `notes` đúng.
7. **Publish:** trên trang Releases, mở draft → sửa ghi chú nếu cần → **Publish release**.
   - **Bản stable:** để nguyên "Set as the latest release". Từ lúc này mọi app (kênh stable và beta) thấy bản mới.
   - **Bản beta** (`0.3.0-beta.1`): tick **Set as a pre-release**. Kênh stable không thấy. Ngay khi Publish, job `beta-channel` của `release.yml` chép `latest.json` của bản này sang pre-release cố định **`updater-beta`** (tự tạo lần đầu) — kênh beta đọc `releases/download/updater-beta/latest.json`. Đừng xóa release/tag `updater-beta`.

Từ lúc Publish, app của người dùng sẽ thấy bản mới trong lần kiểm tra kế tiếp (khi khởi động, tối đa một lần/ngày) hoặc ngay khi họ bấm **Kiểm tra cập nhật** (Settings → Cập nhật, hoặc command palette). Người dùng có thể chọn **Để sau** (hỏi lại lần kiểm tra sau) hoặc **Bỏ qua bản này** (không nhắc lúc khởi động cho tới khi có bản mới hơn).

### Kênh beta hoạt động thế nào

App ở kênh beta kiểm tra **cả hai** manifest (`releases/latest/...` và `updater-beta/...`) rồi chọn version cao nhất. Vì vậy:

- bản stable mới hơn beta cũ vẫn tới được người dùng beta;
- chưa có bản beta nào (404) thì kênh beta hoạt động như stable;
- muốn "rút" một bản beta lỗi: publish bản beta kế tiếp (hoặc xóa asset `latest.json` khỏi `updater-beta`).

---

## C. Sự cố thường gặp

| Hiện tượng                                     | Nguyên nhân / cách xử lý                                                                                                                                                 |
| ---------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Push tag bị từ chối "without `workflow` scope" | Token thiếu quyền `workflow` — đăng nhập lại tài khoản trong app (nút có sẵn trên thông báo lỗi)                                                                         |
| Workflow Release không chạy                    | Tag phải bắt đầu bằng `v` (`v0.2.0`); hoặc chạy tay: Actions → Release → Run workflow → nhập tag                                                                         |
| Draft không có `latest.json` / `.sig`          | Thiếu secret `TAURI_SIGNING_PRIVATE_KEY` hoặc variable `NANOMGG_UPDATER_PUBKEY` (xem dòng _notice_ của job "Create draft release"); thêm vào rồi build lại tag           |
| App báo "Bản build này chưa bật tự cập nhật"   | Bản đó build khi chưa có A2 (hoặc build từ mã nguồn không có `.env`). Người dùng phải cài bản mới bằng tay một lần; từ đó trở đi tự cập nhật được                        |
| App báo "Chưa có bản phát hành nào hỗ trợ…"    | Bản `latest` chưa có `latest.json` (release cũ build trước A2), hoặc release còn là draft                                                                                |
| App người dùng không thấy bản mới              | Release còn là draft hoặc là pre-release (kênh stable); version trong tag không lớn hơn version đang cài; người dùng đã "Bỏ qua bản này" (vẫn thấy khi bấm kiểm tra tay) |
| "Bản mới chưa có gói cập nhật cho máy của bạn" | `latest.json` thiếu nền tảng đó — một job build lỗi hoặc chưa xong. Build lại tag (Run workflow); với SignPath xem job `windows-updater-json`                            |
| "Gói cập nhật không có chữ ký hợp lệ"          | Build bằng private key khác với public key trong app — dùng lại đúng khóa ở A2. Với SignPath: `.sig` phải được tạo **sau** bước ký installer (đã làm trong workflow)     |
| Muốn build lại một tag đã có                   | Actions → Release → Run workflow, nhập tag; file trong draft sẽ được ghi đè                                                                                              |

## D. Ghi chú kỹ thuật

- Code: `apps/desktop/src-tauri/src/updater.rs` (command `updater_check` / `updater_install` / `updater_skip_version` / `updater_set_channel` / `updater_status`, event `update-progress`), giao diện `apps/desktop/src/features/updater/`. Plugin `tauri-plugin-updater` không được cấp quyền cho JavaScript — mọi thao tác đi qua command Rust.
- Setting trong DB: `updater.channel` (`stable`/`beta`), `updater.skippedVersion`, `updater.lastCheck` (giây Unix).
- Windows: bộ cài NSIS chạy ở chế độ `passive` (chỉ hiện thanh tiến độ), tự đóng app và mở lại sau khi cài. macOS/Linux: app thay file rồi tự khởi động lại. Linux `.deb`/`.rpm` có thể hỏi mật khẩu quản trị; AppImage thì không.
- macOS: vì chữ ký ad-hoc đổi sau mỗi bản build, sau khi cập nhật macOS có thể hỏi lại quyền truy cập Keychain — người dùng chọn **Always Allow** (đã ghi trong FAQ của `docs/INSTALL-macOS.md`).

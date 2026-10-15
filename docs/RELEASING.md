# Phát hành bản mới (dành cho maintainer)

Tài liệu này hướng dẫn cách đưa một bản mới lên GitHub Releases để người dùng tải về, và — khi tính năng **Check for Update** (PLAN 3B.4) đã bật — để app của người dùng tự thấy bản mới và cập nhật.

Nguyên tắc: **app chỉ thấy bản đã Publish.** Workflow `release.yml` luôn tạo bản _draft_; người dùng không thấy gì cho tới khi bạn bấm Publish trên GitHub.

---

## A. Thiết lập một lần

### A1. Biến cho build trên CI

Repo → **Settings → Secrets and variables → Actions**:

| Tab       | Tên                              | Giá trị                                          |
| --------- | -------------------------------- | ------------------------------------------------ |
| Variables | `NANOMGG_GITHUB_OAUTH_CLIENT_ID` | Client ID của GitHub OAuth App (bật Device Flow) |
| Variables | `NANOMGG_GITLAB_OAUTH_CLIENT_ID` | Application ID của GitLab (không bắt buộc)       |

Khi build trên máy, app tự đọc hai biến này từ file `.env` ở thư mục gốc (đã git-ignore), không cần đặt gì thêm.

### A2. Khóa ký gói cập nhật (cần cho Check for Update)

Tauri chỉ cài bản cập nhật có chữ ký khớp với public key nằm trong app — kể cả khi ai đó giả mạo file trên mạng.

1. Tạo cặp khóa (chỉ làm **một lần**, giữ kỹ private key — mất là người dùng cũ không cập nhật được nữa):
   ```bash
   pnpm tauri signer generate -w ~/.tauri/nanomgg-updater.key
   ```
   Lệnh hỏi mật khẩu cho khóa, rồi in ra **public key**.
2. Thêm secret trong repo (tab **Secrets**):
   - `TAURI_SIGNING_PRIVATE_KEY` = nội dung file `~/.tauri/nanomgg-updater.key`
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` = mật khẩu vừa đặt
3. Public key được đặt vào `apps/desktop/src-tauri/tauri.conf.json` (`plugins.updater.pubkey`) ở bước 3B.4. Public key không phải bí mật, commit bình thường.
4. Sao lưu private key ở nơi an toàn (password manager).

> Cho tới khi 3B.4 xong, bỏ qua A2: bản phát hành vẫn tải về và cài tay được bình thường.

---

## B. Mỗi lần phát hành

1. **Đánh version** (theo [SemVer](https://semver.org/): sửa lỗi tăng số cuối, thêm tính năng tăng số giữa):
   ```bash
   pnpm release:version 0.2.0        # hoặc 0.3.0-beta.1 cho bản beta
   ```
   Lệnh này sửa `package.json`, `apps/desktop/package.json` (Tauri lấy version từ đây) và version của Cargo workspace.
2. **Ghi CHANGELOG:** trong `CHANGELOG.md`, đổi `## [Unreleased]` thành `## [0.2.0] - 2026-10-20` và thêm lại một mục `## [Unreleased]` trống ở trên. Nội dung này cũng là ghi chú phát hành mà người dùng thấy khi được hỏi cập nhật.
3. **Kiểm tra:** `pnpm check` (format, lint, typecheck, test frontend + Rust).
4. **Commit và tag:**
   ```bash
   git commit -am "chore(release): v0.2.0"
   git tag v0.2.0
   git push origin main v0.2.0
   ```
   > Push file trong `.github/workflows/` cần token có quyền `workflow`. Trong NANOMultipleGitGUI, tài khoản GitHub đăng nhập từ bản 2.9 trở đi đã có sẵn quyền này; tài khoản cũ hãy "Đăng nhập lại" một lần.
5. **Chờ CI:** tab **Actions → Release** build Windows, macOS, Linux (khoảng 15–25 phút) rồi đính kèm file vào bản **draft** `NANOMultipleGitGUI 0.2.0`.
6. **Kiểm tra draft:** tải thử bộ cài, xem `SHA256SUMS.txt`; khi bật updater thì phải có thêm `latest.json` và các file `.sig`.
7. **Publish:** trên trang Releases, mở draft → sửa ghi chú nếu cần → **Publish release**. Với bản beta, tick **Set as a pre-release** (kênh stable sẽ không thấy).

Từ lúc Publish, app của người dùng (bản có updater) sẽ thấy bản mới trong lần kiểm tra kế tiếp (khi khởi động, tối đa một lần/ngày) hoặc ngay khi họ bấm **Kiểm tra cập nhật**.

---

## C. Sự cố thường gặp

| Hiện tượng                                     | Nguyên nhân / cách xử lý                                                                                        |
| ---------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| Push tag bị từ chối "without `workflow` scope" | Token thiếu quyền `workflow` — đăng nhập lại tài khoản trong app (nút có sẵn trên thông báo lỗi)                |
| Workflow Release không chạy                    | Tag phải bắt đầu bằng `v` (`v0.2.0`); hoặc chạy tay: Actions → Release → Run workflow → nhập tag                |
| App người dùng không thấy bản mới              | Release còn là draft hoặc là pre-release; `latest.json` thiếu; version trong tag không lớn hơn version đang cài |
| "signature verification failed" khi cập nhật   | Build bằng private key khác với public key trong app — dùng lại đúng khóa ở A2                                  |
| Muốn build lại một tag đã có                   | Actions → Release → Run workflow, nhập tag; file trong draft sẽ được ghi đè                                     |

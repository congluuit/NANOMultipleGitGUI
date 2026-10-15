# Cài đặt NANOMultipleGitGUI trên macOS

> **Bản nháp.** Ảnh chụp từng bước sẽ được bổ sung vào `docs/images/` khi có bản phát hành đầu tiên.
> English version below.

NANOMultipleGitGUI là phần mềm mã nguồn mở và **chưa được ký bằng chứng chỉ Apple Developer ID** (chương trình này tốn 99 USD/năm). App chỉ được ký _ad-hoc_, nên lần đầu mở macOS sẽ cảnh báo "không xác minh được nhà phát triển". Bạn chỉ cần cho phép **một lần**.

Yêu cầu: macOS 11 (Big Sur) trở lên, máy Apple Silicon (M1/M2/M3/M4…) hoặc Intel. Cùng một file `.dmg` chạy được trên cả hai loại máy.

## 1. Tải và cài

1. Tải file `NANOMultipleGitGUI_<phiên bản>_universal.dmg` ở trang **Releases** của dự án.
2. (Tùy chọn) Kiểm tra toàn vẹn file: mở **Terminal** và chạy
   `shasum -a 256 ~/Downloads/NANOMultipleGitGUI_*_universal.dmg`, so với dòng tương ứng trong `SHA256SUMS.txt`.
3. Mở file `.dmg`, kéo biểu tượng **NANOMultipleGitGUI** vào thư mục **Applications**.

## 2. Mở app lần đầu

### macOS 14 (Sonoma) trở xuống

1. Mở **Finder → Applications**.
2. **Chuột phải** (hoặc Control + click) vào **NANOMultipleGitGUI** → chọn **Open**.
3. Trong hộp thoại cảnh báo, bấm **Open** lần nữa.

### macOS 15 (Sequoia) trở lên

Từ macOS 15, cách chuột phải → Open không còn tác dụng:

1. Mở app một lần như bình thường. macOS sẽ chặn và hiện cảnh báo → bấm **Done** (hoặc **OK**).
2. Vào **System Settings → Privacy & Security**.
3. Kéo xuống phần **Security**, bạn sẽ thấy dòng _"NANOMultipleGitGUI" was blocked…_ → bấm **Open Anyway**.
4. Nhập mật khẩu máy (hoặc Touch ID), rồi bấm **Open** trong hộp thoại tiếp theo.

## 3. Nếu báo "app is damaged and can't be opened"

Lỗi này do cờ _quarantine_ macOS gắn vào file tải từ Internet. Mở **Terminal** và chạy:

```bash
xattr -cr /Applications/NANOMultipleGitGUI.app
```

Sau đó mở lại app.

## 4. Câu hỏi thường gặp

**Cập nhật app thế nào?**
App tự kiểm tra bản mới khi khởi động (tối đa một lần mỗi ngày) và hỏi bạn: **Cập nhật & khởi động lại** / **Để sau** / **Bỏ qua bản này**. Muốn kiểm tra ngay: Settings → **Cập nhật** → **Kiểm tra cập nhật** (hoặc gõ "cập nhật" trong command palette `⌘K`). Dữ liệu và tài khoản được giữ nguyên.

**Có phải làm lại mỗi lần cập nhật không?**
Không. Bản cập nhật tải qua tính năng tự cập nhật của app không bị gắn cờ quarantine. Nếu bạn tự tải `.dmg` mới về cài đè thì có thể phải cho phép lại.

**Sau khi cập nhật, macOS hỏi quyền truy cập Keychain?**
App lưu token đăng nhập GitHub/GitLab trong Keychain. Vì chữ ký ad-hoc thay đổi sau mỗi bản build, macOS có thể hỏi lại. Nhập mật khẩu và chọn **Always Allow**.

**Có an toàn không?**
Toàn bộ mã nguồn công khai và bộ cài được build tự động trên GitHub Actions từ mã nguồn đó. Bạn có thể đối chiếu SHA-256 với `SHA256SUMS.txt`.

**Cần cài Git không?**
Có. Nếu máy chưa có Git, chạy `xcode-select --install`. macOS thường **không có sẵn Git LFS**; app sẽ đề nghị tải giúp, hoặc bạn tự cài bằng `brew install git-lfs`.

---

# Installing NANOMultipleGitGUI on macOS

> **Draft.** Step-by-step screenshots will be added to `docs/images/` with the first release.

NANOMultipleGitGUI is open source and is **not signed with an Apple Developer ID** (that program costs USD 99/year). The app is only _ad-hoc_ signed, so macOS warns that it "cannot verify the developer" the first time you open it. You only need to allow it **once**.

Requirements: macOS 11 (Big Sur) or later, Apple Silicon or Intel. The same `.dmg` works on both.

## 1. Download and install

1. Download `NANOMultipleGitGUI_<version>_universal.dmg` from the project's **Releases** page.
2. (Optional) Verify the file: run `shasum -a 256 ~/Downloads/NANOMultipleGitGUI_*_universal.dmg` in **Terminal** and compare with `SHA256SUMS.txt`.
3. Open the `.dmg` and drag **NANOMultipleGitGUI** into **Applications**.

## 2. First launch

### macOS 14 (Sonoma) and earlier

1. Open **Finder → Applications**.
2. **Right-click** (or Control-click) **NANOMultipleGitGUI** → **Open**.
3. Click **Open** again in the warning dialog.

### macOS 15 (Sequoia) and later

Right-click → Open no longer bypasses the check:

1. Launch the app once. macOS blocks it → click **Done** (or **OK**).
2. Go to **System Settings → Privacy & Security**.
3. Scroll to **Security**; next to _"NANOMultipleGitGUI" was blocked…_ click **Open Anyway**.
4. Authenticate with your password or Touch ID, then click **Open**.

## 3. "App is damaged and can't be opened"

This is caused by the quarantine flag macOS adds to downloads. In **Terminal** run:

```bash
xattr -cr /Applications/NANOMultipleGitGUI.app
```

Then open the app again.

## 4. FAQ

**How do I update?** The app checks for a new version at startup (at most once a day) and asks: **Update & restart** / **Later** / **Skip this version**. To check now: Settings → **Updates** → **Check for updates** (or type "update" in the command palette, `⌘K`). Your data and accounts are kept.

**Do I have to repeat this after every update?** No. Updates installed by the built-in updater are not quarantined. Manually downloaded `.dmg` files may need to be allowed again.

**macOS asks for Keychain access after an update?** Your GitHub/GitLab tokens are stored in the Keychain, and the ad-hoc signature changes with every build. Enter your password and choose **Always Allow**.

**Is it safe?** All source code is public and installers are built by GitHub Actions from that source. Compare the SHA-256 with `SHA256SUMS.txt`.

**Do I need Git?** Yes. Run `xcode-select --install` if Git is missing. macOS usually **does not ship Git LFS**; the app offers to download it, or run `brew install git-lfs`.

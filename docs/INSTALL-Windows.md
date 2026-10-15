# Cài đặt NANOMultipleGitGUI trên Windows

> **Bản nháp.** Ảnh chụp từng bước sẽ được bổ sung vào `docs/images/` khi có bản phát hành đầu tiên.
> English version below.

Yêu cầu: Windows 10 hoặc 11, 64-bit (x64). WebView2 có sẵn trên Windows 10/11; nếu thiếu, bộ cài sẽ tự tải.

## 1. Chọn bộ cài

| File                                           | Dùng khi                                                                            |
| ---------------------------------------------- | ----------------------------------------------------------------------------------- |
| `NANOMultipleGitGUI_<phiên bản>_x64-setup.exe` | **Khuyến nghị.** Cài cho tài khoản Windows hiện tại, không cần quyền Administrator. |
| `NANOMultipleGitGUI_<phiên bản>_x64_en-US.msi` | Triển khai trong doanh nghiệp (Group Policy, Intune…).                              |

(Tùy chọn) Kiểm tra toàn vẹn file trong PowerShell:

```powershell
Get-FileHash .\NANOMultipleGitGUI_*_x64-setup.exe -Algorithm SHA256
```

So kết quả với dòng tương ứng trong `SHA256SUMS.txt`.

## 2. Nếu SmartScreen hiện "Windows protected your PC"

Trong thời gian chờ chương trình ký số miễn phí cho dự án mã nguồn mở (SignPath Foundation) duyệt, bộ cài **chưa được ký số**, nên Microsoft Defender SmartScreen sẽ cảnh báo:

1. Bấm **More info** (Thông tin thêm).
2. Kiểm tra dòng _App_ là `NANOMultipleGitGUI_..._x64-setup.exe`.
3. Bấm **Run anyway** (Vẫn chạy).

Kể cả khi đã được ký, SmartScreen vẫn có thể cảnh báo ở vài bản đầu cho tới khi chứng chỉ tích lũy đủ "uy tín". Khi đó, chứng chỉ sẽ đứng tên **SignPath Foundation** (đặc điểm của chương trình ký số cho dự án mã nguồn mở).

## 3. Cài đặt

Chạy bộ cài và làm theo hướng dẫn. Mặc định app được cài vào
`%LOCALAPPDATA%\NANOMultipleGitGUI` và tạo shortcut trong Start Menu.

## 4. Câu hỏi thường gặp

**Cần cài Git không?** Có — cài [Git for Windows](https://git-scm.com/download/win) (đã kèm Git LFS). Nếu thiếu, app sẽ đề nghị tải bản MinGit portable.

**Dữ liệu và log nằm ở đâu?**

- Cấu hình (SQLite): `%APPDATA%\io.github.nanomgg.nanomultiplegitgui\nanomgg.db`
- Log: `%LOCALAPPDATA%\io.github.nanomgg.nanomultiplegitgui\logs\`
- Token đăng nhập: Windows Credential Manager (không bao giờ lưu dạng văn bản).

**Cập nhật app thế nào?** App tự kiểm tra bản mới khi khởi động (tối đa một lần mỗi ngày) và hỏi bạn: **Cập nhật & khởi động lại** / **Để sau** / **Bỏ qua bản này**. Muốn kiểm tra ngay: Settings → **Cập nhật** → **Kiểm tra cập nhật** (hoặc gõ "cập nhật" trong command palette `Ctrl+K`). Khi cài, một cửa sổ tiến độ nhỏ của bộ cài hiện lên vài giây rồi app tự mở lại; dữ liệu và tài khoản được giữ nguyên. Nếu app báo "Bản build này chưa bật tự cập nhật", hãy tải bộ cài mới trên trang Releases và cài đè.

**Gỡ cài đặt?** Settings → Apps → Installed apps → NANOMultipleGitGUI → Uninstall.

---

# Installing NANOMultipleGitGUI on Windows

> **Draft.** Step-by-step screenshots will be added to `docs/images/` with the first release.

Requirements: Windows 10 or 11, 64-bit (x64). WebView2 ships with Windows 10/11; the installer downloads it if missing.

## 1. Pick an installer

| File                                         | Use it when                                                        |
| -------------------------------------------- | ------------------------------------------------------------------ |
| `NANOMultipleGitGUI_<version>_x64-setup.exe` | **Recommended.** Per-user install, no administrator rights needed. |
| `NANOMultipleGitGUI_<version>_x64_en-US.msi` | Enterprise deployment (Group Policy, Intune…).                     |

(Optional) Verify the download in PowerShell with `Get-FileHash .\NANOMultipleGitGUI_*_x64-setup.exe -Algorithm SHA256` and compare with `SHA256SUMS.txt`.

## 2. "Windows protected your PC" (SmartScreen)

While the project's application to the free SignPath Foundation code-signing program for open source is pending, installers are **unsigned** and SmartScreen warns:

1. Click **More info**.
2. Check that _App_ reads `NANOMultipleGitGUI_..._x64-setup.exe`.
3. Click **Run anyway**.

Even once signed, SmartScreen may still warn for the first few releases until the certificate builds reputation. The certificate publisher will read **SignPath Foundation**, as is standard for that program.

## 3. Install

Run the installer and follow the steps. By default the app goes to `%LOCALAPPDATA%\NANOMultipleGitGUI` with a Start Menu shortcut.

## 4. FAQ

**Do I need Git?** Yes — install [Git for Windows](https://git-scm.com/download/win), which includes Git LFS. If Git is missing the app offers to download portable MinGit.

**Where are data and logs?** Settings DB: `%APPDATA%\io.github.nanomgg.nanomultiplegitgui\nanomgg.db`; logs: `%LOCALAPPDATA%\io.github.nanomgg.nanomultiplegitgui\logs\`; sign-in tokens: Windows Credential Manager (never stored as plain text).

**How do I update?** The app checks for a new version at startup (at most once a day) and asks: **Update & restart** / **Later** / **Skip this version**. To check now: Settings → **Updates** → **Check for updates** (or type "update" in the command palette, `Ctrl+K`). While installing, the installer's small progress window shows for a few seconds and the app reopens; your data and accounts are kept. If the app says "Automatic updates are not enabled in this build", download the new installer from the Releases page and install it over the old one.

**Uninstall?** Settings → Apps → Installed apps → NANOMultipleGitGUI → Uninstall.

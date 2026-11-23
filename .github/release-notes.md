<!-- The release workflow replaces {{VERSION}}, {{TAG}} and {{REPO}}, then appends this version's CHANGELOG section. -->

**NANOMultipleGitGUI** is a free, open-source Git GUI for people who work with several GitHub / GitLab accounts. It picks the right account for every repository, so clone, commit and push always use the account you meant.

## Download

| Your computer                          | Download                                                                                                                                                                                                                                                                                                                                                                                                         | Notes                                                           |
| -------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------- |
| **Windows 10 / 11** (x64, Intel / AMD) | [**NANOMultipleGitGUI_{{VERSION}}_x64_Installer.exe**](https://github.com/{{REPO}}/releases/download/{{TAG}}/NANOMultipleGitGUI_{{VERSION}}_x64_Installer.exe)                                                                                                                                                                                                                                                   | **Recommended.** Step-by-step installer, no admin rights needed |
| Windows (classic installers)           | [`_x64-setup.exe`](https://github.com/{{REPO}}/releases/download/{{TAG}}/NANOMultipleGitGUI_{{VERSION}}_x64-setup.exe) · [`.msi`](https://github.com/{{REPO}}/releases/download/{{TAG}}/NANOMultipleGitGUI_{{VERSION}}_x64_en-US.msi)                                                                                                                                                                            | For silent installs and IT deployment                           |
| **Windows on ARM** (ARM64)             | [**NANOMultipleGitGUI_{{VERSION}}_arm64_Installer.exe**](https://github.com/{{REPO}}/releases/download/{{TAG}}/NANOMultipleGitGUI_{{VERSION}}_arm64_Installer.exe) · [`_arm64-setup.exe`](https://github.com/{{REPO}}/releases/download/{{TAG}}/NANOMultipleGitGUI_{{VERSION}}_arm64-setup.exe) · [`.msi`](https://github.com/{{REPO}}/releases/download/{{TAG}}/NANOMultipleGitGUI_{{VERSION}}_arm64_en-US.msi) | Snapdragon and other ARM64 PCs                                  |
| **macOS 11+** (Apple Silicon & Intel)  | [**NANOMultipleGitGUI_{{VERSION}}_universal.dmg**](https://github.com/{{REPO}}/releases/download/{{TAG}}/NANOMultipleGitGUI_{{VERSION}}_universal.dmg)                                                                                                                                                                                                                                                           | Open it and drag the app into **Applications**                  |
| **Linux** (x64)                        | [`.AppImage`](https://github.com/{{REPO}}/releases/download/{{TAG}}/NANOMultipleGitGUI_{{VERSION}}_amd64.AppImage) · [`.deb`](https://github.com/{{REPO}}/releases/download/{{TAG}}/NANOMultipleGitGUI_{{VERSION}}_amd64.deb) · [`.rpm`](https://github.com/{{REPO}}/releases/download/{{TAG}}/NANOMultipleGitGUI-{{VERSION}}-1.x86_64.rpm)                                                                      | The AppImage runs without installing                            |
| **Linux** (ARM64)                      | [`.AppImage`](https://github.com/{{REPO}}/releases/download/{{TAG}}/NANOMultipleGitGUI_{{VERSION}}_aarch64.AppImage) · [`.deb`](https://github.com/{{REPO}}/releases/download/{{TAG}}/NANOMultipleGitGUI_{{VERSION}}_arm64.deb) · [`.rpm`](https://github.com/{{REPO}}/releases/download/{{TAG}}/NANOMultipleGitGUI-{{VERSION}}-1.aarch64.rpm)                                                                   | Needs glibc 2.35+ (Ubuntu 22.04, Debian 12 or newer)            |

Every file is also listed under **Assets** at the bottom of this page with its SHA-256 checksum (`SHA256SUMS.txt` has them all). Already using the app? It updates itself: **Settings → Updates → Check for updates**.

<details>
<summary><b>First launch: "Windows protected your PC" or "cannot be opened"</b></summary>

- **Windows:** if SmartScreen appears, click **More info → Run anyway** ([guide](https://github.com/{{REPO}}/blob/main/docs/INSTALL-Windows.md)).
- **macOS:** right-click the app → **Open**, or allow it in **System Settings → Privacy & Security** ([guide](https://github.com/{{REPO}}/blob/main/docs/INSTALL-macOS.md)).

</details>

<details>
<summary><b>Hướng dẫn tải và cài bằng Tiếng Việt</b></summary>

- **Windows 10 / 11:** tải [**NANOMultipleGitGUI_{{VERSION}}_x64_Installer.exe**](https://github.com/{{REPO}}/releases/download/{{TAG}}/NANOMultipleGitGUI_{{VERSION}}_x64_Installer.exe), mở lên và làm theo từng bước (không cần quyền quản trị).
- **macOS:** tải file [`.dmg`](https://github.com/{{REPO}}/releases/download/{{TAG}}/NANOMultipleGitGUI_{{VERSION}}_universal.dmg), mở ra rồi kéo app vào **Applications**.
- **Máy Windows chạy chip ARM** (Snapdragon...): tải bản `_arm64_Installer.exe` thay cho bản `_x64`.
- **Linux:** tải `.AppImage`, `.deb` hoặc `.rpm` trong mục **Assets** bên dưới (máy ARM64 chọn file `aarch64` / `arm64`).
- Nếu Windows hiện "Windows protected your PC": bấm **More info → Run anyway** ([hướng dẫn](https://github.com/{{REPO}}/blob/main/docs/INSTALL-Windows.md)). macOS: chuột phải vào app → **Open** ([hướng dẫn](https://github.com/{{REPO}}/blob/main/docs/INSTALL-macOS.md)).
- Đã cài app rồi thì app tự cập nhật: **Settings → Cập nhật → Kiểm tra cập nhật**.

</details>

## What's Changed

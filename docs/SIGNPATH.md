# Ký số bản Windows bằng SignPath Foundation (dành cho maintainer)

Tài liệu này là danh sách việc cần làm **sau khi đơn đăng ký SignPath Foundation được duyệt**. Workflow `release.yml` đã có sẵn job `windows-signed`; bạn chỉ cần cấu hình SignPath, thêm biến vào GitHub rồi bật công tắc `SIGNPATH_ENABLED`.

Kết quả: `nanomgg.exe`, `nanomgg-credential.exe`, bộ cài từng bước `_x64_Installer.exe`, bộ cài NSIS `_x64-setup.exe` và `.msi` đều mang chữ ký "SignPath Foundation". Cảnh báo SmartScreen giảm dần khi chứng chỉ tích lũy uy tín (thường sau vài trăm lượt tải).

## 1. Nhận quyền truy cập

1. Mở email mời của SignPath, đăng nhập [app.signpath.io](https://app.signpath.io) (nên dùng chính tài khoản GitHub của repo).
2. **Bật xác thực hai lớp (MFA)** cho tài khoản SignPath và GitHub. SignPath Foundation yêu cầu điều này với mọi người có quyền ký hoặc push code.
3. Ghi lại **Organization ID**: Settings của organization, hoặc chuỗi GUID trong URL `app.signpath.io/Web/<OrganizationId>/...`.

## 2. Kiểm tra project mà SignPath tạo cho bạn

SignPath Foundation thường tạo sẵn project và các signing policy. Mở **Projects** và kiểm tra:

| Mục                        | Giá trị cần có                                                                                                       |
| -------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| Project slug               | ví dụ `NANOMultipleGitGUI` (ghi lại, dùng ở bước 5)                                                                  |
| Repository URL             | `https://github.com/congluuit/NANOMultipleGitGUI`                                                                    |
| Trusted build system       | **GitHub.com** đã được liên kết. Nếu SignPath yêu cầu, cài **SignPath GitHub App** cho repo để SignPath tải artifact |
| Signing policy (thử)       | thường là `test-signing` (chứng chỉ tự ký, dùng để chạy thử)                                                         |
| Signing policy (phát hành) | thường là `release-signing` (chứng chỉ SignPath Foundation). Bạn là **Approver**: mỗi yêu cầu ký cần bạn bấm duyệt   |

## 3. Tạo hai artifact configuration

Trong project → **Artifact Configurations** → **Add**. Tên slug phải đúng như dưới đây vì workflow gọi đúng tên đó.

**`app-exe`**: vòng 1, ký hai file chạy của app (GitHub đóng cả hai vào một file zip):

```xml
<?xml version="1.0" encoding="utf-8"?>
<artifact-configuration xmlns="http://signpath.io/artifact-configuration/v1">
  <zip-file>
    <pe-file path="nanomgg.exe">
      <authenticode-sign />
    </pe-file>
    <pe-file path="nanomgg-credential.exe">
      <authenticode-sign />
    </pe-file>
  </zip-file>
</artifact-configuration>
```

**`installers`**: vòng 2, ký ba bộ cài (giữ nguyên thư mục `nsis/`, `msi/`, `installer/` như trong zip):

```xml
<?xml version="1.0" encoding="utf-8"?>
<artifact-configuration xmlns="http://signpath.io/artifact-configuration/v1">
  <zip-file>
    <pe-file path="nsis/*_x64-setup.exe">
      <authenticode-sign />
    </pe-file>
    <msi-file path="msi/*.msi">
      <authenticode-sign />
    </msi-file>
    <pe-file path="installer/*_x64_Installer.exe">
      <authenticode-sign />
    </pe-file>
  </zip-file>
</artifact-configuration>
```

Chỉ ký file do dự án build ra. Không thêm DLL plugin của NSIS hay file của bên thứ ba (đây là điều kiện của SignPath Foundation).

## 4. Tạo API token cho GitHub Actions

1. Organization → **Users** → **Add CI user** (ví dụ tên `github-actions`).
2. Cho user này quyền **Submitter** trên cả hai signing policy (`test-signing`, `release-signing`).
3. Tạo **API token** cho user đó và chép lại ngay (chỉ hiện một lần).

## 5. Thêm biến vào GitHub

Repo → **Settings → Secrets and variables → Actions**:

| Tab       | Tên                            | Giá trị                                                             |
| --------- | ------------------------------ | ------------------------------------------------------------------- |
| Secrets   | `SIGNPATH_API_TOKEN`           | API token ở bước 4                                                  |
| Variables | `SIGNPATH_ORGANIZATION_ID`     | Organization ID ở bước 1                                            |
| Variables | `SIGNPATH_PROJECT_SLUG`        | Project slug ở bước 2                                               |
| Variables | `SIGNPATH_SIGNING_POLICY_SLUG` | `test-signing` cho lần chạy thử, sau đó đổi thành `release-signing` |
| Variables | `SIGNPATH_ENABLED`             | `true`. Đặt thành `false` (hoặc xóa) để quay về bản không ký        |

## 6. Chạy thử với chứng chỉ test

1. Đặt `SIGNPATH_SIGNING_POLICY_SLUG=test-signing`.
2. Tạo một tag beta, ví dụ `v1.0.1-beta.1` (hoặc **Actions → Release → Run workflow** với một tag có sẵn).
3. Job **Build (Windows x64, SignPath)** dừng hai lần chờ SignPath. Mở SignPath → **Signing Requests**, kiểm tra và duyệt (policy test có thể tự duyệt).
4. Tải bộ cài từ bản draft, kiểm tra chữ ký trong PowerShell:
   ```powershell
   Get-AuthenticodeSignature .\NANOMultipleGitGUI_1.0.1-beta.1_x64_Installer.exe | Format-List
   ```
   Với chứng chỉ test, `Status` báo không tin cậy là bình thường; quan trọng là file **có** chữ ký và job chạy xanh.
5. Xóa bản draft và tag thử.

## 7. Bật ký thật

1. Đổi `SIGNPATH_SIGNING_POLICY_SLUG=release-signing`.
2. Phát hành như bình thường ([RELEASING.md](RELEASING.md), mục B). Mỗi lần phát hành, SignPath gửi email: vào **Signing Requests** bấm **Approve** cho cả hai vòng (app rồi đến bộ cài). Job chờ tối đa vài giờ.
3. Kiểm tra: chuột phải bộ cài → **Properties → Digital Signatures** phải thấy chữ ký SignPath Foundation; `Get-AuthenticodeSignature` báo `Valid`.

## 8. Điều kiện của SignPath Foundation cần giữ

- README có mục **Code signing policy** với câu: _"Free code signing provided by [SignPath.io](https://signpath.io), certificate by [SignPath Foundation](https://signpath.org)"_, kèm danh sách vai trò (ai commit, ai review, ai duyệt ký: hiện tại cả ba là Cong Luu) và link [PRIVACY.md](../PRIVACY.md).
- Chỉ ký bản build từ GitHub Actions của chính repo này (workflow đã làm vậy). Không ký file build trên máy cá nhân.
- Mọi thay đổi vào `main` đi qua người có quyền; giữ MFA bật.
- Nếu chứng chỉ hoặc token bị lộ: thu hồi token trong SignPath ngay và báo cho SignPath Foundation.

## 9. Sau lần ký thật đầu tiên

- Sửa phần "First launch" trong `.github/release-notes.md` và `docs/INSTALL-Windows.md`: SmartScreen vẫn có thể hiện trong vài tuần đầu cho tới khi đủ uy tín.
- Cập nhật mục "Ký số" trong PLAN.md.

## Sự cố thường gặp

| Hiện tượng                                           | Cách xử lý                                                                                                            |
| ---------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| `Artifact configuration 'app-exe' not found`         | Slug ở bước 3 sai tên hoặc tạo nhầm project                                                                           |
| SignPath báo file không khớp `path`                  | Kiểm tra lại thư mục trong zip (tải artifact `unsigned-installers` ở trang Actions xem cấu trúc)                      |
| `origin verification failed`                         | Repository URL hoặc trusted build system của project chưa đúng (bước 2); tag phải chạy trên repo gốc, không phải fork |
| Job chờ mãi                                          | Yêu cầu ký chưa được Approve trong SignPath                                                                           |
| `latest.json` thiếu mục Windows sau khi bật SignPath | Xem job `windows-updater-json` ([RELEASING.md](RELEASING.md), mục C)                                                  |

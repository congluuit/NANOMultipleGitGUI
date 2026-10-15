# NANOMultipleGitGUI — Project Knowledge Base

Tài liệu dành cho người dùng muốn hiểu cách ứng dụng hoạt động và contributor cần tìm đúng nơi để sửa đổi. Nội dung mô tả kiến trúc, quy ước và hành vi của dự án; mã nguồn hiện tại là nguồn tham chiếu cuối cùng.

Tên hiển thị: **NANO Multiple Git GUI**. Tên repository: **NANOMultipleGitGUI**. Mã ngắn trong crate, binary và biến môi trường: **`nanomgg`**.

> Tài liệu này được giữ tại `PLAN.md` để các liên kết sẵn có tiếp tục hoạt động. Hướng dẫn sử dụng tổng quan nằm ở [README](README.md); thay đổi theo phiên bản nằm ở [CHANGELOG](CHANGELOG.md).

## Tra cứu nhanh

- [1. Mục tiêu và nguyên tắc](#1-mục-tiêu-và-nguyên-tắc)
- [2. Nền tảng kỹ thuật](#2-nền-tảng-kỹ-thuật)
- [3. Kiến trúc và bản đồ mã nguồn](#3-kiến-trúc-và-bản-đồ-mã-nguồn)
- [4. Mô hình nhiều tài khoản](#4-mô-hình-nhiều-tài-khoản)
- [5. Luồng thao tác Git](#5-luồng-thao-tác-git)
- [6. Git LFS và file lớn](#6-git-lfs-và-file-lớn)
- [7. Giao diện và khả năng mở rộng](#7-giao-diện-và-khả-năng-mở-rộng)
- [8. Build, đóng gói và cập nhật](#8-build-đóng-gói-và-cập-nhật)
- [9. Dữ liệu, bảo mật và quyền riêng tư](#9-dữ-liệu-bảo-mật-và-quyền-riêng-tư)
- [10. Chẩn đoán sự cố](#10-chẩn-đoán-sự-cố)
- [11. Kiểm thử và xác minh](#11-kiểm-thử-và-xác-minh)
- [12. Đóng góp và quyết định kiến trúc](#12-đóng-góp-và-quyết-định-kiến-trúc)

## 1. Mục tiêu và nguyên tắc

### 1.1 Phạm vi sản phẩm

NANOMultipleGitGUI là ứng dụng Git desktop mã nguồn mở theo Apache-2.0, hướng đến người dùng làm việc với nhiều tài khoản cá nhân, công ty và nhóm. Tài khoản GitHub và GitLab có thể tồn tại đồng thời, kể cả nhiều tài khoản trên cùng host.

Ứng dụng kết hợp quản lý tài khoản với workflow Git, Git LFS và tích hợp provider. Phạm vi provider hiện tại gồm GitHub, GitHub Enterprise, GitLab.com và GitLab self-hosted. Các nền tảng khác chưa có tích hợp provider tương đương.

### 1.2 Các khái niệm chính

| Khái niệm          | Ý nghĩa                                                                    |
| ------------------ | -------------------------------------------------------------------------- |
| Provider           | Một instance GitHub/GitLab với địa chỉ web, API và cấu hình đăng nhập      |
| Account            | Tài khoản người dùng thuộc một provider                                    |
| Identity           | Tên/email và cấu hình ký commit của tài khoản                              |
| Repository binding | Liên kết repository hoặc một remote cụ thể với account/identity            |
| Rule               | Điều kiện chọn tài khoản theo host, owner/group, đường dẫn hoặc URL remote |
| Account context    | Kết quả chọn tài khoản và identity dùng cho một thao tác                   |

### 1.3 Nguyên tắc thiết kế

- Hiển thị rõ tài khoản đang dùng trước các thao tác quan trọng.
- Chọn context theo repository và remote; không thay đổi cấu hình Git global để chuyển tài khoản giữa các thao tác.
- Tách metadata khỏi bí mật: SQLite giữ dữ liệu ứng dụng, keychain hệ điều hành giữ credential.
- Chạy Git qua CLI để sử dụng cơ chế Git, hook, LFS và signing của môi trường cục bộ.
- Cảnh báo sớm về identity, quyền truy cập và file lớn; thông báo lỗi phải giúp người dùng xác định bước xử lý.

## 2. Nền tảng kỹ thuật

| Lớp             | Công nghệ                             | Vai trò                                                   |
| --------------- | ------------------------------------- | --------------------------------------------------------- |
| Desktop         | Tauri 2                               | Cửa sổ, IPC, tích hợp hệ điều hành, đóng gói              |
| Backend         | Rust, Cargo workspace                 | Điều phối Git, tài khoản, credential, dữ liệu và provider |
| Frontend        | React 19, TypeScript, Vite            | Giao diện và workflow                                     |
| UI              | Tailwind CSS, Radix UI                | Thành phần dùng chung và design tokens                    |
| State           | Zustand, TanStack Query               | State giao diện và dữ liệu bất đồng bộ                    |
| IPC             | Tauri commands, tauri-specta          | Sinh binding TypeScript từ backend                        |
| Git             | System Git CLI, Git LFS CLI           | Repository và truyền dữ liệu                              |
| Dữ liệu         | SQLite, OS keychain                   | Metadata và bí mật                                        |
| Diff / terminal | Monaco Editor, xterm.js, portable-pty | So sánh file và terminal tích hợp                         |
| Ngôn ngữ        | i18next                               | English và Tiếng Việt                                     |

Git CLI hiện phục vụ cả thao tác đọc và ghi. Không nên hiểu ADR về khả năng bổ sung `gix` như một dependency đã được dùng cho mọi luồng đọc. Xem [ADR 0002](docs/adr/0002-git-cli-and-gix.md).

## 3. Kiến trúc và bản đồ mã nguồn

```text
React UI
  → generated TypeScript bindings
  → Tauri commands + AppState
      → Git engine → Git / Git LFS subprocesses
      → Accounts + Auth + Providers
      → Storage + OS keychain
      → Credential server ← nanomgg-credential ← Git / Git LFS
```

Frontend gọi command có kiểu dữ liệu thay vì tự ghép tên IPC. Tauri command kết nối state ứng dụng với các crate chuyên trách; logic có thể tái sử dụng nên nằm trong crate phù hợp.

| Khu vực                                                                   | Trách nhiệm                                     |
| ------------------------------------------------------------------------- | ----------------------------------------------- |
| [apps/desktop/src](apps/desktop/src/)                                     | React UI, feature modules, component dùng chung |
| [apps/desktop/src/ipc](apps/desktop/src/ipc/)                             | Binding và giao diện gọi backend                |
| [apps/desktop/src-tauri/src](apps/desktop/src-tauri/src/)                 | Command, AppState, điều phối workflow desktop   |
| [crates/git-engine](crates/git-engine/)                                   | Thực thi Git, đọc lịch sử, thao tác repository  |
| [crates/accounts](crates/accounts/)                                       | Model, rule engine, phân giải account context   |
| [crates/auth](crates/auth/)                                               | OAuth và vòng đời credential                    |
| [crates/providers](crates/providers/)                                     | API GitHub/GitLab, PR/MR, issue và CI           |
| [crates/credentials](crates/credentials/)                                 | Credential server cục bộ                        |
| [crates/credential-helper](crates/credential-helper/)                     | Sidecar `nanomgg-credential` phục vụ Git        |
| [crates/ssh-keys](crates/ssh-keys/)                                       | Quản lý SSH key                                 |
| [crates/lfs](crates/lfs/)                                                 | Kiểm tra file lớn và workflow LFS               |
| [crates/storage](crates/storage/)                                         | SQLite và migration                             |
| [crates/backup](crates/backup/)                                           | Xuất/nhập tài khoản, mã hóa, backup/restore     |
| [crates/app-paths](crates/app-paths/) / [crates/logging](crates/logging/) | Vị trí dữ liệu và logging có redact             |
| [crates/import](crates/import/)                                           | Nhập cấu hình Git/SSH và công cụ có sẵn         |
| [tools/test-git-server](tools/test-git-server/)                           | Git/LFS server cục bộ cho kiểm thử              |
| [scripts](scripts/) / [.github/workflows](.github/workflows/)             | Công cụ phát triển, CI và release               |

## 4. Mô hình nhiều tài khoản

### 4.1 Metadata và bí mật

Provider, account, identity, binding và rule được mô hình hóa trong [accounts/model.rs](crates/accounts/src/model.rs). Account chỉ ghi nhận metadata về credential; token và passphrase không phải trường dữ liệu plaintext trong model này.

Một account có thể có nhiều identity. Khi có identity được chỉ định hợp lệ, resolver dùng identity đó; nếu không, chọn identity mặc định của account, rồi identity cũ nhất làm phương án dự phòng.

### 4.2 Đăng nhập

GitHub hỗ trợ device flow; GitLab có các luồng OAuth phù hợp với cấu hình instance. Browser sign-in cần OAuth client ID được cấu hình cho provider hoặc nhúng trong build. PAT là cách đăng nhập khi chưa cấu hình OAuth.

Build configuration công khai và tên biến nằm trong [.env.example](.env.example). Không đưa token, client secret hoặc private key vào source control.

### 4.3 Thứ tự chọn tài khoản

Với mỗi remote, resolver xét theo thứ tự:

1. Binding đã khóa của remote, sau đó binding đã khóa cấp repository.
2. Binding thủ công hoặc được tạo khi clone của remote, sau đó cấp repository.
3. Rule engine theo độ ưu tiên.
4. Binding tự động đã ghi nhớ, ưu tiên remote trước repository.
5. Không có kết quả: repository chưa gắn tài khoản; UI thông báo và Git dùng cấu hình riêng của nó.

Các rule đang bật được xét theo `priority` tăng dần, rồi thời điểm tạo và ID. Mọi điều kiện đã khai báo trong một rule phải cùng khớp; rule hợp lệ đầu tiên thắng.

Binding cấp repository và rule không chỉ định host được kiểm tra tương thích host trước khi dùng cho remote. Nhờ vậy, `upstream` trên host khác không tự nhận tài khoản của `origin` chỉ vì repository đã có mặc định.

Context mặc định cho commit hoặc lệnh không gắn với remote ưu tiên binding cấp repository đã khóa, binding thủ công/clone, rồi context của remote chính (`origin` hoặc remote đầu tiên), rule theo thư mục và binding tự động đã nhớ.

Nguồn tham chiếu: [resolve.rs](crates/accounts/src/resolve.rs) và [rules.rs](crates/accounts/src/rules.rs).

### 4.4 Credential HTTPS và SSH

Với HTTPS, ứng dụng inject cấu hình theo tiến trình: reset helper kế thừa, dùng `nanomgg-credential`, bật `credential.useHttpPath` và truyền tham chiếu context tới credential server cục bộ. Sidecar trao đổi với server qua named pipe trên Windows hoặc Unix-domain socket trên macOS/Linux.

Credential server kiểm tra session, account và host được yêu cầu trước khi trả credential. Token không cần nằm trong remote URL hoặc command line. Git LFS kế thừa môi trường để sử dụng cùng context. Endpoint LFS khác host cần được xử lý theo chính sách credential riêng, không mặc nhiên nhận token của Git provider.

Với SSH, ứng dụng quản lý key theo account và cấu hình lệnh Git theo context. Identity có thể được ghi vào `.git/config` theo tùy chọn; cần phân biệt cấu hình cục bộ của repository với cấu hình Git global.

Xem [ADR 0003](docs/adr/0003-credential-helper-sidecar.md) để hiểu thiết kế ban đầu và tradeoff. Chi tiết triển khai hiện tại nằm trong source tương ứng.

### 4.5 Guard và giới hạn

Commit Guard và Push Guard giúp phát hiện identity không phù hợp, email chưa xác minh, quyền push hoặc file lớn. Đây là kiểm tra hỗ trợ workflow; provider vẫn quyết định quyền thực tế, branch protection và giới hạn tài nguyên khi nhận yêu cầu.

## 5. Luồng thao tác Git

Một thao tác thường đi qua: chọn repository/remote → phân giải context → kiểm tra điều kiện và guard → chạy Git → nhận progress/kết quả → làm mới dữ liệu UI.

Các nhóm chức năng gồm:

- **Working tree:** status, diff, stage/unstage file, hunk và dòng, commit/amend, discard.
- **History:** commit graph, tìm kiếm, file history, blame, nội dung file tại commit.
- **Branch và lịch sử:** checkout, merge, rebase, interactive rebase, cherry-pick, revert, reset, tag.
- **Workspace:** stash, submodule, worktree, terminal, thao tác chuyển nhánh với cất/khôi phục thay đổi.
- **Remote:** clone, fetch, pull, push, progress, hủy và fetch nền.
- **Provider:** PR/MR, issue, fork, CI và My Work.

Các thao tác như reset, rebase và discard có thể thay đổi hoặc loại bỏ dữ liệu. Tài liệu và UI cần nói rõ phạm vi tác động; Undo/Redo không thay thế bản sao lưu repository.

## 6. Git LFS và file lớn

Large File Guard kiểm tra file trước commit và blob trong lịch sử chưa push. Ngưỡng mặc định của ứng dụng theo provider được định nghĩa trong [accounts/model.rs](crates/accounts/src/model.rs): cấu hình GitHub cảnh báo ở 50 MB và đặt ngưỡng chặn 100 MB; GitLab mặc định cảnh báo 50 MB, không áp dụng ngưỡng chặn chung vì mỗi instance có chính sách riêng. Đây là cấu hình ứng dụng, không phải cam kết về giới hạn server.

Workflow LFS tách ước lượng khỏi truyền dữ liệu để người dùng biết phạm vi trước khi upload. Giao diện có progress và hủy; khi thử lại, object đã có trên server có thể được bỏ qua. Không nên hiểu khả năng tiếp tục là mọi server đều hỗ trợ resume từng byte.

LFS cần `git-lfs` và quyền truy cập endpoint tương ứng. Track file bằng LFS không tự chuyển toàn bộ lịch sử cũ: blob lớn trong commit đã tạo vẫn cần workflow xử lý lịch sử phù hợp. Xem [ADR 0004](docs/adr/0004-lfs-two-step-transfer.md).

## 7. Giao diện và khả năng mở rộng

Feature UI nằm trong `apps/desktop/src/features/`; thành phần dùng chung nằm trong `components/`. Theme dùng design tokens thay vì màu hard-code. Ứng dụng có Light, Dark, System, preset tương phản cao và theme tùy chỉnh.

Chuỗi giao diện dùng i18next và phải có cả bản English lẫn Tiếng Việt trong [i18n/locales](apps/desktop/src/i18n/locales/). Binding IPC được sinh từ backend; không sửa thủ công file được sinh để che lỗi không khớp kiểu.

AI commit messages là tính năng tùy chọn, mặc định tắt. Khi người dùng yêu cầu, staged diff, tên nhánh và tiêu đề commit gần đây được gửi tới endpoint đã chọn. Ollama có thể phục vụ mô hình trên máy; quyền riêng tư phụ thuộc cả địa chỉ endpoint được cấu hình.

## 8. Build, đóng gói và cập nhật

Yêu cầu phát triển và lệnh khởi chạy nằm tại [README — Development](README.md#development). Version và script nằm trong [package.json](package.json); Rust MSRV nằm trong [Cargo.toml](Cargo.toml).

```bash
pnpm install
pnpm dev          # Desktop + backend
pnpm dev:web      # Chỉ frontend
pnpm build        # Bundle theo OS hiện tại
```

Release workflow xây dựng Windows, macOS universal và Linux rồi tạo draft trên GitHub Releases. Định dạng và artifact thực tế phụ thuộc cấu hình workflow và từng release. Windows signing qua SignPath là cấu hình tùy chọn; macOS hiện dùng ad-hoc signing. Không nên suy ra một bản tải xuống đã được ký chỉ từ tên định dạng.

Bộ cài Windows chính là `apps/installer` (`nanomgg-setup`): một app Tauri nhỏ với giao diện từng bước (chào mừng → giấy phép → thư mục → cài → hoàn tất), nhúng bộ cài NSIS (`NANOMGG_SETUP_PAYLOAD` lúc build) và chạy nó ở chế độ im lặng `/S /D=<thư mục>`. Vì vậy file, registry, shortcut, uninstaller và đường cập nhật trong app giống hệt bộ cài NSIS; updater vẫn dùng gói NSIS. Máy thiếu WebView2 thì bộ cài mở thẳng wizard NSIS cổ điển (wizard này tự cài WebView2). Release đăng file `NANOMultipleGitGUI_<version>_x64_Installer.exe`.

Tự cập nhật cần repository đích và public key trong build, cùng artifact được ký bằng private key tương ứng. Chữ ký gói cập nhật độc lập với chữ ký bộ cài của hệ điều hành. Build thiếu cấu hình updater vẫn có thể dùng trang Releases để tải thủ công.

Hướng dẫn đầy đủ: [RELEASING.md](docs/RELEASING.md); ký số Windows: [SIGNPATH.md](docs/SIGNPATH.md). Hướng dẫn cài đặt: [Windows](docs/INSTALL-Windows.md) và [macOS](docs/INSTALL-macOS.md).

## 9. Dữ liệu, bảo mật và quyền riêng tư

| Dữ liệu                                                              | Nơi lưu / cách xử lý                                               |
| -------------------------------------------------------------------- | ------------------------------------------------------------------ |
| Settings, metadata tài khoản, repository, binding/rule, theme, notes | SQLite trong thư mục dữ liệu ứng dụng                              |
| Token, refresh token, SSH passphrase, AI API key                     | Keychain hệ điều hành                                              |
| SSH private key do app tạo                                           | Thư mục app với quyền truy cập hạn chế hoặc keychain, tùy cách lưu |
| Log                                                                  | Thư mục log, có rotation và lọc bí mật                             |
| Account export và full app backup                                    | File mã hóa tại vị trí người dùng chọn                             |

Ứng dụng không vận hành server riêng và hiện không có telemetry. Kết nối mạng phục vụ Git remotes, provider API, avatar, kiểm tra cập nhật, tải công cụ theo yêu cầu và AI tùy chọn. [PRIVACY.md](PRIVACY.md) mô tả chính sách chi tiết; [SECURITY.md](SECURITY.md) quy định kênh báo cáo lỗ hổng.

File export/backup dùng Argon2id để dẫn xuất khóa và XChaCha20-Poly1305 để mã hóa có xác thực. Người dùng chọn có đưa bí mật vào hay không. Full restore được chuẩn bị và áp dụng khi ứng dụng khởi động lại, trước khi mở database; xem [crates/backup](crates/backup/) và [desktop backup.rs](apps/desktop/src-tauri/src/backup.rs).

**Full app backup sao lưu dữ liệu của ứng dụng, không sao lưu nội dung các Git repository.** Người dùng vẫn cần phương án riêng cho working tree, file chưa commit và repository cục bộ.

## 10. Chẩn đoán sự cố

| Hiện tượng                                | Điểm cần kiểm tra                                                                                                |
| ----------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| Browser sign-in không khả dụng            | OAuth client ID của provider/build; có thể dùng PAT                                                              |
| Push trả 401/403                          | Account context, thời hạn/scope token, quyền repository, chính sách tổ chức                                      |
| Commit dùng sai email                     | Identity đang chọn, binding, thứ tự rule và `.git/config` cục bộ                                                 |
| Git chạy ngoài app dùng tài khoản khác    | Context theo tiến trình chỉ áp dụng cho tiến trình được app cấu hình; kiểm tra Git/SSH config của terminal ngoài |
| Git push được nhưng LFS lỗi               | `git-lfs`, endpoint LFS, credential host, quota và quyền LFS                                                     |
| File lớn vẫn bị từ chối sau khi track LFS | Blob có thể vẫn nằm trong commit cũ chưa được chuyển đổi                                                         |
| Không thấy bản cập nhật                   | Cấu hình updater của build, kênh stable/beta và release đã publish                                               |
| Bị cảnh báo khi mở bộ cài                 | Signing của artifact; xem hướng dẫn cài tương ứng                                                                |
| Frontend chạy nhưng thao tác desktop lỗi  | `pnpm dev:web` không cung cấp đầy đủ Tauri backend                                                               |

Khi mở issue, cung cấp OS, phiên bản app/Git/Git LFS, bước tái hiện và thông báo lỗi đã loại bỏ thông tin nhạy cảm. Không đăng token, private key hoặc log chứa dữ liệu riêng tư.

## 11. Kiểm thử và xác minh

```bash
pnpm check          # Format, lint, types, frontend tests, Rust fmt/Clippy/tests
pnpm test           # Frontend tests
pnpm rust:test      # Cargo workspace tests, gồm kiểm tra/sinh IPC bindings
```

CI có job frontend và Rust trên Linux, Windows, macOS; kiểm tra binding đã sinh không bị lệch với source. Cấu hình cụ thể nằm trong [ci.yml](.github/workflows/ci.yml).

Integration test đa tài khoản dùng [test-git-server](tools/test-git-server/) cục bộ để kiểm chứng credential Git/LFS. Những test này không thay thế kiểm tra quyền, OAuth, quota và chính sách trên provider thật.

### Checklist xác minh bộ cài

- Cài và mở ứng dụng; kiểm tra Git/Git LFS và keychain trên OS đích.
- Thêm hai tài khoản trên cùng host; clone/commit/push vào repository thử nghiệm với identity khác nhau.
- Kiểm tra HTTPS, SSH và LFS trong phạm vi bản phát hành hỗ trợ.
- Thử progress, hủy thao tác và kiểm tra trạng thái repository sau lỗi/hủy.
- Kiểm tra giao diện sáng/tối, English/Tiếng Việt và kích thước cửa sổ nhỏ.
- Thử export/backup/restore với dữ liệu dùng riêng cho kiểm thử.
- Xác minh artifact, checksum, signing và updater theo cấu hình release.

Chạy thao tác ghi hoặc viết lại lịch sử trên repository thử nghiệm có thể phục hồi. Ghi kết quả kiểm thử của từng release trong PR/release notes phù hợp; Knowledge Base giữ hướng dẫn có thể tái sử dụng.

## 12. Đóng góp và quyết định kiến trúc

Đọc [CONTRIBUTING.md](CONTRIBUTING.md) trước khi gửi PR. Issue và PR có thể dùng English hoặc Tiếng Việt. Báo cáo lỗi và đề xuất tính năng dùng [GitHub Issues](https://github.com/congluuit/NANOMultipleGitGUI/issues); lỗ hổng bảo mật đi theo kênh riêng trong SECURITY.md.

Các quyết định kiến trúc nằm ở [docs/adr](docs/adr/): desktop shell, Git CLI, credential helper, LFS transfer và HTTP client provider. ADR giải thích bối cảnh tại thời điểm quyết định; hành vi hiện tại cần được đối chiếu với source. Quyết định thay thế một ADR đã chấp nhận được ghi bằng ADR mới.

Khi sửa behavior, cập nhật tài liệu tương ứng: README cho trải nghiệm tổng quan, Knowledge Base cho cơ chế và quy ước, ADR cho quyết định kiến trúc, CHANGELOG cho thay đổi người dùng thấy được. Những kế hoạch chưa triển khai nên được thảo luận trong issue hoặc proposal, với trạng thái rõ ràng.

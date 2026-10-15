export type Language = "en" | "vi";

const en = {
  "window.title": "NANOMultipleGitGUI Setup",
  "window.minimize": "Minimize",
  "window.close": "Close",
  "sidebar.tagline": "Multi-account Git GUI",
  "sidebar.version": "Version {version}",
  "steps.label": "Setup steps",
  "step.welcome": "Welcome",
  "step.license": "License",
  "step.location": "Location",
  "step.install": "Install",
  "step.done": "Done",
  "welcome.title": "Welcome to NANOMultipleGitGUI",
  "welcome.tagline": "The Git GUI for people with more than one GitHub or GitLab account.",
  "welcome.feature1": "Every repository remembers which account it belongs to.",
  "welcome.feature2": "Clone, commit and push with the right name, email and token, automatically.",
  "welcome.feature3": "Free and open source, under the Apache License 2.0.",
  "welcome.perUser":
    "This installs version {version} for your Windows user. No administrator rights needed.",
  "welcome.language": "Language",
  "welcome.update":
    "Version {installed} is installed on this computer. Continuing updates it to {version}. Your accounts, settings and repositories are kept.",
  "welcome.repair":
    "Version {version} is already installed. Continuing reinstalls it, which repairs missing or damaged files. Your accounts, settings and repositories are kept.",
  "welcome.newer":
    "A newer version ({installed}) is already installed. Download the latest setup from the Releases page instead.",
  "welcome.noPayload":
    "Development build: the app package is not embedded, so the Install step will fail. Set NANOMGG_SETUP_PAYLOAD when building.",
  "license.title": "License agreement",
  "license.intro":
    "NANOMultipleGitGUI is free software. Please read the license before you continue.",
  "license.accept": "I accept the terms of the license",
  "location.title": "Choose where to install",
  "location.intro":
    "The app is installed for your user only. The default folder works for most people.",
  "location.folder": "Install folder",
  "location.browse": "Browse…",
  "location.reset": "Use the default folder",
  "location.desktop": "Create a desktop shortcut",
  "location.startMenu":
    "A Start menu shortcut is always created. Uninstall any time from Settings → Apps.",
  "location.running":
    "NANOMultipleGitGUI is open. It will be closed while installing, so save your work first.",
  "location.summary": "Ready to install NANOMultipleGitGUI {version}.",
  "error.notAbsolute": "Enter a full folder path, for example {example}.",
  "error.notWritable":
    "This folder needs administrator rights. Choose a folder inside your user profile, such as the default one.",
  "error.notEmpty":
    "This folder already contains other files. Choose an empty folder or a new one.",
  "install.title": "Installing NANOMultipleGitGUI",
  "install.extracting": "Preparing the files…",
  "install.installing": "Copying the app to your computer…",
  "install.finishing": "Creating shortcuts…",
  "install.hint": "This usually takes less than a minute.",
  "done.title": "All set!",
  "done.body": "NANOMultipleGitGUI {version} is installed in {dir}.",
  "done.next":
    "When the app opens, a short Getting started guide helps you check Git and add your GitHub / GitLab accounts.",
  "done.launch": "Open NANOMultipleGitGUI now",
  "failed.title": "The installation did not finish",
  "failed.body":
    "Nothing important was changed. You can try again, or use the classic installer from the Releases page.",
  "failed.details": "Details: {code}",
  "failed.exit2": "Close NANOMultipleGitGUI (check the system tray) and try again.",
  "button.back": "Back",
  "button.next": "Next",
  "button.install": "Install",
  "button.cancel": "Cancel",
  "button.finish": "Finish",
  "button.retry": "Try again",
  "button.close": "Close",
} as const;

export type MessageKey = keyof typeof en;

const vi: Record<MessageKey, string> = {
  "window.title": "Cài đặt NANOMultipleGitGUI",
  "window.minimize": "Thu nhỏ",
  "window.close": "Đóng",
  "sidebar.tagline": "Git GUI nhiều tài khoản",
  "sidebar.version": "Phiên bản {version}",
  "steps.label": "Các bước cài đặt",
  "step.welcome": "Chào mừng",
  "step.license": "Giấy phép",
  "step.location": "Thư mục",
  "step.install": "Cài đặt",
  "step.done": "Hoàn tất",
  "welcome.title": "Chào mừng đến với NANOMultipleGitGUI",
  "welcome.tagline": "Git GUI dành cho người dùng nhiều tài khoản GitHub hoặc GitLab.",
  "welcome.feature1": "Mỗi repository tự nhớ nó thuộc tài khoản nào.",
  "welcome.feature2": "Clone, commit và push đúng tên, email và token — hoàn toàn tự động.",
  "welcome.feature3": "Miễn phí và mã nguồn mở, giấy phép Apache 2.0.",
  "welcome.perUser":
    "Bộ cài này cài phiên bản {version} cho tài khoản Windows của bạn. Không cần quyền quản trị.",
  "welcome.language": "Ngôn ngữ",
  "welcome.update":
    "Máy đang có phiên bản {installed}. Tiếp tục sẽ cập nhật lên {version}; tài khoản, cài đặt và repository của bạn được giữ nguyên.",
  "welcome.repair":
    "Máy đã có phiên bản {version}. Tiếp tục sẽ cài lại để sửa file bị thiếu hoặc hỏng; tài khoản, cài đặt và repository của bạn được giữ nguyên.",
  "welcome.newer":
    "Máy đã có phiên bản mới hơn ({installed}). Hãy tải bộ cài mới nhất ở trang Releases.",
  "welcome.noPayload":
    "Bản build thử: chưa nhúng gói cài app nên bước Cài đặt sẽ lỗi. Đặt NANOMGG_SETUP_PAYLOAD khi build.",
  "license.title": "Thỏa thuận giấy phép",
  "license.intro":
    "NANOMultipleGitGUI là phần mềm tự do. Vui lòng đọc giấy phép trước khi tiếp tục.",
  "license.accept": "Tôi đồng ý với các điều khoản của giấy phép",
  "location.title": "Chọn nơi cài đặt",
  "location.intro":
    "App chỉ cài cho tài khoản của bạn. Thư mục mặc định phù hợp với hầu hết mọi người.",
  "location.folder": "Thư mục cài đặt",
  "location.browse": "Chọn…",
  "location.reset": "Dùng thư mục mặc định",
  "location.desktop": "Tạo lối tắt ngoài màn hình (Desktop)",
  "location.startMenu":
    "Luôn tạo lối tắt trong menu Start. Gỡ cài đặt bất cứ lúc nào trong Settings → Apps.",
  "location.running":
    "NANOMultipleGitGUI đang mở và sẽ bị đóng trong lúc cài — hãy lưu công việc trước.",
  "location.summary": "Sẵn sàng cài NANOMultipleGitGUI {version}.",
  "error.notAbsolute": "Nhập đường dẫn đầy đủ, ví dụ {example}.",
  "error.notWritable":
    "Thư mục này cần quyền quản trị. Hãy chọn thư mục trong hồ sơ người dùng của bạn, ví dụ thư mục mặc định.",
  "error.notEmpty": "Thư mục này đã có file khác. Hãy chọn thư mục trống hoặc thư mục mới.",
  "install.title": "Đang cài NANOMultipleGitGUI",
  "install.extracting": "Đang chuẩn bị file…",
  "install.installing": "Đang chép app vào máy…",
  "install.finishing": "Đang tạo lối tắt…",
  "install.hint": "Thường mất chưa tới một phút.",
  "done.title": "Xong rồi!",
  "done.body": "Đã cài NANOMultipleGitGUI {version} vào {dir}.",
  "done.next":
    "Khi mở app, phần Hướng dẫn ban đầu sẽ giúp bạn kiểm tra Git và thêm tài khoản GitHub / GitLab.",
  "done.launch": "Mở NANOMultipleGitGUI ngay",
  "failed.title": "Cài đặt chưa hoàn tất",
  "failed.body":
    "Chưa có thay đổi quan trọng nào. Bạn có thể thử lại, hoặc dùng bộ cài cổ điển ở trang Releases.",
  "failed.details": "Chi tiết: {code}",
  "failed.exit2": "Hãy đóng NANOMultipleGitGUI (xem cả khay hệ thống) rồi thử lại.",
  "button.back": "Quay lại",
  "button.next": "Tiếp tục",
  "button.install": "Cài đặt",
  "button.cancel": "Hủy",
  "button.finish": "Hoàn tất",
  "button.retry": "Thử lại",
  "button.close": "Đóng",
};

const messages: Record<Language, Record<MessageKey, string>> = { en, vi };

export function detectLanguage(locale: string | undefined): Language {
  return locale?.toLowerCase().startsWith("vi") ? "vi" : "en";
}

export function translate(
  lang: Language,
  key: MessageKey,
  params: Record<string, string> = {},
): string {
  return messages[lang][key].replace(/\{(\w+)\}/g, (match, name: string) => params[name] ?? match);
}

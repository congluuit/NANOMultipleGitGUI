/** Phase 6.7: Hướng dẫn đầy đủ — tiếng Việt. Giữ cùng cấu trúc với `en.ts`. `**x**` = in đậm. */
import type { GuideTopic } from "./types";

export const guideVi: GuideTopic[] = [
  {
    id: "start",
    title: "Bắt đầu & màn hình chính",
    summary: "App làm được gì và từng vùng trên cửa sổ dùng để làm gì.",
    sections: [
      {
        id: "what",
        title: "App này là gì?",
        blocks: [
          {
            kind: "p",
            text: "NANOMultipleGitGUI giúp bạn dùng Git bằng nút bấm thay vì gõ lệnh, và được làm cho người có nhiều tài khoản GitHub / GitLab (ví dụ một tài khoản công ty, một tài khoản cá nhân). Mỗi repo nhớ nó dùng tài khoản nào, nên commit và push luôn đi đúng tên.",
          },
          {
            kind: "term",
            term: "Git",
            text: "công cụ lưu lại mọi phiên bản bạn đã lưu của dự án, để quay lại lúc trước và làm chung với người khác mà không đè lên nhau.",
          },
          {
            kind: "term",
            term: "Repository (repo)",
            text: "một thư mục dự án mà Git đang ghi lại lịch sử.",
          },
          {
            kind: "tip",
            text: "Lần đầu mở app, trình **Hướng dẫn ban đầu** ngắn sẽ kiểm tra Git đã cài chưa và giúp bạn thêm tài khoản. Chạy lại được ở **Cài đặt → Hướng dẫn ban đầu**.",
          },
        ],
      },
      {
        id: "areas",
        title: "Các vùng trên cửa sổ",
        blocks: [
          {
            kind: "list",
            items: [
              "**Thanh trên cùng**: nút tài khoản (repo này đang dùng tài khoản nào — đường kẻ mảnh trên cùng cửa sổ cũng mang màu của tài khoản đó), mỗi repo đang mở là một tab, nút **+** (mở, tạo hoặc clone repo) và ô **Tìm kiếm / lệnh** (Ctrl+K).",
              "**Thanh công cụ**: Fetch, Pull, Push, Branch, Stash, Pop, Hoàn tác, Làm lại; bên phải là **Việc của tôi**, Thanh bên, Bảng phải, Terminal và **Cài đặt**. Khi cửa sổ hẹp, nút thừa được dồn vào menu **⋯**.",
              "**Panel trái**: các nhánh (**Local** = trên máy bạn, **Remote** = trên server), Tags, Stashes, PR / MR, Issues, LFS, Submodules, Worktrees. Ô trên cùng để lọc nhánh theo tên.",
              "**Graph** (ở giữa): mỗi commit là một dòng, mới nhất ở trên, các đường nối cho thấy nhánh tách ra và gộp lại thế nào. Cột: Nhánh / Tag, Graph, Commit message, Tác giả, Thời gian. Dòng **WIP** trên cùng nghĩa là bạn có thay đổi chưa commit.",
              "**Panel phải**: **Thay đổi** (các file bạn đã sửa, ô commit) — hoặc **Chi tiết commit** khi bạn bấm vào một commit trên graph.",
              "**Thanh trạng thái** (dưới cùng): nhánh hiện tại, số commit đi trước (↑) / đi sau (↓) so với server, và hàng đợi truyền tải (fetch / pull / push / tải file đang chạy).",
            ],
          },
          {
            kind: "p",
            text: "Thông báo hiện ở thanh ngay dưới thanh trên cùng. Nhiều thông báo có nút sửa lỗi giúp bạn — đọc trước khi đóng. **Thanh màu vàng** nghĩa là một thao tác (merge, rebase…) đang tạm dừng chờ bạn.",
          },
          {
            kind: "tip",
            text: "Kéo đường viền mảnh giữa các panel để đổi kích thước (nhấp đúp vào đường viền để về mặc định). Ctrl+B ẩn / hiện panel trái, Ctrl+Alt+B ẩn / hiện panel phải.",
          },
        ],
      },
      {
        id: "first-day",
        title: "Một ngày làm việc trong 5 bước",
        blocks: [
          {
            kind: "steps",
            items: [
              "Mở repo (tab ở trên) và bấm **Pull** để lấy phần việc mới nhất của đồng đội.",
              "Nhấp đúp vào nhánh bạn muốn làm ở panel trái (**Local**), hoặc tạo nhánh mới bằng nút **Branch**.",
              "Sửa file trong trình soạn code. File sửa sẽ hiện ở panel phải, mục **Chưa stage**.",
              "Bấm **Stage** ở các file muốn lưu, gõ tóm tắt ngắn, rồi bấm **Commit**.",
              "Bấm **Push** để gửi commit lên server.",
            ],
          },
        ],
      },
    ],
  },
  {
    id: "repos",
    title: "Mở, tạo hoặc clone repo",
    summary: "Đưa dự án vào app: có sẵn trên máy, tạo mới, hoặc lấy từ GitHub / GitLab.",
    sections: [
      {
        id: "open",
        title: "Mở repo có sẵn trên máy",
        blocks: [
          {
            kind: "steps",
            items: [
              "Bấm **+** bên phải các tab (thanh trên cùng), chọn **Mở repo…**.",
              "Chọn thư mục dự án. Nếu thư mục chưa phải repo Git, app sẽ hỏi có tạo repo ở đó không.",
            ],
          },
          {
            kind: "p",
            text: "Các repo đang mở sẽ hiện lại thành tab ở lần mở app sau. Nút **+** cũng liệt kê các repo mở gần đây.",
          },
        ],
      },
      {
        id: "new",
        title: "Tạo repo mới",
        blocks: [
          {
            kind: "steps",
            items: [
              "Bấm **+** → **Tạo repo mới…**, chọn (hoặc tạo) một thư mục trống.",
              "Giữ tên nhánh đầu tiên (thường là main) rồi bấm tạo.",
            ],
          },
        ],
      },
      {
        id: "clone",
        title: "Clone repo từ GitHub / GitLab",
        blocks: [
          {
            kind: "term",
            term: "Clone",
            text: "tải bản sao đầy đủ của một dự án từ server về máy, kèm toàn bộ lịch sử.",
          },
          {
            kind: "steps",
            items: [
              "Bấm **+** → **Clone repository…**.",
              "Tìm trong danh sách (gồm repo của mọi tài khoản đã đăng nhập) hoặc dán URL vào ô **Hoặc dán URL**. App tự đoán tài khoản phù hợp; đổi ở mục **Tài khoản** nếu cần.",
              "Chọn **Thư mục**. Trong **Tùy chọn** có thể tải luôn submodule, chỉ lấy bản nông (shallow) hoặc chọn file LFS nào cần tải.",
              "Bấm **Clone**. Tiến độ hiện ngay trong hộp thoại; hủy lúc nào cũng được (thư mục tải dở sẽ được xóa).",
            ],
          },
          {
            kind: "tip",
            text: "Muốn có bản riêng của dự án người khác? Trong hộp thoại clone, đánh dấu **Fork về tài khoản … rồi clone**: app tạo fork trên tài khoản của bạn, clone fork đó và thêm repo gốc làm remote “upstream”.",
          },
        ],
      },
    ],
  },
  {
    id: "accounts",
    title: "Tài khoản",
    summary:
      "Nhiều tài khoản GitHub / GitLab, repo dùng tài khoản nào, đổi tài khoản, SSH key, đăng nhập lại.",
    sections: [
      {
        id: "add",
        title: "Thêm tài khoản",
        blocks: [
          {
            kind: "steps",
            items: [
              "Mở **Cài đặt** (bánh răng trên thanh công cụ) → **Quản lý tài khoản…** → tab **Tài khoản** → **Thêm tài khoản**.",
              "Chọn tài khoản nằm ở đâu: GitHub.com, GitHub Enterprise, GitLab.com hoặc GitLab tự host.",
              "Chọn cách đăng nhập: gõ mã thiết bị ngắn trên trang web, đăng nhập qua trình duyệt, hoặc dán token cá nhân (PAT).",
            ],
          },
          {
            kind: "p",
            text: "Đã đăng nhập bằng GitHub CLI (gh) hoặc GitLab CLI (glab)? Phần đầu trình quản lý tài khoản liệt kê các tài khoản đó — bấm **Thêm** là dùng được ngay, không cần đăng nhập lại. Token được cất trong kho mật khẩu của hệ điều hành (keychain), không bao giờ nằm trong file thường.",
          },
          {
            kind: "term",
            term: "Token",
            text: "một chuỗi bí mật dài, dùng như mật khẩu cho app; có thể thu hồi trên trang web bất cứ lúc nào.",
          },
        ],
      },
      {
        id: "which",
        title: "Repo đang dùng tài khoản nào?",
        blocks: [
          {
            kind: "p",
            text: "**Nút tài khoản** ở đầu bên trái thanh trên cùng cho biết điều đó, kèm lý do: chọn thủ công, đã khóa, từ lúc clone, hoặc theo rule. Repo chưa có tài khoản sẽ có một dải thông báo nhắc bạn chọn.",
          },
          {
            kind: "list",
            items: [
              "**Quy tắc** (trình quản lý tài khoản → tab **Quy tắc**) tự chọn tài khoản, ví dụ “mọi repo ở github.com/cong-ty-toi dùng tài khoản Công ty”. Quy tắc được xét từ trên xuống.",
              "Mỗi tài khoản có thể có nhiều **danh tính commit** (tên + email ghi vào commit) — ví dụ email công ty và email cá nhân.",
              "Một repo có thể dùng tài khoản khác nhau cho từng remote (xem mục **Theo từng remote** trong nút tài khoản).",
            ],
          },
        ],
      },
      {
        id: "switch",
        title: "Đổi tài khoản, hoặc dùng tài khoản khác một lần",
        blocks: [
          {
            kind: "steps",
            items: [
              "Bấm vào nút tài khoản.",
              "Chọn **Dùng cho repo này** (repo sẽ dùng tài khoản đó từ nay; đánh dấu **Ghi nhớ** để khóa lại, quy tắc không đổi được) hoặc **Chỉ cho thao tác này** (chỉ lần commit / push kế tiếp dùng tài khoản đó; xong thì quay về tài khoản quen của repo).",
              "Bấm vào tài khoản (hoặc một danh tính của nó).",
            ],
          },
          {
            kind: "p",
            text: "Phím tắt: Ctrl+Alt+1 … 9 gắn repo đang mở với tài khoản thứ 1 … 9 (theo thứ tự trong trình quản lý tài khoản). Ở màn hình PR / issue, nút “với tư cách ● tài khoản ▾” cũng có **Chỉ cho thao tác này**.",
          },
        ],
      },
      {
        id: "ssh",
        title: "SSH key",
        blocks: [
          {
            kind: "term",
            term: "SSH key",
            text: "một cặp file chứng minh bạn là ai với GitHub / GitLab mà không cần gõ mật khẩu; dùng cho remote có dạng git@….",
          },
          {
            kind: "steps",
            items: [
              "Trình quản lý tài khoản → tab **Tài khoản** → **SSH key…** ở tài khoản cần dùng.",
              "**Tạo khóa mới** (hoặc nhập từ file), rồi **Tải lên provider** và **Kiểm tra kết nối**.",
            ],
          },
          {
            kind: "p",
            text: "App tự dùng đúng khóa cho từng tài khoản — bạn không cần tạo bí danh host trong ~/.ssh/config.",
          },
        ],
      },
      {
        id: "reauth",
        title: "Đăng nhập lại",
        blocks: [
          {
            kind: "p",
            text: "Khi token hết hạn hoặc bị thu hồi, tài khoản hiện “Cần đăng nhập lại” và thông báo lỗi có nút **Đăng nhập lại**. Bạn cũng có thể mở trình quản lý tài khoản và bấm **Đăng nhập lại** ở tài khoản đó. Đăng nhập GitHub được tự gia hạn trong lúc app chạy.",
          },
        ],
      },
      {
        id: "export",
        title: "Xuất / nhập cấu hình tài khoản",
        blocks: [
          {
            kind: "p",
            text: "Chuyển sang máy khác? Trình quản lý tài khoản → tab **Tài khoản** → **Xuất cấu hình…** lưu tài khoản, danh tính và quy tắc vào một file được khóa bằng mật khẩu bạn tự đặt. Ở máy mới dùng **Nhập cấu hình…** với đúng mật khẩu đó. Xem thêm mục “Sao lưu & chuyển máy”.",
          },
        ],
      },
    ],
  },
  {
    id: "commit",
    title: "Commit thay đổi",
    summary: "Stage, bỏ stage, hoàn tác, commit, amend, gợi ý bằng AI.",
    sections: [
      {
        id: "stage",
        title: "Chọn những gì đưa vào commit (stage)",
        blocks: [
          {
            kind: "term",
            term: "Commit",
            text: "một lần chụp lại các file kèm một câu ngắn nói đã đổi gì.",
          },
          {
            kind: "term",
            term: "Stage",
            text: "bỏ thay đổi vào “giỏ” những thứ commit tới sẽ lưu; thay đổi chưa stage vẫn nằm trên ổ đĩa nhưng không được lưu vào commit.",
          },
          {
            kind: "list",
            items: [
              "Panel phải → **Chưa stage**: bấm **Stage** ở từng file (hoặc **Stage tất cả**).",
              "Lỡ stage nhầm? Ở mục **Đã stage** bấm **Bỏ stage** (hoặc **Bỏ stage tất cả**).",
              "Bấm vào tên file để mở diff (thay đổi từng dòng). Trong diff có thể chỉ stage một khối (hunk) hoặc vài dòng: bấm chọn dòng, rồi **Stage N dòng**.",
              "**Hoàn tác** ở file chưa stage sẽ bỏ đi phần bạn đã sửa trong file đó (có hỏi lại; không lấy lại được).",
            ],
          },
        ],
      },
      {
        id: "write",
        title: "Viết message rồi commit",
        blocks: [
          {
            kind: "steps",
            items: [
              "Gõ **Tóm tắt** ngắn (bạn đã làm gì, ví dụ “Sửa nút đăng nhập trên điện thoại”). Cần thì ghi thêm ở **Mô tả chi tiết**.",
              "Bấm **Commit N file** (hoặc nhấn Ctrl+Enter).",
            ],
          },
          {
            kind: "p",
            text: "Trước khi commit, app có thể hiện **Kiểm tra trước khi commit** nếu tên / email không khớp tài khoản của repo, và **Large File Guard** nếu có file quá lớn (xem mục “File lớn & LFS”).",
          },
          {
            kind: "tip",
            text: "Gõ # trong message để chọn issue trên server: Enter chèn “#12”, Shift+Enter chèn “Closes #12” (issue tự đóng khi commit vào nhánh chính).",
          },
        ],
      },
      {
        id: "amend",
        title: "Sửa commit vừa làm (amend)",
        blocks: [
          {
            kind: "term",
            term: "Amend",
            text: "thay commit cuối bằng bản đã sửa (message mới và/hoặc thêm file) thay vì tạo thêm commit.",
          },
          {
            kind: "p",
            text: "Đánh dấu **Sửa commit trước (amend)** dưới ô message, sửa message hoặc stage thêm file, rồi bấm **Sửa commit trước**. Tránh amend commit đã push: bản trên server sẽ khác bản trên máy bạn.",
          },
        ],
      },
      {
        id: "ai",
        title: "Gợi ý commit message bằng AI ✨",
        blocks: [
          {
            kind: "steps",
            items: [
              "Bật một lần: **Cài đặt → Gợi ý commit bằng AI → Cài đặt AI…**, bật **Bật gợi ý commit bằng AI** rồi chọn nguồn AI (máy tính của bạn với Ollama — miễn phí, code không rời máy — hoặc dịch vụ như OpenAI dùng API key của bạn).",
              "Stage các file, rồi bấm **✨** cạnh ô tóm tắt. Gợi ý được điền vào ô message — đọc và sửa lại trước khi commit.",
            ],
          },
          {
            kind: "warn",
            text: "Phần thay đổi đã stage sẽ được gửi tới dịch vụ AI bạn chọn. Đừng dùng dịch vụ trên mạng cho code bạn không được phép chia sẻ; model chạy trên máy giữ mọi thứ trong máy bạn.",
          },
        ],
      },
    ],
  },
  {
    id: "branches",
    title: "Nhánh",
    summary: "Tạo, chuyển nhánh (nhấp đúp, tự cất tạm), merge, rebase, kéo-thả, xóa.",
    sections: [
      {
        id: "what",
        title: "Nhánh là gì?",
        blocks: [
          {
            kind: "term",
            term: "Nhánh (branch)",
            text: "một hướng làm việc riêng: thử nghiệm trên nhánh mà không đụng bản chính, xong rồi gộp lại sau.",
          },
          {
            kind: "term",
            term: "Checkout / chuyển nhánh",
            text: "chuyển sang nhánh khác — các file trong thư mục đổi thành phiên bản của nhánh đó.",
          },
        ],
      },
      {
        id: "create",
        title: "Tạo nhánh",
        blocks: [
          {
            kind: "steps",
            items: [
              "Bấm **Branch** trên thanh công cụ (hoặc chuột phải một commit → **Tạo nhánh tại đây…**).",
              "Gõ tên không có dấu cách, ví dụ feature/dang-nhap, giữ dấu tích **Checkout nhánh mới sau khi tạo** rồi bấm **Tạo**.",
            ],
          },
        ],
      },
      {
        id: "switch",
        title: "Chuyển nhánh: nhấp đúp",
        blocks: [
          {
            kind: "steps",
            items: [
              "Ở panel trái, mục **Local**, **nhấp đúp** vào tên nhánh (hoặc chọn bằng bàn phím rồi nhấn Enter). Nhấp một lần chỉ đưa graph tới commit mới nhất của nhánh.",
              "Nhánh chỉ có ở mục **Remote** (ví dụ origin/feature)? Cũng nhấp đúp: app tạo bản local theo dõi nhánh đó rồi chuyển sang.",
            ],
          },
          {
            kind: "p",
            text: "Nếu bạn có thay đổi chưa commit, app sẽ hỏi làm gì với chúng:",
          },
          {
            kind: "list",
            items: [
              "**Cất tạm thay đổi rồi chuyển** (khuyến nghị): thay đổi được cất an toàn (vào một stash tên “nanomgg-autostash: <nhánh>”), rồi mới chuyển nhánh; khi bạn quay lại nhánh cũ, chúng được **tự trả lại**.",
              "**Mang thay đổi sang nhánh mới**: thay đổi đi theo bạn sang nhánh kia. Nếu Git từ chối vì sẽ ghi đè file ở nhánh đó, app báo và gợi ý chọn cách thứ nhất.",
              "Đánh dấu **Ghi nhớ lựa chọn** để không phải hỏi lại. Đổi lại sau ở **Cài đặt → Chuyển nhánh** (Luôn hỏi / Luôn cất tạm / Luôn mang theo, và bật / tắt tự khôi phục).",
            ],
          },
          {
            kind: "p",
            text: "App cũng dừng lại và giải thích khi chuyển nhánh không an toàn: đang làm dở merge / rebase (hoàn tất hoặc hủy trước), đang pull / push (đợi xong), nhánh đang mở ở một worktree khác (nút **Mở worktree đó**), hoặc bạn đang đứng ở commit không thuộc nhánh nào (gợi ý **Tạo nhánh ở đây trước**).",
          },
          {
            kind: "tip",
            text: "Sau khi chuyển, thông báo có nút **Quay lại <nhánh>**. Nếu dự án có submodule giờ trỏ sang commit khác, thông báo gợi ý **Cập nhật submodule**.",
          },
        ],
      },
      {
        id: "merge",
        title: "Gộp nhánh: merge và rebase",
        blocks: [
          {
            kind: "term",
            term: "Merge",
            text: "đưa các commit của nhánh khác vào nhánh hiện tại, giữ nguyên lịch sử cả hai bên.",
          },
          {
            kind: "term",
            term: "Rebase",
            text: "dời các commit của bạn để bắt đầu từ commit mới nhất của nhánh khác, cho lịch sử thẳng một đường (commit của bạn đổi mã mới).",
          },
          {
            kind: "list",
            items: [
              "Chuột phải một nhánh → **Merge vào <nhánh hiện tại>…**, chọn cách merge, rồi bấm **Merge**.",
              "Chuột phải → **Rebase <nhánh hiện tại> lên <nhánh>**. App tạo bản sao lưu trước, nên luôn hoàn tác được.",
              "**Kéo-thả**: kéo một nhánh ở panel trái (hoặc nhãn nhánh trên graph) thả lên nhánh khác, rồi chọn Fast-forward, Merge hoặc Rebase.",
              "**Interactive rebase** (chuột phải commit → **Interactive rebase từ đây…**): kéo để đổi thứ tự commit (hoặc Alt+↑ / Alt+↓), chọn pick / reword / squash / fixup / drop / edit cho từng commit.",
            ],
          },
          {
            kind: "p",
            text: "Nếu hai bên cùng sửa một chỗ, Git dừng lại vì xung đột — xem mục “Xung đột”.",
          },
        ],
      },
      {
        id: "delete",
        title: "Đổi tên hoặc xóa nhánh",
        blocks: [
          {
            kind: "p",
            text: "Chuột phải nhánh → **Đổi tên…** hoặc **Xóa…**. Không xóa được nhánh đang đứng — chuyển sang nhánh khác trước. Xóa nhánh có commit chưa gộp vào đâu sẽ được hỏi lại (**Vẫn xóa**). Menu nhánh cũng có mục xóa bản trên server, hoặc xóa cả hai.",
          },
        ],
      },
    ],
  },
  {
    id: "sync",
    title: "Đồng bộ với server",
    summary: "Fetch, pull, push, số ↑ / ↓, tự fetch nền, Push Guard.",
    sections: [
      {
        id: "words",
        title: "Fetch, pull và push",
        blocks: [
          {
            kind: "term",
            term: "Remote",
            text: "bản repo nằm trên server (GitHub / GitLab), thường tên là origin.",
          },
          {
            kind: "list",
            items: [
              "**Fetch**: hỏi server có gì mới, không đổi file của bạn.",
              "**Pull**: fetch rồi đưa commit mới vào nhánh hiện tại. Mũi tên ▾ có merge (mặc định), rebase hoặc chỉ fast-forward.",
              "**Push**: gửi commit mới của bạn lên server. Mũi tên ▾ có **Force push** an toàn — chỉ dùng khi bạn hiểu rõ vì sao.",
            ],
          },
        ],
      },
      {
        id: "badges",
        title: "Các số ↑ / ↓",
        blocks: [
          {
            kind: "p",
            text: "↑3 trên nút Push = 3 commit của bạn chưa lên server. ↓2 trên nút Pull = server có 2 commit bạn chưa có. Màu vàng là có cả hai — pull trước. Nếu bấm Push khi đang thiếu commit, app đề nghị **Pull rồi push**.",
          },
          {
            kind: "p",
            text: "App tự hỏi server (khi mở repo, vài phút một lần và khi bạn quay lại cửa sổ), lặng lẽ, và chỉ với tài khoản đã đăng nhập.",
          },
        ],
      },
      {
        id: "guard",
        title: "Push Guard",
        blocks: [
          {
            kind: "p",
            text: "Trước khi push có thể hiện **Kiểm tra trước khi push**: tài khoản không có quyền ghi vào repo, có commit mang tác giả khác danh tính của bạn (app sửa được tác giả của commit chưa push), hoặc có file quá lớn với server (app chuyển được sang LFS — **Chuyển sang LFS** — có bản sao lưu để hoàn tác).",
          },
          {
            kind: "p",
            text: "Các lần truyền tải đang chạy nằm ở thanh trạng thái (hàng đợi truyền tải): hủy, thử lại và sao chép chi tiết lỗi ở đó.",
          },
        ],
      },
    ],
  },
  {
    id: "conflicts",
    title: "Xung đột",
    summary: "Khi hai thay đổi cùng đụng một chỗ: cách chọn giữ cái nào.",
    sections: [
      {
        id: "what",
        title: "Xung đột là gì?",
        blocks: [
          {
            kind: "term",
            term: "Xung đột (conflict)",
            text: "Git không tự quyết được vì cùng một dòng bị sửa khác nhau ở hai bên; bạn chọn kết quả.",
          },
          {
            kind: "p",
            text: "Thanh vàng cho biết thao tác đang tạm dừng và còn bao nhiêu file xung đột. Các file xung đột nằm ở mục **Xung đột** trong panel phải.",
          },
        ],
      },
      {
        id: "resolve",
        title: "Giải quyết từng bước",
        blocks: [
          {
            kind: "steps",
            items: [
              "Bấm vào một file ở mục **Xung đột** → **Giải quyết xung đột**.",
              "Phía trên là “của tôi” và “của họ” đặt cạnh nhau (chỉ đọc); phía dưới là **Kết quả** sẽ được lưu. Ở từng khối bấm **Lấy** của tôi, **Lấy** của họ hoặc **Lấy cả hai**, hoặc gõ thẳng vào kết quả. ‹ › để nhảy giữa các khối.",
              "Bấm **Lưu & đánh dấu đã giải quyết**. File xung đột tiếp theo tự mở ra.",
              "Khi hết xung đột, bấm **Tiếp tục** ở thanh vàng (hoặc **Hủy** để quay về như trước khi bắt đầu).",
            ],
          },
          {
            kind: "warn",
            text: "Khi rebase, Git đảo “ours” và “theirs”. Nhãn trong trình sửa ghi rõ bên nào là bên nào — bấm **Ours / theirs là gì?** nếu chưa chắc.",
          },
          {
            kind: "p",
            text: "Ảnh và file nhị phân không gộp từng dòng được: trình sửa cho xem cả hai phiên bản để bạn giữ một bên.",
          },
        ],
      },
    ],
  },
  {
    id: "history",
    title: "Lịch sử",
    summary: "Graph, tìm commit, lịch sử file, blame, xem file tại một commit.",
    sections: [
      {
        id: "graph",
        title: "Đọc graph",
        blocks: [
          {
            kind: "p",
            text: "Mỗi dòng là một commit. Đường màu là các nhánh; điểm có hai đường đi vào là một lần merge. Nhãn cho biết nhánh (laptop = local, đám mây = server, ✓ = nhánh đang đứng) và tag. Bấm vào commit để xem chi tiết bên phải; chuột phải để có thao tác (checkout, tạo nhánh, cherry-pick, revert, reset, tag…).",
          },
          {
            kind: "term",
            term: "HEAD",
            text: "chỗ bạn đang đứng — thường là commit mới nhất của nhánh hiện tại.",
          },
          {
            kind: "term",
            term: "SHA",
            text: "mã định danh duy nhất của một commit, kiểu 3f2a9c1.",
          },
        ],
      },
      {
        id: "search",
        title: "Tìm commit",
        blocks: [
          {
            kind: "p",
            text: "Nhấn Ctrl+F (hoặc bấm ô tìm phía trên graph) rồi gõ chữ trong message, tên tác giả hoặc SHA. Enter / Shift+Enter nhảy tới kết quả sau / trước. Bảng lệnh → **Đi tới commit theo SHA…** để nhảy thẳng.",
          },
        ],
      },
      {
        id: "file",
        title: "Lịch sử một file, blame, file ở commit cũ",
        blocks: [
          {
            kind: "list",
            items: [
              "Chuột phải một file (ở **Thay đổi** hoặc **Chi tiết commit**) → **Lịch sử file**: mọi commit đã sửa file đó.",
              "→ **Blame (ai sửa dòng nào)**: từng dòng kèm commit và người sửa nó lần cuối; bấm vào dòng để xem commit đó trên graph.",
              "Trong lịch sử file, **Xem file tại commit này** cho thấy toàn bộ file như lúc đó.",
            ],
          },
        ],
      },
    ],
  },
  {
    id: "stash",
    title: "Stash",
    summary: "Cất tạm thay đổi đang làm dở và lấy lại sau.",
    sections: [
      {
        id: "use",
        title: "Dùng stash",
        blocks: [
          {
            kind: "term",
            term: "Stash",
            text: "ngăn kéo Git dùng để cất thay đổi đang làm dở cho thư mục sạch; lấy ra lại sau.",
          },
          {
            kind: "list",
            items: [
              "Nút **Stash** trên thanh công cụ: cất thay đổi (có thể kèm ghi chú và gồm cả file mới).",
              "Nút **Pop** trên thanh công cụ: lấy lại stash mới nhất và xóa nó khỏi danh sách.",
              "Panel trái → **Stashes**: nhấp đúp để xem file trong stash; chuột phải để **Áp dụng** (vẫn giữ trong danh sách), **Pop** hoặc **Xóa stash**.",
            ],
          },
          {
            kind: "tip",
            text: "Stash tên “nanomgg-autostash: <nhánh>” do tính năng chuyển nhánh an toàn tạo. Chúng tự quay về khi bạn trở lại nhánh đó.",
          },
        ],
      },
    ],
  },
  {
    id: "undo",
    title: "Hoàn tác / Làm lại",
    summary: "Lấy lại các thao tác vừa làm: commit, merge, rebase, reset, đổi nhánh…",
    sections: [
      {
        id: "use",
        title: "Cách hoạt động",
        blocks: [
          {
            kind: "p",
            text: "Nút **Hoàn tác** / **Làm lại** trên thanh công cụ (Ctrl+Z / Ctrl+Shift+Z khi không gõ trong ô chữ) hoàn tác hoặc làm lại thao tác gần nhất: commit, amend, reset, chuyển nhánh, merge, rebase, cherry-pick, revert, pull, tạo / xóa / đổi tên nhánh. Rê chuột lên nút để xem nó sẽ hoàn tác việc gì.",
          },
          {
            kind: "list",
            items: [
              "Hoàn tác một commit trả thay đổi về trạng thái đã stage — không mất gì.",
              "Hoàn tác không bao giờ ghi đè thay đổi chưa commit; nếu repo đã đổi khác, app sẽ báo thay vì làm.",
              "Nếu các commit đã push, app hỏi trước: hoàn tác chỉ đổi trên máy bạn.",
            ],
          },
        ],
      },
    ],
  },
  {
    id: "lfs",
    title: "File lớn & LFS",
    summary: "Giữ file lớn (ảnh, video, tài nguyên game) ngoài lịch sử; Large File Guard.",
    sections: [
      {
        id: "what",
        title: "LFS là gì?",
        blocks: [
          {
            kind: "term",
            term: "Git LFS",
            text: "phần mở rộng cất file lớn ở một kho riêng và chỉ giữ một “con trỏ” nhỏ trong lịch sử, để repo luôn nhẹ.",
          },
          {
            kind: "p",
            text: "Panel trái → **LFS** cho biết repo có dùng LFS không và đang theo dõi những mẫu file nào. **Quản lý LFS…** mở cửa sổ Git LFS: **File** (tải các file mới chỉ là con trỏ), **Khóa** (khóa file để người khác không sửa cùng lúc), **Pattern**, **Bảo trì** (giải phóng ổ đĩa, kiểm tra toàn vẹn).",
          },
        ],
      },
      {
        id: "guard",
        title: "Large File Guard",
        blocks: [
          {
            kind: "p",
            text: "Khi commit file lớn, app dừng lại và đề nghị **Track bằng LFS (khuyến nghị)**, **Thêm vào .gitignore** (không lưu file đó vào Git) hoặc **Vẫn commit** (không được nếu vượt giới hạn cứng của server). Ngưỡng có thể chỉnh theo từng server ở trình quản lý tài khoản → tab **Máy chủ**.",
          },
          {
            kind: "tip",
            text: "Lỡ commit file lớn nhưng chưa push? Push Guard có **Chuyển sang LFS** cho các commit chưa push.",
          },
        ],
      },
    ],
  },
  {
    id: "pulls",
    title: "Pull / merge request & Issues",
    summary: "Xem và merge PR / MR, tạo issue, tạo nhánh từ issue.",
    sections: [
      {
        id: "pr",
        title: "Pull request (GitHub) / merge request (GitLab)",
        blocks: [
          {
            kind: "term",
            term: "PR / MR",
            text: "lời đề nghị gộp nhánh của bạn vào nhánh khác trên server, để đồng đội góp ý và duyệt trước.",
          },
          {
            kind: "list",
            items: [
              "Panel trái → **PR / MR**: danh sách của repo này; bấm để mở (Tổng quan, Commit, File thay đổi).",
              "Trong PR: bình luận, bình luận theo dòng (rê chuột lên dòng ở File thay đổi), **Review** (duyệt / yêu cầu sửa), **Checkout** (lấy nhánh của PR về máy), **Merge**.",
              "Tạo mới: chuột phải nhánh của bạn → **Tạo pull request từ <nhánh>…** (app gợi ý push trước), hoặc ngay sau khi push nhánh mới.",
            ],
          },
        ],
      },
      {
        id: "issues",
        title: "Issues",
        blocks: [
          {
            kind: "p",
            text: "Panel trái → **Issues**: lọc, mở, bình luận, đóng / mở lại, **Tạo issue**. Trong một issue, **Tạo nhánh từ issue này** tạo nhánh tên kiểu feature/123-tieu-de.",
          },
          {
            kind: "tip",
            text: "Mỗi thao tác đều có “với tư cách ● tài khoản ▾”: bấm vào để làm riêng lần này bằng tài khoản khác.",
          },
        ],
      },
    ],
  },
  {
    id: "mywork",
    title: "Việc của tôi & Ghi chú của tôi",
    summary:
      "PR đang chờ bạn, PR của bạn, issue giao cho bạn — từ mọi tài khoản — và ghi chú riêng.",
    sections: [
      {
        id: "mywork",
        title: "Việc của tôi",
        blocks: [
          {
            kind: "p",
            text: "Bấm **Việc của tôi** trên thanh công cụ. App gom từ mọi tài khoản đã đăng nhập: PR / MR cần bạn review, PR / MR của bạn, và issue giao cho bạn. Mỗi dòng mang màu của tài khoản; bấm để mở. Tự làm mới 5 phút một lần.",
          },
        ],
      },
      {
        id: "notes",
        title: "Ghi chú của tôi",
        blocks: [
          {
            kind: "p",
            text: "Cũng trong cửa sổ đó, tab **Ghi chú của tôi** là cuốn sổ nhỏ cho riêng bạn (việc cần làm, lời nhắc, lệnh hay quên), có tag để phân loại. Ghi chú nằm trên máy này; xóa được ở **Cài đặt → Lưu trữ → Quản lý lưu trữ… → Dữ liệu của bạn trong app**.",
          },
        ],
      },
    ],
  },
  {
    id: "terminal",
    title: "Terminal tích hợp",
    summary: "Gõ lệnh khi cần, đã gắn sẵn tài khoản của repo.",
    sections: [
      {
        id: "use",
        title: "Dùng terminal",
        blocks: [
          {
            kind: "p",
            text: "Bấm **Terminal** trên thanh công cụ hoặc nhấn Ctrl+` (phím dưới Esc). Terminal mở ngay trong thư mục repo, và lệnh git gõ ở đó chạy bằng tài khoản của repo. Sao chép / dán bằng Ctrl+Shift+C / Ctrl+Shift+V. Mỗi repo mở được nhiều terminal.",
          },
        ],
      },
    ],
  },
  {
    id: "submodules",
    title: "Submodule & worktree",
    summary: "Repo nằm trong repo; nhiều thư mục của cùng một repo.",
    sections: [
      {
        id: "submodules",
        title: "Submodule",
        blocks: [
          {
            kind: "term",
            term: "Submodule",
            text: "một repo khác được nhúng vào một thư mục trong repo của bạn, gắn cố định với một commit.",
          },
          {
            kind: "p",
            text: "Panel trái → **Submodules** liệt kê từng cái kèm trạng thái (chưa tải, đúng commit, lệch commit, có thay đổi). **Cập nhật tất cả** tải về và đưa từng submodule về đúng commit repo của bạn ghi nhận. Chuột phải để mở thành tab, đồng bộ URL, deinit hoặc xóa. Mỗi submodule dùng tài khoản khớp với URL của chính nó.",
          },
        ],
      },
      {
        id: "worktrees",
        title: "Worktree",
        blocks: [
          {
            kind: "term",
            term: "Worktree",
            text: "thư mục thứ hai của cùng một repo, đứng ở nhánh khác, để làm hai nhánh cùng lúc.",
          },
          {
            kind: "p",
            text: "Panel trái → **Worktrees** → **Thêm worktree…** (hoặc chuột phải nhánh → **Tạo worktree từ <nhánh>…**). Mở thành tab, khóa hoặc xóa được. Mỗi nhánh chỉ được mở ở một worktree tại một thời điểm.",
          },
        ],
      },
    ],
  },
  {
    id: "signing",
    title: "Ký commit",
    summary: "Chứng minh commit đúng là của bạn (GPG hoặc SSH).",
    sections: [
      {
        id: "setup",
        title: "Bật ký commit",
        blocks: [
          {
            kind: "steps",
            items: [
              "Trình quản lý tài khoản → tab **Tài khoản** → **Danh tính commit** của tài khoản → **Sửa**.",
              "Ở **Ký commit** chọn GPG hoặc SSH, chọn khóa, rồi bấm **Thử ký**.",
            ],
          },
          {
            kind: "p",
            text: "Chi tiết commit sẽ có dòng **Chữ ký** (Đã xác minh, Khóa chưa tin cậy, Không ký…). Với ký bằng SSH, hãy thêm public key vào phần “signing key” trong cài đặt GitHub / GitLab trên trang web để trang web hiện “Verified”.",
          },
        ],
      },
    ],
  },
  {
    id: "appearance",
    title: "Giao diện & theme tùy chỉnh",
    summary: "Sáng, tối, theo hệ thống, tương phản cao hoặc màu của riêng bạn; ngôn ngữ.",
    sections: [
      {
        id: "theme",
        title: "Theme và ngôn ngữ",
        blocks: [
          {
            kind: "steps",
            items: [
              "**Cài đặt** → **Giao diện**: Sáng / Tối / Theo hệ thống / **Tùy chỉnh**.",
              "**Quản lý theme…** mở trình tạo theme: bắt đầu từ **Tương phản cao — Tối** hoặc **Tương phản cao — Sáng**, hoặc tự chọn màu.",
              "**Ngôn ngữ**: English hoặc Tiếng Việt (đổi ngay).",
            ],
          },
          {
            kind: "p",
            text: "Cũng trong Cài đặt: hiện kết quả CI (mặc định tắt) và avatar tác giả trên graph.",
          },
        ],
      },
    ],
  },
  {
    id: "backup",
    title: "Sao lưu & chuyển máy",
    summary: "Xuất cấu hình tài khoản, sao lưu và khôi phục toàn bộ app.",
    sections: [
      {
        id: "full",
        title: "Sao lưu toàn bộ",
        blocks: [
          {
            kind: "steps",
            items: [
              "**Cài đặt → Sao lưu & khôi phục → Tạo bản sao lưu…**, đặt mật khẩu và chọn nơi lưu file.",
              "Ở máy mới: cài app, rồi **Cài đặt → Sao lưu & khôi phục → Khôi phục từ bản sao lưu…** và nhập đúng mật khẩu đó.",
            ],
          },
          {
            kind: "warn",
            text: "Cất mật khẩu ở nơi an toàn: quên mật khẩu thì không mở được bản sao lưu. Code của bạn không nằm trong bản sao lưu — nó ở trong các repo (hãy push lên server, hoặc chép các thư mục).",
          },
        ],
      },
      {
        id: "accounts",
        title: "Chỉ chuyển tài khoản",
        blocks: [
          {
            kind: "p",
            text: "Trình quản lý tài khoản → tab **Tài khoản** → **Xuất cấu hình…** / **Nhập cấu hình…** chỉ chuyển tài khoản, danh tính và quy tắc.",
          },
        ],
      },
    ],
  },
  {
    id: "storage",
    title: "Lưu trữ & dọn dẹp",
    summary: "App cất log, cache và dữ liệu ở đâu, và cách giải phóng chỗ trống.",
    sections: [
      {
        id: "folders",
        title: "Các thư mục của app",
        blocks: [
          {
            kind: "p",
            text: "**Cài đặt → Lưu trữ → Quản lý lưu trữ…** cho thấy dung lượng và vị trí của **Log**, **Cache** và **Dữ liệu app**. Bạn có thể chuyển từng loại sang ổ khác (**Đổi…**), **Về mặc định**, **Xóa toàn bộ cache** (không mất gì) hoặc **Xóa log cũ**. Repo của bạn không bao giờ được cất hay bị xóa ở đây.",
          },
          {
            kind: "p",
            text: "Mục **Dữ liệu của bạn trong app** liệt kê những gì bạn tạo trong app (theme tùy chỉnh, ghi chú) để xóa từng mục hoặc xóa tất cả.",
          },
          {
            kind: "tip",
            text: "App sẽ cảnh báo khi ổ chứa dữ liệu của app còn dưới 1 GB.",
          },
        ],
      },
    ],
  },
  {
    id: "updates",
    title: "Cập nhật app",
    summary: "Kiểm tra bản mới và cài đặt.",
    sections: [
      {
        id: "check",
        title: "Kiểm tra cập nhật",
        blocks: [
          {
            kind: "p",
            text: "App tự kiểm tra tối đa mỗi ngày một lần sau khi mở. Bạn cũng có thể vào **Cài đặt → Cập nhật** và bấm nút kiểm tra, hoặc dùng bảng lệnh → **Kiểm tra cập nhật**. Khi có bản mới: **Cập nhật & khởi động lại**, **Để sau** hoặc **Bỏ qua bản này**. Chọn **Ổn định (khuyến nghị)** hoặc **Beta** ở Kênh cập nhật.",
          },
          {
            kind: "p",
            text: "Nếu bản build này không tự cập nhật được, app sẽ nói rõ và có link tới trang Releases: tải bộ cài mới và cài đè lên — dữ liệu và tài khoản vẫn giữ nguyên.",
          },
        ],
      },
    ],
  },
  {
    id: "shortcuts",
    title: "Phím tắt",
    summary: "Các phím tiết kiệm thời gian nhất. Trên macOS dùng ⌘ thay cho Ctrl.",
    sections: [
      {
        id: "global",
        title: "Ở mọi nơi",
        blocks: [
          {
            kind: "keys",
            rows: [
              ["Ctrl+K", "Bảng lệnh: tìm và chạy mọi lệnh"],
              [
                "Ctrl+Z / Ctrl+Shift+Z",
                "Hoàn tác / làm lại thao tác gần nhất (không áp dụng khi đang gõ chữ)",
              ],
              ["Ctrl+B", "Ẩn / hiện panel trái"],
              ["Ctrl+Alt+B", "Ẩn / hiện panel phải"],
              ["Ctrl+`", "Ẩn / hiện terminal"],
              ["Ctrl+Alt+1 … 9", "Dùng tài khoản thứ 1 … 9 cho repo đang mở"],
            ],
          },
        ],
      },
      {
        id: "work",
        title: "Khi làm việc",
        blocks: [
          {
            kind: "keys",
            rows: [
              ["Ctrl+F", "Tìm commit trên graph (Enter / Shift+Enter: kết quả sau / trước)"],
              ["↑ ↓ PageUp PageDown Home End", "Di chuyển giữa các commit trên graph"],
              ["Esc", "Đóng chi tiết commit / diff"],
              ["Enter (trên một nhánh ở panel trái)", "Chuyển sang nhánh đó"],
              ["Ctrl+Enter", "Commit (trong ô message); gửi bình luận"],
              ["#", "Trong commit message: gợi ý issue (Enter: #12, Shift+Enter: Closes #12)"],
              ["Alt+↑ / Alt+↓", "Đổi thứ tự commit khi interactive rebase"],
              ["Ctrl+Shift+C / Ctrl+Shift+V", "Sao chép / dán trong terminal"],
            ],
          },
        ],
      },
    ],
  },
  {
    id: "faq",
    title: "Câu hỏi thường gặp & xử lý sự cố",
    summary: "Lỗi hay gặp và bấm vào đâu để sửa.",
    sections: [
      {
        id: "push-rejected",
        title: "Push bị từ chối: “remote có commit mới hơn”",
        blocks: [
          {
            kind: "p",
            text: "Có người đã push trước bạn. Bấm **Pull** (giải quyết xung đột nếu có), rồi **Push** lại — hoặc dùng **Pull rồi push**. Tránh force push nếu chưa chắc chắn.",
          },
        ],
      },
      {
        id: "auth",
        title: "“Xác thực thất bại” hoặc “không có quyền”",
        blocks: [
          {
            kind: "p",
            text: "Xem nút tài khoản: có đúng tài khoản có quyền vào repo không? Đổi tài khoản, hoặc bấm **Đăng nhập lại** trong thông báo. Push file trong .github/workflows cần quyền “workflow” trên GitHub — thông báo có nút để cấp quyền đó.",
          },
        ],
      },
      {
        id: "wrong-author",
        title: "Commit hiện sai tên / email",
        blocks: [
          {
            kind: "p",
            text: "Chọn đúng danh tính trong nút tài khoản, hoặc thêm quy tắc. Với commit chưa push, Push Guard sửa được tác giả trước khi push.",
          },
        ],
      },
      {
        id: "cannot-switch",
        title: "Không chuyển nhánh được",
        blocks: [
          {
            kind: "p",
            text: "Đọc thông báo: hoàn tất hoặc hủy merge / rebase ở thanh vàng, đợi pull / push xong, hoặc chọn **Cất tạm thay đổi rồi chuyển** khi có thay đổi chưa commit. Thay đổi “biến mất” sau khi chuyển nhánh? Chúng nằm trong stash “nanomgg-autostash: <nhánh>”: quay lại nhánh đó (tự trả lại) hoặc Pop từ mục **Stashes**.",
          },
        ],
      },
      {
        id: "detached",
        title: "“HEAD tách rời” (detached HEAD)",
        blocks: [
          {
            kind: "term",
            term: "HEAD tách rời",
            text: "bạn đang đứng ở một commit, không ở nhánh nào; commit mới tạo ở đây rất dễ thất lạc.",
          },
          {
            kind: "p",
            text: "Tạo nhánh ngay tại đó (chuột phải commit → **Tạo nhánh tại đây…**) hoặc nhấp đúp một nhánh có sẵn để quay về.",
          },
        ],
      },
      {
        id: "lfs-pointers",
        title: "File chỉ có vài dòng chữ lạ (con trỏ LFS)",
        blocks: [
          {
            kind: "p",
            text: "Máy chưa có Git LFS hoặc file chưa được tải. Cài git-lfs (**Cài đặt → Hướng dẫn ban đầu** kiểm tra giúp), rồi **Quản lý LFS… → File** và tải về.",
          },
        ],
      },
      {
        id: "report",
        title: "Có gì đó trục trặc",
        blocks: [
          {
            kind: "p",
            text: "Mở **Chi tiết kỹ thuật** trong thông báo để xem nguyên văn lời của Git. Log nằm ở **Cài đặt → Lưu trữ → Quản lý lưu trữ… → Log → Mở thư mục** — gửi kèm file hôm nay khi báo lỗi.",
          },
        ],
      },
    ],
  },
];

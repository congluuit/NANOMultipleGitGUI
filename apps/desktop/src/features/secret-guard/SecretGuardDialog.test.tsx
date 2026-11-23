import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeAll, describe, expect, it, vi } from "vitest";
import { initI18n } from "@/i18n";
import type { SecretReport } from "@/ipc";
import { SecretGuardDialog } from "./SecretGuardDialog";
import { secretFinding, secretReport } from "./testData";

beforeAll(() => {
  initI18n("vi");
});

const env = secretFinding({
  path: "app/.env",
  line: 4,
  ruleId: "aws-access-token",
  ruleName: "AWS access key ID",
  masked: "AKIAQ****",
});
const conf = secretFinding({ path: "src/config.ts", line: 12 });
const sample = secretReport([conf, env]);

function setup(report: SecretReport | null = sample) {
  const props = { onApply: vi.fn(), onCommitAnyway: vi.fn(), onCancel: vi.fn() };
  render(<SecretGuardDialog report={report} {...props} />);
  return props;
}

describe("SecretGuardDialog", () => {
  it("renders nothing when closed", () => {
    setup(null);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("lists rule, file, line and the masked value only", () => {
    setup();
    const dialog = screen.getByRole("dialog");
    expect(
      within(dialog).getByText("2 chỗ có thể là thông tin bí mật trong thay đổi đã stage"),
    ).toBeInTheDocument();
    expect(within(dialog).getByText("AWS access key ID")).toBeInTheDocument();
    expect(within(dialog).getByText("GitHub personal access token")).toBeInTheDocument();
    expect(within(dialog).getByText("AKIAQ****")).toBeInTheDocument();
    expect(within(dialog).getByTitle("src/config.ts:12")).toHaveTextContent(
      "src/config.ts dòng 12",
    );
    // Same order as the report (files in first-seen order).
    const rows = within(dialog).getAllByTestId("secret-finding");
    expect(rows[0]).toHaveTextContent("config.ts");
    expect(rows[1]).toHaveTextContent(".env");
  });

  it("unstages the files by default", async () => {
    const user = userEvent.setup();
    const { onApply } = setup();
    await user.click(screen.getByRole("button", { name: "Áp dụng rồi commit" }));
    expect(onApply).toHaveBeenCalledWith({ unstage: ["src/config.ts", "app/.env"], always: [] });
  });

  it("applies a choice per finding", async () => {
    const user = userEvent.setup();
    const { onApply } = setup();
    const confRow = screen.getByRole("radiogroup", { name: "Xử lý src/config.ts dòng 12" });
    await user.click(within(confRow).getByRole("radio", { name: "Luôn bỏ qua ở repo này" }));
    const envRow = screen.getByRole("radiogroup", { name: "Xử lý app/.env dòng 4" });
    await user.click(within(envRow).getByRole("radio", { name: "Bỏ qua lần này" }));
    expect(screen.getByText(/được app ghi nhớ riêng cho repo này/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Áp dụng rồi commit" }));
    expect(onApply).toHaveBeenCalledWith({ unstage: [], always: [conf.fingerprint] });
  });

  it("sets every finding at once", async () => {
    const user = userEvent.setup();
    const { onApply } = setup();
    await user.click(screen.getByRole("button", { name: "Bỏ qua lần này" }));
    for (const radio of screen.getAllByRole("radio", { name: "Bỏ qua lần này" })) {
      expect(radio).toBeChecked();
    }
    await user.click(screen.getByRole("button", { name: "Áp dụng rồi commit" }));
    expect(onApply).toHaveBeenCalledWith({ unstage: [], always: [] });
  });

  it("commits anyway or cancels", async () => {
    const user = userEvent.setup();
    const { onCommitAnyway, onCancel, onApply } = setup();
    await user.click(screen.getByRole("button", { name: "Vẫn commit" }));
    expect(onCommitAnyway).toHaveBeenCalledOnce();
    await user.click(screen.getByRole("button", { name: "Hủy" }));
    expect(onCancel).toHaveBeenCalled();
    expect(onApply).not.toHaveBeenCalled();
  });

  it("mentions ignored findings and a partial scan", () => {
    setup(secretReport([conf], { ignored: 3, truncated: true }));
    expect(screen.getByText(/Đã bỏ qua 3 chỗ khác/)).toBeInTheDocument();
    expect(screen.getByText(/chỉ quét được một phần/)).toBeInTheDocument();
  });

  it("disables the buttons while applying", () => {
    render(
      <SecretGuardDialog
        report={sample}
        busy
        onApply={vi.fn()}
        onCommitAnyway={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByRole("button", { name: "Vẫn commit" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Áp dụng rồi commit" })).toBeDisabled();
  });
});

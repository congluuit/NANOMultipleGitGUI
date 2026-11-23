import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { initI18n } from "@/i18n";
import { EncodingMenu, EolMenu } from "./EncodingStatus";
import { encodingOverride, useEncodingOverrides } from "./encoding";

beforeEach(() => initI18n("en"));

const group = (name: string) => within(screen.getByRole("group", { name }));

describe("EncodingMenu", () => {
  function setup(props: Partial<Parameters<typeof EncodingMenu>[0]> = {}) {
    const onReopen = vi.fn();
    const onSave = vi.fn();
    render(
      <EncodingMenu
        encoding="utf8Bom"
        canReopen
        canSave
        onReopen={onReopen}
        onSave={onSave}
        {...props}
      />,
    );
    return { onReopen, onSave, user: userEvent.setup() };
  }

  it("shows the encoding and reopens the file in another one", async () => {
    const { onReopen, onSave, user } = setup();
    const trigger = screen.getByTestId("encoding-menu");
    expect(trigger).toHaveTextContent("UTF-8 with BOM");
    await user.click(trigger);
    const reopen = group("Reopen with encoding");
    // UTF-8 with or without BOM reads the same: that is the current choice.
    expect(reopen.getByRole("menuitem", { name: "UTF-8" })).toHaveAttribute("data-current");
    await user.click(reopen.getByRole("menuitem", { name: "UTF-16 LE" }));
    expect(onReopen).toHaveBeenCalledWith("utf16Le");
    expect(onSave).not.toHaveBeenCalled();

    await user.click(trigger);
    await user.click(group("Reopen with encoding").getByRole("menuitem", { name: /Detect/ }));
    expect(onReopen).toHaveBeenLastCalledWith(null);
  });

  it("saves in another encoding", async () => {
    const { onReopen, onSave, user } = setup();
    await user.click(screen.getByTestId("encoding-menu"));
    const save = group("Save with encoding");
    expect(save.getAllByRole("menuitem").map((i) => i.textContent)).toEqual([
      "UTF-8",
      "UTF-8 with BOM",
      "UTF-16 LE",
      "UTF-16 BE",
      "Windows-1252",
    ]);
    expect(save.getByRole("menuitem", { name: "UTF-8 with BOM" })).toHaveAttribute("data-current");
    await user.click(save.getByRole("menuitem", { name: "Windows-1252" }));
    expect(onSave).toHaveBeenCalledWith("windows1252");
    expect(onReopen).not.toHaveBeenCalled();
  });

  it("does not reopen over unsaved edits, and has no save for a read-only file", async () => {
    initI18n("vi");
    const { onReopen, user } = setup({ canReopen: false, canSave: false, encoding: "utf16Be" });
    await user.click(screen.getByTestId("encoding-menu"));
    const reopen = group("Mở lại với bảng mã");
    for (const item of reopen.getAllByRole("menuitem")) {
      expect(item).toHaveAttribute("data-disabled");
    }
    expect(screen.getByText("Hãy lưu hoặc hoàn tác các chỉnh sửa trước.")).toBeInTheDocument();
    expect(screen.queryByRole("group", { name: "Lưu với bảng mã" })).not.toBeInTheDocument();
    await user.click(reopen.getByRole("menuitem", { name: "Windows-1252" }));
    expect(onReopen).not.toHaveBeenCalled();
  });
});

describe("EolMenu", () => {
  it("converts the line endings", async () => {
    const onConvert = vi.fn();
    const user = userEvent.setup();
    render(<EolMenu eol="crlf" canConvert onConvert={onConvert} />);
    const trigger = screen.getByTestId("eol-menu");
    expect(trigger).toHaveTextContent("CRLF");
    await user.click(trigger);
    expect(screen.getByRole("menuitem", { name: "CRLF (Windows)" })).toHaveAttribute(
      "data-current",
    );
    await user.click(screen.getByRole("menuitem", { name: "LF (Linux, macOS)" }));
    expect(onConvert).toHaveBeenCalledWith("lf");
  });

  it("warns about mixed line endings", () => {
    initI18n("vi");
    render(<EolMenu eol="mixed" canConvert onConvert={() => {}} />);
    const trigger = screen.getByTestId("eol-menu");
    expect(trigger).toHaveTextContent("Lẫn lộn");
    expect(trigger).toHaveAttribute(
      "title",
      "File này trộn lẫn LF và CRLF. Chọn một kiểu để chỉnh sửa.",
    );
  });

  it("is only a label for a read-only copy", async () => {
    const onConvert = vi.fn();
    render(<EolMenu eol="lf" canConvert={false} onConvert={onConvert} />);
    expect(screen.getByTestId("eol-menu")).toBeDisabled();
    await userEvent.setup().click(screen.getByTestId("eol-menu"));
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });
});

it("remembers the encoding picked for a file", () => {
  const { set } = useEncodingOverrides.getState();
  set(1, "a.txt", "windows1252");
  expect(encodingOverride(1, "a.txt")).toBe("windows1252");
  expect(encodingOverride(2, "a.txt")).toBeNull();
  set(1, "a.txt", null);
  expect(encodingOverride(1, "a.txt")).toBeNull();
});

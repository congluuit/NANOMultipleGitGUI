import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeAll, describe, expect, it, vi } from "vitest";
import { initI18n } from "@/i18n";
import type { GuardFinding, GuardReport } from "@/ipc";
import { LargeFileGuardDialog } from "./LargeFileGuardDialog";

const MB = 1024 * 1024;

beforeAll(() => {
  initI18n("vi");
});

function finding(over: Partial<GuardFinding> & { path: string }): GuardFinding {
  const ext = over.path.includes(".") ? `*.${over.path.split(".").pop()}` : null;
  return {
    size: MB,
    diskSize: MB,
    stagedSize: MB,
    level: "none",
    alreadyLfs: false,
    stagedPointer: false,
    needsRenormalize: false,
    looksBinary: true,
    looksLikeBuildOutput: false,
    patternByExtension: ext,
    patternByPath: `/${over.path}`,
    ...over,
  };
}

const report = (findings: GuardFinding[], blockBytes: number | null = 100 * MB): GuardReport => ({
  thresholds: { warnBytes: 50 * MB, blockBytes },
  findings,
  suggestCount: 0,
  warnCount: 0,
  blockCount: 0,
});

const sample = report([
  finding({ path: "src/main.ts", looksBinary: false }),
  finding({ path: "Assets/intro.mp4", level: "warn", size: 80 * MB, diskSize: 80 * MB }),
  finding({ path: "Design/home.psd", level: "suggest", size: 3 * MB, diskSize: 3 * MB }),
]);

function setup(r: GuardReport | null = sample) {
  const props = { onTrack: vi.fn(), onCommitAnyway: vi.fn(), onCancel: vi.fn() };
  render(<LargeFileGuardDialog report={r} {...props} />);
  return props;
}

describe("LargeFileGuardDialog", () => {
  it("renders nothing when closed", () => {
    setup(null);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("lists files over the threshold or worth LFS, with sizes and badges", () => {
    setup();
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByText("1 file vượt ngưỡng 50 MB")).toBeInTheDocument();
    expect(within(dialog).getByText("80 MB")).toBeInTheDocument();
    expect(within(dialog).getByText("3 MB")).toBeInTheDocument();
    expect(within(dialog).getByText("Vượt ngưỡng")).toBeInTheDocument();
    expect(within(dialog).getByText("Nên dùng LFS")).toBeInTheDocument();
    expect(within(dialog).getAllByText("nhị phân")).toHaveLength(2);
    expect(within(dialog).queryByText("main.ts")).not.toBeInTheDocument();
    expect(
      within(dialog).getByText("Sẽ thêm vào .gitattributes: *.mp4, *.psd"),
    ).toBeInTheDocument();
  });

  it("tracks the selected files by extension or by path", async () => {
    const user = userEvent.setup();
    const { onTrack } = setup();
    await user.click(screen.getByRole("button", { name: "Track bằng LFS (khuyến nghị)" }));
    expect(onTrack).toHaveBeenLastCalledWith(
      ["*.mp4", "*.psd"],
      ["Assets/intro.mp4", "Design/home.psd"],
    );

    await user.click(screen.getByRole("radio", { name: "Theo đường dẫn" }));
    await user.click(screen.getByRole("checkbox", { name: "Chọn Design/home.psd" }));
    await user.click(screen.getByRole("button", { name: "Track bằng LFS (khuyến nghị)" }));
    expect(onTrack).toHaveBeenLastCalledWith(["/Assets/intro.mp4"], ["Assets/intro.mp4"]);
  });

  it("allows committing anyway below the hard limit, and cancelling", async () => {
    const user = userEvent.setup();
    const { onCommitAnyway, onCancel } = setup();
    expect(screen.getByRole("button", { name: "Thêm vào .gitignore" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Vẫn commit" }));
    expect(onCommitAnyway).toHaveBeenCalledOnce();
    await user.click(screen.getByRole("button", { name: "Hủy" }));
    expect(onCancel).toHaveBeenCalled();
  });

  it("disables commit anyway when a file exceeds the hard limit", () => {
    setup(
      report([
        finding({
          path: "build/app.zip",
          level: "block",
          size: 140 * MB,
          diskSize: 140 * MB,
          looksLikeBuildOutput: true,
        }),
      ]),
    );
    expect(screen.getByRole("button", { name: "Vẫn commit" })).toBeDisabled();
    expect(screen.getByRole("alert")).toHaveTextContent("vượt giới hạn cứng 100 MB");
    expect(screen.getByText("có vẻ là file build")).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "Chọn build/app.zip" })).toBeDisabled();
  });

  it("only re-stages files that are already tracked by LFS", async () => {
    const user = userEvent.setup();
    const { onTrack } = setup(
      report([finding({ path: "art/old.psd", alreadyLfs: true, needsRenormalize: true })]),
    );
    expect(screen.getByText("1 file nên lưu bằng Git LFS")).toBeInTheDocument();
    expect(screen.getByText("cần stage lại")).toBeInTheDocument();
    expect(screen.getByText(/chỉ stage lại file đã track LFS/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Track bằng LFS (khuyến nghị)" }));
    expect(onTrack).toHaveBeenCalledWith([], ["art/old.psd"]);
  });
});

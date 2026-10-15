import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClientProvider } from "@tanstack/react-query";
import i18n from "i18next";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { initI18n } from "@/i18n";
import type { ConflictFile, ConflictStage } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { sideLabels } from "./labels";
import { WholeFileResolver } from "./WholeFileResolver";

const ipc = vi.hoisted(() => ({
  conflictTakeSide: vi.fn(),
  conflictDeleteFile: vi.fn(),
  worktreeStatus: vi.fn(),
  repoOperationState: vi.fn(),
}));
vi.mock("@/ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/ipc")>();
  return { ...actual, commands: { ...actual.commands, ...ipc } };
});
vi.mock("@/features/worktree/queries", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/features/worktree/queries")>()),
  invalidateRepo: vi.fn(),
}));

const stage = (patch: Partial<ConflictStage> = {}): ConflictStage => ({
  oid: "1".repeat(40),
  mode: "100644",
  size: 10,
  binary: true,
  lfs: null,
  text: null,
  ...patch,
});

const file = (patch: Partial<ConflictFile>): ConflictFile => ({
  path: "data/blob.bin",
  resolved: false,
  kind: "bothModified",
  base: stage(),
  ours: stage(),
  theirs: stage({ size: 2048 }),
  workingExists: true,
  workingSize: 10,
  working: null,
  binary: true,
  tooLarge: false,
  editable: false,
  crlf: false,
  bom: false,
  blocks: 0,
  image: false,
  ...patch,
});

const rebaseLabels = () =>
  sideLabels(
    {
      origin: "rebase",
      currentBranch: null,
      ours: { oid: "aaaaaaa1", summary: "upstream", refs: ["main"] },
      theirs: { oid: "bbbbbbb2", summary: "mine", refs: [] },
      theirsName: null,
      rebaseBranch: "feature",
      rebaseOnto: ["main"],
      conflicts: ["data/blob.bin"],
    },
    i18n.t,
  );

function renderResolver(f: ConflictFile) {
  return render(
    <QueryClientProvider client={queryClient}>
      <WholeFileResolver repoId={7} file={f} labels={rebaseLabels()} />
    </QueryClientProvider>,
  );
}

describe("WholeFileResolver", () => {
  beforeAll(() => {
    initI18n("vi");
  });
  beforeEach(() => {
    queryClient.clear();
    Object.values(ipc).forEach((m) => m.mockReset());
    ipc.conflictTakeSide.mockResolvedValue({ status: "done", value: null });
    ipc.conflictDeleteFile.mockResolvedValue({ status: "done", value: null });
    ipc.worktreeStatus.mockResolvedValue({ conflicted: [] });
    ipc.repoOperationState.mockResolvedValue({ operation: "rebase", conflicts: [] });
  });

  it("keeps the picked side of a binary conflict, with rebase-aware labels", async () => {
    const user = userEvent.setup();
    renderResolver(file({}));
    expect(screen.getByText(/không gộp từng dòng được/)).toBeInTheDocument();
    expect(screen.getByText("Dung lượng: 2 KB")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Giữ bản commit của bạn" }));
    expect(ipc.conflictTakeSide).toHaveBeenCalledWith(7, "data/blob.bin", "theirs");
  });

  it("offers to accept a deletion when one side deleted the file", async () => {
    const user = userEvent.setup();
    renderResolver(file({ kind: "deletedByThem", theirs: null, binary: false, editable: true }));
    expect(screen.getByText("Bên commit của bạn đã xóa file này.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Đồng ý xóa file" }));
    expect(ipc.conflictTakeSide).toHaveBeenCalledWith(7, "data/blob.bin", "theirs");
    // No separate "delete" button when a side is already missing.
    expect(screen.queryByRole("button", { name: "Xóa file" })).toBeNull();
  });
});

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import { initI18n } from "@/i18n";
import type { FailureKind } from "@/ipc";
import { useAccountDialogs } from "@/features/accounts/dialogs";
import { NoticeBar } from "@/features/shell/Banners";
import { useUi } from "@/stores/ui";
import { failureNotice } from "./outcome";

const KINDS = {
  invalidName: true,
  invalidInput: true,
  notFound: true,
  alreadyExists: true,
  currentBranch: true,
  notFullyMerged: true,
  localChanges: true,
  untrackedFiles: true,
  unmergedFiles: true,
  operationInProgress: true,
  noOperation: true,
  nothingToCommit: true,
  nonFastForward: true,
  staleLease: true,
  remoteRejected: true,
  authFailed: true,
  connectionFailed: true,
  noUpstream: true,
  detachedHead: true,
  permissionDenied: true,
  workflowScope: true,
  protectedBranch: true,
  emailPrivacy: true,
  secretDetected: true,
  fileTooLarge: true,
  pushedCommits: true,
  checkedOutElsewhere: true,
  dirtyWorktree: true,
  worktreeLocked: true,
  signingFailed: true,
} satisfies Record<FailureKind, true>;

const PUSH_ERROR =
  "To https://github.com/o/r.git\n ! [remote rejected] main -> main (refusing to allow an OAuth App to create or update workflow `.github/workflows/ci.yml` without `workflow` scope)";

beforeAll(() => {
  initI18n("vi");
});

beforeEach(() => {
  useUi.setState({ notice: null });
  useAccountDialogs.setState({ manager: null });
});

describe("failureNotice", () => {
  it.each(["vi", "en"])("has a label and a fix for every failure kind (%s)", async (lang) => {
    await i18n.changeLanguage(lang);
    for (const kind of Object.keys(KINDS) as FailureKind[]) {
      expect(i18n.exists(`failure.${kind}`), kind).toBe(true);
      expect(i18n.exists(`failureHelp.${kind}`), kind).toBe(true);
    }
    await i18n.changeLanguage("vi");
  });

  it("explains a missing workflow scope, keeps git's output and offers the account manager", async () => {
    const user = userEvent.setup();
    const notice = failureNotice("workflowScope", PUSH_ERROR);
    expect(notice.message).toMatch(/workflow/);
    expect(notice.hint).toMatch(/\.github\/workflows/);
    expect(notice.detail).toBe(PUSH_ERROR);

    useUi.getState().showNotice(notice);
    render(<NoticeBar />);
    expect(screen.getByRole("alert")).toHaveTextContent("Bấm nút bên dưới để đăng nhập lại");
    expect(screen.getByText("Chi tiết kỹ thuật (log của git)")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Mở quản lý tài khoản" }));
    expect(useAccountDialogs.getState().manager).toEqual({
      accountId: null,
      add: false,
      reauth: false,
    });
  });
});

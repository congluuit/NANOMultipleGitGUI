import i18n from "i18next";
import { accountLabel, bindRepo } from "@/features/accounts/switcher";
import { useAccounts } from "@/features/accounts/queries";
import { failureText } from "@/features/commits/outcome";
import { invalidateRepo } from "@/features/worktree/queries";
import {
  commands,
  errorMessage,
  type AccountSuggestion,
  type FixAuthorMode,
  type LargeFileFix,
  type Outcome,
} from "@/ipc";
import { useUi } from "@/stores/ui";
import { CommitGuardDialog } from "./CommitGuardDialog";
import { PushGuardDialog } from "./PushGuardDialog";
import { useGuardDialogs } from "./store";

const notify = (message: string, kind: "info" | "error" = "info") =>
  useUi.getState().showNotice({ kind, message });

/** Runs a rewrite command; reports rejections and errors; returns the value when done. */
async function rewriteStep<T>(call: () => Promise<Outcome<T>>): Promise<T | null> {
  try {
    const out = await call();
    if (out.status === "done") return out.value;
    if (out.status === "rejected") {
      notify(
        i18n.t("guards.push.rewriteFailed", { message: failureText(out.reason, out.message) }),
        "error",
      );
    }
  } catch (e) {
    notify(i18n.t("guards.push.rewriteFailed", { message: errorMessage(e) }), "error");
  }
  return null;
}

/** Re-runs Push Guard after a fix so the dialog shows what is left. */
async function refreshPush() {
  const push = useGuardDialogs.getState().push;
  if (!push) return;
  try {
    const out = await commands.pushGuard(push.repoId, push.remote, push.branch, push.limitBytes);
    if (out.status === "done") useGuardDialogs.getState().updatePush({ report: out.value });
  } catch (e) {
    console.warn("push guard refresh failed", e);
  }
}

async function fixAuthors(mode: FixAuthorMode) {
  const { push, setBusy, updatePush } = useGuardDialogs.getState();
  if (!push) return;
  setBusy(true);
  const out = await rewriteStep(() =>
    commands.guardFixAuthors(push.repoId, push.remote, push.branch, mode),
  );
  invalidateRepo(push.repoId);
  if (out) {
    updatePush({ rewrite: out });
    await refreshPush();
  }
  setBusy(false);
}

/**
 * "Chuyển sang LFS" / "Bỏ khỏi commit": uncommitted changes are put aside and back by the
 * backend; says so when they had to stay in the stash, and which files `.gitignore` now keeps
 * out.
 */
async function fixLargeFiles(paths: string[], mode: LargeFileFix) {
  const { push, setBusy, updatePush } = useGuardDialogs.getState();
  if (!push) return;
  setBusy(true);
  const out = await rewriteStep(() =>
    commands.guardFixLargeFiles(push.repoId, push.remote, push.branch, paths, mode),
  );
  invalidateRepo(push.repoId);
  if (out) {
    updatePush({ rewrite: out.rewrite });
    if (out.aside.kept) notify(i18n.t("largeFiles.stashKept"), "error");
    else if (out.ignored.length)
      notify(i18n.t("largeFiles.ignored", { count: out.ignored.length }));
    await refreshPush();
  }
  setBusy(false);
}

async function undo(backupRef: string) {
  const { push, setBusy, updatePush } = useGuardDialogs.getState();
  if (!push) return;
  setBusy(true);
  const out = await rewriteStep(() => commands.guardUndo(push.repoId, backupRef));
  invalidateRepo(push.repoId);
  if (out) {
    notify(i18n.t("guards.push.undone", { branch: out.branch }));
    updatePush({ rewrite: null });
    await refreshPush();
  }
  setBusy(false);
}

async function switchAccount(suggestion: AccountSuggestion) {
  const { commit, setBusy, settleCommit } = useGuardDialogs.getState();
  if (!commit) return;
  setBusy(true);
  const ok = await bindRepo(commit.repoId, {
    accountId: suggestion.accountId,
    identityId: suggestion.identityId,
  });
  if (ok) settleCommit("switched");
  else setBusy(false);
}

/** Mount once (App.tsx): renders the Commit Guard and Push Guard dialogs on demand. */
export function GuardDialogsHost() {
  const commit = useGuardDialogs((s) => s.commit);
  const push = useGuardDialogs((s) => s.push);
  const busy = useGuardDialogs((s) => s.busy);
  const settleCommit = useGuardDialogs((s) => s.settleCommit);
  const settlePush = useGuardDialogs((s) => s.settlePush);
  // Bound account's label, from the account list the switcher already caches.
  const accounts = useAccounts();
  const bound = commit?.report.accountId
    ? accounts.data?.find((a) => a.id === commit.report.accountId)
    : undefined;

  return (
    <>
      <CommitGuardDialog
        report={commit?.report ?? null}
        busy={busy}
        accountLabel={bound ? accountLabel(bound) : null}
        onSwitch={(s) => void switchAccount(s)}
        onCommitAnyway={() => settleCommit("proceed")}
        onCancel={() => settleCommit("cancel")}
      />
      <PushGuardDialog
        report={push?.report ?? null}
        rewrite={push?.rewrite ?? null}
        busy={busy}
        onFixAuthors={(mode) => void fixAuthors(mode)}
        onMigrate={(paths) => void fixLargeFiles(paths, "lfs")}
        onDrop={(paths) => void fixLargeFiles(paths, "drop")}
        onUndo={(ref) => void undo(ref)}
        onPush={() => settlePush("proceed")}
        onCancel={() => settlePush("cancel")}
      />
    </>
  );
}

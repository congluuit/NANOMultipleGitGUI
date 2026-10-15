import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { AddWorktreeDialog } from "@/features/linked-worktrees/AddWorktreeDialog";
import { useLinkedWorktrees } from "@/features/linked-worktrees/queries";
import { isPlausibleBranchName } from "@/features/repos/branchName";
import { commands, type MergeMode } from "@/ipc";
import { deleteRemoteBranch } from "./branchActions";
import { conflictText, runOutcome } from "./outcome";
import { useRefDialog, type RefDialog } from "./store";

/** Renders whichever dialog a context menu asked for. Mounted once in the shell. */
export function RefDialogs() {
  const dialog = useRefDialog((s) => s.dialog);
  const close = () => useRefDialog.getState().open(null);
  if (!dialog) return null;
  // Keyed so each request starts with fresh form state.
  const key = JSON.stringify(dialog);
  switch (dialog.kind) {
    case "createBranch":
    case "createTag":
    case "renameBranch":
      return <NameDialog key={key} dialog={dialog} onClose={close} />;
    case "merge":
      return <MergeDialog key={key} dialog={dialog} onClose={close} />;
    case "deleteRemoteBranch":
      return <DeleteRemoteDialog key={key} dialog={dialog} onClose={close} />;
    case "addWorktree":
      return <WorktreeFromBranch key={key} dialog={dialog} onClose={close} />;
    default:
      return <ConfirmRefDialog key={key} dialog={dialog} onClose={close} />;
  }
}

type NameKind = Extract<RefDialog, { kind: "createBranch" | "createTag" | "renameBranch" }>;

function NameDialog({ dialog, onClose }: { dialog: NameKind; onClose: () => void }) {
  const { t } = useTranslation();
  const [name, setName] = useState(dialog.kind === "renameBranch" ? dialog.name : "");
  const [checkout, setCheckout] = useState(true);
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);
  const annotated = dialog.kind === "createTag" && !!dialog.annotated;
  const valid = isPlausibleBranchName(name) && (!annotated || message.trim().length > 0);
  const { repoId } = dialog;

  const submit = async () => {
    if (!valid || busy) return;
    setBusy(true);
    const n = name.trim();
    let ok: unknown;
    if (dialog.kind === "createBranch") {
      ok = await runOutcome(
        repoId,
        () => commands.branchCreate(repoId, n, dialog.start, checkout),
        () => t(checkout ? "ops.branchCreatedCheckedOut" : "ops.branchCreated", { name: n }),
      );
    } else if (dialog.kind === "createTag") {
      const msg = message.trim() || null;
      ok = await runOutcome(
        repoId,
        () => commands.tagCreate(repoId, n, dialog.oid, msg),
        () => t("ops.tagCreated", { name: n }),
      );
    } else {
      ok = await runOutcome(
        repoId,
        () => commands.branchRename(repoId, dialog.name, n),
        () => t("ops.branchRenamed", { old: dialog.name, name: n }),
      );
    }
    setBusy(false);
    if (ok !== undefined) onClose();
  };

  const title =
    dialog.kind === "createBranch"
      ? t("refs.createBranchTitle", { at: dialog.label })
      : dialog.kind === "createTag"
        ? t(annotated ? "branchMenu.annotatedTagTitle" : "refs.createTagTitle", {
            at: dialog.label,
          })
        : t("refs.renameTitle", { name: dialog.name });

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>
            {dialog.kind === "createTag" ? t("refs.tagHint") : t("refs.branchHint")}
          </DialogDescription>
        </DialogHeader>
        <form
          className="space-y-2"
          onSubmit={(e) => {
            e.preventDefault();
            void submit();
          }}
        >
          <label htmlFor="ref-name" className="text-xs font-semibold">
            {dialog.kind === "createTag" ? t("refs.tagName") : t("refs.branchName")}
          </label>
          <input
            id="ref-name"
            autoFocus
            value={name}
            onChange={(e) => setName(e.target.value)}
            aria-invalid={name.length > 0 && !valid}
            className="h-8 w-full rounded-md border bg-background px-2 font-mono text-xs focus:border-primary focus:outline-none aria-invalid:border-danger"
          />
          {dialog.kind === "createBranch" && (
            <label className="flex items-center gap-1.5 text-xs">
              <input
                type="checkbox"
                checked={checkout}
                onChange={(e) => setCheckout(e.target.checked)}
                className="accent-primary"
              />
              {t("refs.checkoutAfter")}
            </label>
          )}
          {dialog.kind === "createTag" && (
            <>
              <label htmlFor="tag-message" className="text-xs font-semibold">
                {annotated ? t("branchMenu.tagMessageRequired") : t("refs.tagMessage")}
              </label>
              <textarea
                id="tag-message"
                rows={3}
                value={message}
                onChange={(e) => setMessage(e.target.value)}
                className="w-full rounded-md border bg-background px-2 py-1 text-xs focus:border-primary focus:outline-none"
              />
            </>
          )}
        </form>
        <DialogFooter>
          <Button variant="outline" size="sm" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button size="sm" disabled={!valid || busy} onClick={() => void submit()}>
            {dialog.kind === "renameBranch" ? t("refs.rename") : t("refs.create")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

const MERGE_MODES: MergeMode[] = ["fastForward", "noFastForward", "squash", "fastForwardOnly"];

function MergeDialog({
  dialog,
  onClose,
}: {
  dialog: Extract<RefDialog, { kind: "merge" }>;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [mode, setMode] = useState<MergeMode>("fastForward");
  const [busy, setBusy] = useState(false);
  const { repoId, rev } = dialog;

  const submit = async () => {
    setBusy(true);
    await runOutcome(
      repoId,
      () => commands.mergeIntoCurrent(repoId, rev, mode, null),
      (o) => conflictText(o) || t(`ops.merge.${o.result}`, { rev }),
    );
    setBusy(false);
    onClose();
  };

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{t("refs.mergeTitle", { rev, into: dialog.into ?? "HEAD" })}</DialogTitle>
          <DialogDescription>{t("refs.mergeHint")}</DialogDescription>
        </DialogHeader>
        <div className="space-y-1.5 text-xs" role="radiogroup">
          {MERGE_MODES.map((m) => (
            <label key={m} className="flex items-start gap-2">
              <input
                type="radio"
                name="merge-mode"
                checked={mode === m}
                onChange={() => setMode(m)}
                className="mt-0.5 accent-primary"
              />
              <span>
                <span className="font-medium">{t(`refs.mergeMode.${m}`)}</span>
                <span className="block text-muted-foreground">{t(`refs.mergeModeHint.${m}`)}</span>
              </span>
            </label>
          ))}
        </div>
        <DialogFooter>
          <Button variant="outline" size="sm" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button size="sm" disabled={busy} onClick={() => void submit()} autoFocus>
            {t("refs.merge")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

type ConfirmKind = Extract<RefDialog, { kind: "resetHard" | "deleteBranch" | "deleteTag" }>;

function ConfirmRefDialog({ dialog, onClose }: { dialog: ConfirmKind; onClose: () => void }) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  const { repoId } = dialog;

  const confirm = async () => {
    setBusy(true);
    if (dialog.kind === "resetHard") {
      await runOutcome(
        repoId,
        () => commands.resetTo(repoId, dialog.oid, "hard"),
        () => t("ops.resetDone", { oid: dialog.label, mode: "hard" }),
      );
      onClose();
    } else if (dialog.kind === "deleteTag") {
      await runOutcome(
        repoId,
        () => commands.tagDelete(repoId, dialog.name),
        () => t("ops.tagDeleted", { name: dialog.name }),
      );
      onClose();
    } else {
      const out = await commands.branchDelete(repoId, dialog.name, dialog.force).catch(() => null);
      if (out?.status === "rejected" && out.reason === "notFullyMerged" && !dialog.force) {
        // Offer the forced delete instead of failing.
        setBusy(false);
        useRefDialog.getState().open({ ...dialog, force: true });
        return;
      }
      await runOutcome(
        repoId,
        () =>
          out ? Promise.resolve(out) : commands.branchDelete(repoId, dialog.name, dialog.force),
        () => t("ops.branchDeleted", { name: dialog.name }),
      );
      onClose();
    }
    setBusy(false);
  };

  const [title, body, action] =
    dialog.kind === "resetHard"
      ? [
          t("refs.resetHardTitle", { branch: dialog.branch ?? "HEAD", at: dialog.label }),
          t("refs.resetHardBody"),
          t("refs.resetHard"),
        ]
      : dialog.kind === "deleteTag"
        ? [
            t("refs.deleteTagTitle", { name: dialog.name }),
            t("refs.deleteTagBody"),
            t("refs.delete"),
          ]
        : dialog.force
          ? [
              t("refs.forceDeleteTitle", { name: dialog.name }),
              t("refs.forceDeleteBody"),
              t("refs.forceDelete"),
            ]
          : [
              t("refs.deleteBranchTitle", { name: dialog.name }),
              t("refs.deleteBranchBody"),
              t("refs.delete"),
            ];

  return (
    <ConfirmDialog
      open
      title={title}
      description={body}
      confirmLabel={action}
      destructive
      busy={busy}
      onConfirm={() => void confirm()}
      onCancel={onClose}
    />
  );
}

function DeleteRemoteDialog({
  dialog,
  onClose,
}: {
  dialog: Extract<RefDialog, { kind: "deleteRemoteBranch" }>;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  const { repoId, remote, branch, local } = dialog;
  const full = `${remote}/${branch}`;

  const confirm = async () => {
    setBusy(true);
    if (local) {
      const ok = await runOutcome(
        repoId,
        () => commands.branchDelete(repoId, local, true),
        () => null,
      );
      if (ok === undefined) {
        setBusy(false);
        return;
      }
    }
    onClose();
    await deleteRemoteBranch(repoId, remote, branch);
  };

  return (
    <ConfirmDialog
      open
      title={
        local
          ? t("branchMenu.deleteBothTitle", { name: local, remote: full })
          : t("branchMenu.deleteRemoteTitle", { name: full })
      }
      description={t("branchMenu.deleteRemoteBody", { name: full, remote })}
      confirmLabel={t("refs.delete")}
      destructive
      busy={busy}
      onConfirm={() => void confirm()}
      onCancel={onClose}
    />
  );
}

function WorktreeFromBranch({
  dialog,
  onClose,
}: {
  dialog: Extract<RefDialog, { kind: "addWorktree" }>;
  onClose: () => void;
}) {
  const list = useLinkedWorktrees(dialog.repoId);
  if (!list.data) return null;
  return (
    <AddWorktreeDialog
      repoId={dialog.repoId}
      worktrees={list.data}
      open
      initialBranch={dialog.branch}
      onClose={onClose}
    />
  );
}

/** Phase 6.1: questions of the safe branch switch (uncommitted changes, detached HEAD). */
import { useState } from "react";
import { ArchiveRestore, ArrowRightLeft, GitBranchPlus, TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import type { SwitchDirtyMode } from "@/ipc";
import { useSettings } from "@/stores/settings";
import { createBranchHere, runSwitch, switchBranch } from "./actions";
import { targetLabel } from "./model";
import { savePrefs } from "./prefs";
import { useBranchSwitchUi, type ConfirmSwitch, type PendingSwitch } from "./store";

/** Mounted once (App). */
export function BranchSwitchDialogHost() {
  const pending = useBranchSwitchUi((s) => s.pending);
  const close = useBranchSwitchUi((s) => s.close);
  const confirm = useBranchSwitchUi((s) => s.confirm);
  const closeConfirm = useBranchSwitchUi((s) => s.closeConfirm);
  if (confirm) {
    return (
      <ConfirmSwitchDialog
        key={`confirm-${confirm.repoId}-${targetLabel(confirm.target)}`}
        confirm={confirm}
        onClose={closeConfirm}
      />
    );
  }
  if (!pending) return null;
  return pending.kind === "dirty" ? (
    <DirtyDialog
      key={`dirty-${pending.repoId}-${pending.check.branch}`}
      pending={pending}
      onClose={close}
    />
  ) : (
    <DetachedDialog
      key={`detached-${pending.repoId}-${pending.check.branch}`}
      pending={pending}
      onClose={close}
    />
  );
}

function ConfirmSwitchDialog({
  confirm,
  onClose,
}: {
  confirm: ConfirmSwitch;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [dontAsk, setDontAsk] = useState(false);
  const { repoId, target } = confirm;
  const branch = targetLabel(target);

  const go = () => {
    onClose();
    if (dontAsk) useSettings.getState().setConfirmBranchSwitch(false);
    void switchBranch(repoId, target);
  };

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <ArrowRightLeft className="size-4 text-primary" />
            {t("branchSwitch.confirm.title", { branch })}
          </DialogTitle>
          <DialogDescription className="break-words">
            {t(
              target.kind === "local"
                ? "branchSwitch.confirm.description"
                : "branchSwitch.confirm.descriptionRemote",
              { branch },
            )}
          </DialogDescription>
        </DialogHeader>

        <label className="flex items-center gap-2 text-xs">
          <input
            type="checkbox"
            checked={dontAsk}
            onChange={(e) => setDontAsk(e.target.checked)}
            className="accent-primary"
          />
          {t("branchSwitch.confirm.dontAsk")}
        </label>

        <DialogFooter>
          <Button variant="outline" size="sm" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button size="sm" autoFocus onClick={go}>
            {t("branchSwitch.confirm.switch")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function DirtyDialog({ pending, onClose }: { pending: PendingSwitch; onClose: () => void }) {
  const { t } = useTranslation();
  const [remember, setRemember] = useState(false);
  const { repoId, target, check } = pending;
  const from = check.current;

  const choose = (mode: SwitchDirtyMode) => {
    onClose();
    if (remember) void savePrefs({ dirtyAction: mode });
    void runSwitch(repoId, target, mode);
  };

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>
            {from
              ? t("branchSwitch.dialog.title", { count: check.changes, branch: from })
              : t("branchSwitch.dialog.titleDetached", { count: check.changes })}
          </DialogTitle>
          <DialogDescription>
            {t("branchSwitch.dialog.description", { target: check.branch })}
          </DialogDescription>
        </DialogHeader>

        <div className="grid gap-2">
          <OptionButton
            autoFocus
            icon={<ArchiveRestore className="size-4 text-primary" />}
            title={t("branchSwitch.dialog.stash")}
            badge={t("branchSwitch.dialog.recommended")}
            hint={t("branchSwitch.dialog.stashHint", { branch: from ?? "HEAD" })}
            onClick={() => choose("stash")}
          />
          <OptionButton
            icon={<ArrowRightLeft className="size-4 text-muted-foreground" />}
            title={t("branchSwitch.dialog.carry")}
            hint={t("branchSwitch.dialog.carryHint", { target: check.branch })}
            onClick={() => choose("carry")}
          />
        </div>

        <label className="flex items-center gap-2 text-xs">
          <input
            type="checkbox"
            checked={remember}
            onChange={(e) => setRemember(e.target.checked)}
            className="accent-primary"
          />
          {t("branchSwitch.dialog.remember")}
        </label>

        <DialogFooter>
          <Button variant="outline" size="sm" onClick={onClose}>
            {t("common.cancel")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function OptionButton({
  icon,
  title,
  hint,
  badge,
  autoFocus,
  onClick,
}: {
  icon: React.ReactNode;
  title: string;
  hint: string;
  badge?: string;
  autoFocus?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      autoFocus={autoFocus}
      onClick={onClick}
      className="flex items-start gap-3 rounded-md border p-3 text-left text-xs hover:border-primary hover:bg-surface-2 focus-visible:border-primary focus-visible:outline-none"
    >
      <span className="mt-0.5 shrink-0">{icon}</span>
      <span className="grid gap-0.5">
        <span className="flex items-center gap-2 font-medium">
          {title}
          {badge && (
            <span className="rounded bg-primary/10 px-1.5 py-px text-[10px] font-normal text-primary">
              {badge}
            </span>
          )}
        </span>
        <span className="text-[11px] text-muted-foreground">{hint}</span>
      </span>
    </button>
  );
}

function DetachedDialog({ pending, onClose }: { pending: PendingSwitch; onClose: () => void }) {
  const { t } = useTranslation();
  const { repoId, target, check } = pending;
  const commit = check.headShort ?? "HEAD";
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <TriangleAlert className="size-4 text-warning" />
            {t("branchSwitch.detached.title", { commit })}
          </DialogTitle>
          <DialogDescription>
            {t("branchSwitch.detached.description", { commit, target: check.branch })}
          </DialogDescription>
        </DialogHeader>
        <DialogFooter className="gap-2 sm:justify-between">
          <Button variant="outline" size="sm" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <div className="flex flex-wrap gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={() => {
                onClose();
                void switchBranch(repoId, target, { detachedConfirmed: true });
              }}
            >
              {t("branchSwitch.detached.switchAnyway")}
            </Button>
            <Button
              size="sm"
              autoFocus
              onClick={() => {
                onClose();
                createBranchHere(repoId, check);
              }}
            >
              <GitBranchPlus />
              {t("branchSwitch.detached.createBranch")}
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

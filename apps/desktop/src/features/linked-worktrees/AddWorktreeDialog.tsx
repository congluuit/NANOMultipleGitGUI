import { useState } from "react";
import { FolderOpen, LoaderCircle } from "lucide-react";
import { isTauri } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
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
import type { Worktree, WorktreeTarget } from "@/ipc";
import { useRepoRefs } from "@/features/repos/queries";
import { addWorktree } from "./actions";
import { checkedOutBranches, folderName, suggestFolder } from "./model";

const inputClass =
  "h-8 w-full rounded-md border bg-background px-2 text-xs focus:border-primary focus:outline-none";

type Mode = "branch" | "newBranch";

/** "Add worktree…": a branch (existing or new) checked out in another folder. */
export function AddWorktreeDialog({
  repoId,
  worktrees,
  open,
  initialBranch,
  onClose,
}: {
  repoId: number;
  worktrees: readonly Worktree[];
  open: boolean;
  initialBranch?: string;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const refs = useRepoRefs(repoId);
  const used = checkedOutBranches(worktrees);
  const free = (refs.data?.local ?? []).map((b) => b.name).filter((n) => !used.has(n));
  const main = worktrees.find((w) => w.isMain)?.path ?? "";
  const [mode, setMode] = useState<Mode>(initialBranch ? "branch" : "newBranch");
  const [branch, setBranch] = useState(initialBranch ?? "");
  const [newName, setNewName] = useState("");
  const [start, setStart] = useState("");
  const [folder, setFolder] = useState<string | null>(null);
  const [openAfter, setOpenAfter] = useState(true);
  const [busy, setBusy] = useState(false);

  const chosen = mode === "branch" ? branch || free[0] || "" : newName.trim();
  const effectiveFolder = folder ?? (main && chosen ? suggestFolder(main, chosen) : "");
  const valid = chosen.length > 0 && effectiveFolder.trim().length > 0;

  const browse = async () => {
    if (!isTauri()) return;
    const dir = await openDialog({ directory: true, multiple: false, title: t("worktrees.pick") });
    if (typeof dir === "string") {
      const sep = dir.includes("\\") ? "\\" : "/";
      setFolder(`${dir.replace(/[\\/]+$/, "")}${sep}${folderName(suggestFolder(main, chosen))}`);
    }
  };

  const submit = async () => {
    const target: WorktreeTarget =
      mode === "branch"
        ? { kind: "branch", name: chosen }
        : { kind: "newBranch", name: chosen, start: start.trim() || null };
    setBusy(true);
    const ok = await addWorktree(repoId, effectiveFolder.trim(), target, openAfter);
    setBusy(false);
    if (ok) onClose();
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-lg">
        <form
          className="grid gap-3"
          onSubmit={(e) => {
            e.preventDefault();
            if (valid && !busy) void submit();
          }}
        >
          <DialogHeader>
            <DialogTitle>{t("worktrees.addTitle")}</DialogTitle>
            <DialogDescription>{t("worktrees.addDesc")}</DialogDescription>
          </DialogHeader>
          <div className="flex gap-4 text-xs">
            {(["newBranch", "branch"] as const).map((m) => (
              <label key={m} className="flex items-center gap-1.5">
                <input
                  type="radio"
                  name="worktree-mode"
                  className="accent-primary"
                  checked={mode === m}
                  onChange={() => setMode(m)}
                />
                {t(`worktrees.mode.${m}`)}
              </label>
            ))}
          </div>
          {mode === "branch" ? (
            <label className="grid gap-1 text-xs">
              <span className="text-muted-foreground">{t("worktrees.branch")}</span>
              <select
                className={inputClass}
                value={chosen}
                onChange={(e) => setBranch(e.target.value)}
              >
                {free.length === 0 && <option value="">{t("worktrees.noFreeBranch")}</option>}
                {free.map((b) => (
                  <option key={b} value={b}>
                    {b}
                  </option>
                ))}
              </select>
              {used.size > 0 && (
                <span className="text-[11px] text-muted-foreground">{t("worktrees.usedHint")}</span>
              )}
            </label>
          ) : (
            <div className="grid grid-cols-2 gap-2">
              <label className="grid gap-1 text-xs">
                <span className="text-muted-foreground">{t("worktrees.newName")}</span>
                <input
                  className={inputClass}
                  value={newName}
                  onChange={(e) => setNewName(e.target.value)}
                  placeholder="feature/x"
                  spellCheck={false}
                  autoFocus
                />
              </label>
              <label className="grid gap-1 text-xs">
                <span className="text-muted-foreground">{t("worktrees.start")}</span>
                <input
                  className={inputClass}
                  value={start}
                  onChange={(e) => setStart(e.target.value)}
                  placeholder="HEAD"
                  spellCheck={false}
                />
              </label>
            </div>
          )}
          <label className="grid gap-1 text-xs">
            <span className="text-muted-foreground">{t("worktrees.folder")}</span>
            <div className="flex gap-1">
              <input
                className={inputClass}
                value={effectiveFolder}
                onChange={(e) => setFolder(e.target.value)}
                spellCheck={false}
              />
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={() => void browse()}
                title={t("worktrees.pick")}
              >
                <FolderOpen />
              </Button>
            </div>
            <span className="text-[11px] text-muted-foreground">{t("worktrees.folderHint")}</span>
          </label>
          <label className="flex items-center gap-2 text-xs">
            <input
              type="checkbox"
              className="accent-primary"
              checked={openAfter}
              onChange={(e) => setOpenAfter(e.target.checked)}
            />
            {t("worktrees.openAfter")}
          </label>
          <DialogFooter>
            <Button type="button" variant="outline" size="sm" onClick={onClose}>
              {t("common.cancel")}
            </Button>
            <Button type="submit" size="sm" disabled={!valid || busy}>
              {busy && <LoaderCircle className="animate-spin" />}
              {t("worktrees.addSubmit")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

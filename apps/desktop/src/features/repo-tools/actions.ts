import { open as openFileDialog, save as saveFileDialog } from "@tauri-apps/plugin-dialog";
import i18n from "i18next";
import { conflictText, runOutcome } from "@/features/commits/outcome";
import { commands, type ApplyMode, type BisectMark, type PatchSource } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { toolKeys } from "./queries";
import { openTool } from "./store";

const short = (oid: string) => oid.slice(0, 7);

const PATCH_FILTERS = [{ name: "Patch", extensions: ["patch", "diff"] }];

/** File name for a patch: letters, digits and dashes only (accents are dropped). */
export function patchFileName(subject: string, fallback: string): string {
  const slug = subject
    // The Vietnamese d with a stroke has no accent to drop: map it by hand.
    .replace(/đ/g, "d")
    .replace(/Đ/g, "D")
    .normalize("NFKD")
    .replace(/\p{M}+/gu, "")
    .replace(/[^A-Za-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 50)
    .replace(/-+$/, "");
  return `${slug || fallback}.patch`;
}

/** "Save as patch" of a commit: asks where, then writes the file. */
export async function saveCommitPatch(repoId: number, oid: string, subject: string) {
  const dest = await saveFileDialog({
    title: i18n.t("patch.saveTitle"),
    defaultPath: patchFileName(subject, short(oid)),
    filters: PATCH_FILTERS,
  });
  if (!dest) return;
  await runOutcome(
    repoId,
    () => commands.patchSaveCommit(repoId, oid, dest),
    () => i18n.t("patch.saved", { file: dest }),
  );
}

/** Saves the uncommitted (or only the staged) changes as a patch file. */
export async function saveChangesPatch(repoId: number, source: PatchSource) {
  const dest = await saveFileDialog({
    title: i18n.t("patch.saveTitle"),
    defaultPath: "changes.patch",
    filters: PATCH_FILTERS,
  });
  if (!dest) return;
  await runOutcome(
    repoId,
    () => commands.patchSaveChanges(repoId, source, dest),
    () => i18n.t("patch.saved", { file: dest }),
  );
}

/** "Apply patch": asks for the file, then shows what is inside before anything changes. */
export async function pickPatchToApply(repoId: number) {
  const file = await openFileDialog({
    title: i18n.t("patch.pickTitle"),
    multiple: false,
    filters: [...PATCH_FILTERS, { name: "*", extensions: ["*"] }],
  });
  if (typeof file === "string") openTool({ kind: "patchApply", repoId, file });
}

export async function applyPatch(repoId: number, file: string, mode: ApplyMode): Promise<boolean> {
  const out = await runOutcome(
    repoId,
    () => commands.patchApply(repoId, file, mode),
    (o) => conflictText(o) || i18n.t(mode === "commits" ? "patch.appliedCommits" : "patch.applied"),
  );
  return out !== undefined;
}

const refreshBisect = (repoId: number) =>
  queryClient.invalidateQueries({ queryKey: toolKeys.bisect(repoId) });

export async function startBisect(repoId: number, bad: string, good: string): Promise<boolean> {
  const st = await runOutcome(
    repoId,
    () => commands.bisectStart(repoId, bad, good),
    (s) => i18n.t("bisect.started", { count: s.remaining }),
  );
  await refreshBisect(repoId);
  return st !== undefined;
}

export async function markBisect(repoId: number, mark: BisectMark) {
  await runOutcome(
    repoId,
    () => commands.bisectMark(repoId, mark),
    (s) =>
      s.firstBad
        ? i18n.t("bisect.found", { oid: short(s.firstBad), subject: s.firstBadSubject ?? "" })
        : null,
  );
  await refreshBisect(repoId);
}

export async function stopBisect(repoId: number) {
  await runOutcome(
    repoId,
    () => commands.bisectReset(repoId),
    () => i18n.t("bisect.stopped"),
  );
  await refreshBisect(repoId);
}

export async function setSparseFolders(repoId: number, folders: string[]): Promise<boolean> {
  const st = await runOutcome(
    repoId,
    () => commands.sparseSet(repoId, folders),
    (s) => i18n.t("sparse.applied", { count: s.folders.length }),
  );
  await queryClient.invalidateQueries({ queryKey: toolKeys.sparse(repoId) });
  return st !== undefined;
}

export async function disableSparse(repoId: number): Promise<boolean> {
  const st = await runOutcome(
    repoId,
    () => commands.sparseDisable(repoId),
    () => i18n.t("sparse.disabled"),
  );
  await queryClient.invalidateQueries({ queryKey: toolKeys.sparse(repoId) });
  return st !== undefined;
}

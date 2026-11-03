/** Creating, renaming and deleting files and folders from the app. */
import { bufferKey, dropBuffer, useDirtyBuffers } from "@/features/editor/store";
import { openInEditor } from "@/features/editor/open";
import { invalidateWorktree } from "@/features/worktree/queries";
import { commands, errorMessage } from "@/ipc";
import i18n from "@/i18n";
import { useUi } from "@/stores/ui";
import { useWorktreeStore } from "@/stores/worktree";
import { useExpandedFolders } from "./store";
import { isUnder } from "./tree";

async function run(repoId: number, op: () => Promise<unknown>): Promise<boolean> {
  try {
    await op();
    return true;
  } catch (e) {
    useUi.getState().showNotice({ kind: "error", message: errorMessage(e), repoId });
    return false;
  } finally {
    invalidateWorktree(repoId);
  }
}

/** Files under `path` (or `path` itself) with unsaved edits in the editor. */
function unsavedUnder(repoId: number, path: string): string[] {
  const prefix = bufferKey(repoId, "");
  return Object.keys(useDirtyBuffers.getState().dirty)
    .filter((k) => k.startsWith(prefix))
    .map((k) => k.slice(prefix.length))
    .filter((p) => isUnder(p, path));
}

/** The open diff / editor shows `path` or something inside it. */
function openUnder(repoId: number, path: string): string | null {
  const open = useWorktreeStore.getState().diff[repoId];
  return open && isUnder(open.path, path) ? open.path : null;
}

/** Normalizes what the user typed into a repository-relative path (`""` when unusable). */
export function cleanPath(input: string): string {
  return input
    .trim()
    .replaceAll("\\", "/")
    .split("/")
    .filter((part) => part !== "" && part !== ".")
    .join("/");
}

export const joinPath = (folder: string, name: string) =>
  cleanPath(folder ? `${folder}/${name}` : name);

export async function createFile(repoId: number, path: string): Promise<boolean> {
  if (!(await run(repoId, () => commands.fileCreate(repoId, path)))) return false;
  useExpandedFolders.getState().reveal(repoId, path);
  openInEditor(repoId, path, { untracked: true });
  return true;
}

export async function createFolder(repoId: number, path: string): Promise<boolean> {
  if (!(await run(repoId, () => commands.folderCreate(repoId, path)))) return false;
  useExpandedFolders.getState().reveal(repoId, `${path}/x`);
  return true;
}

export async function renamePath(repoId: number, from: string, to: string): Promise<boolean> {
  if (unsavedUnder(repoId, from).length > 0) {
    useUi.getState().showNotice({
      kind: "error",
      message: i18n.t("files.saveBeforeRename"),
      repoId,
    });
    return false;
  }
  const shown = openUnder(repoId, from);
  if (!(await run(repoId, () => commands.pathRename(repoId, from, to)))) return false;
  useExpandedFolders.getState().reveal(repoId, to);
  if (shown) openInEditor(repoId, to + shown.slice(from.length));
  return true;
}

export async function deletePath(repoId: number, path: string): Promise<boolean> {
  if (!(await run(repoId, () => commands.pathDelete(repoId, path)))) return false;
  for (const p of unsavedUnder(repoId, path)) dropBuffer(repoId, p);
  if (openUnder(repoId, path)) useWorktreeStore.getState().openDiff(repoId, null);
  return true;
}

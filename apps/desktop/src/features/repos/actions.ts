import { isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { errorMessage, isAppError } from "@/ipc";
import { useUi } from "@/stores/ui";
import { useWorkspace } from "@/stores/workspace";
import { queryClient } from "@/lib/queryClient";

async function pickFolder(title: string): Promise<string | null> {
  if (!isTauri()) return null;
  const dir = await open({ directory: true, multiple: false, title });
  return typeof dir === "string" ? dir : null;
}

/** Opens `path`; a folder that is not a repository leads to the "create repository here?" dialog. */
export async function openRepository(path: string): Promise<void> {
  try {
    await useWorkspace.getState().open(path);
    void queryClient.invalidateQueries({ queryKey: ["recent"] });
  } catch (e) {
    if (isAppError(e) && e.kind === "notARepository") {
      useUi.getState().requestInit({ path, reason: "notRepository" });
    } else {
      useUi.getState().showNotice({ kind: "error", message: errorMessage(e) });
    }
  }
}

export async function openRepositoryWithDialog(title: string): Promise<void> {
  const path = await pickFolder(title);
  if (path) await openRepository(path);
}

export async function newRepositoryWithDialog(title: string): Promise<void> {
  const path = await pickFolder(title);
  if (path) useUi.getState().requestInit({ path, reason: "new" });
}

export async function initRepository(path: string, initialBranch: string): Promise<boolean> {
  try {
    await useWorkspace.getState().init(path, initialBranch);
    void queryClient.invalidateQueries({ queryKey: ["recent"] });
    return true;
  } catch (e) {
    useUi.getState().showNotice({ kind: "error", message: errorMessage(e) });
    return false;
  }
}

export async function closeRepository(id: number): Promise<void> {
  try {
    await useWorkspace.getState().close(id);
  } catch (e) {
    useUi.getState().showNotice({ kind: "error", message: errorMessage(e) });
  }
}

export async function forgetRepository(id: number): Promise<void> {
  try {
    await useWorkspace.getState().forget(id);
    void queryClient.invalidateQueries({ queryKey: ["recent"] });
  } catch (e) {
    useUi.getState().showNotice({ kind: "error", message: errorMessage(e) });
  }
}

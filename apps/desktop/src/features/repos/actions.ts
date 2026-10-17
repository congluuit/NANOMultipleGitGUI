import { isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { errorMessage, isAppError } from "@/ipc";
import { isPinned, displayedTabs, usePinnedTabs } from "@/features/shell/pinnedTabs";
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
      useUi.getState().showNotice({
        kind: "error",
        message: errorMessage(e),
        retry: () => void openRepository(path),
      });
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
    usePinnedTabs.getState().unpin(id);
  } catch (e) {
    useUi.getState().showNotice({ kind: "error", message: errorMessage(e) });
  }
}

/** Closes several tabs one after the other (the first failure is reported, the rest still close). */
export async function closeRepositories(ids: number[]): Promise<void> {
  for (const id of ids) await closeRepository(id);
}

/** Tab context menu: every tab but `keepId` (pinned tabs stay). */
export function closeOtherRepositories(keepId: number): Promise<void> {
  const ids = useWorkspace
    .getState()
    .tabs.filter((t) => t.id !== keepId && !isPinned(t.id))
    .map((t) => t.id);
  return closeRepositories(ids);
}

/** Tab context menu: the unpinned tabs after `id` in strip order. */
export function closeRepositoriesToTheRight(id: number): Promise<void> {
  const tabs = displayedTabs(useWorkspace.getState().tabs);
  const index = tabs.findIndex((t) => t.id === id);
  return closeRepositories(
    index < 0
      ? []
      : tabs
          .slice(index + 1)
          .filter((t) => !isPinned(t.id))
          .map((t) => t.id),
  );
}

/** Every unpinned tab. */
export function closeAllRepositories(): Promise<void> {
  return closeRepositories(
    useWorkspace
      .getState()
      .tabs.filter((t) => !isPinned(t.id))
      .map((t) => t.id),
  );
}

/** `Ctrl/Cmd+Shift+T`: opens the tab closed most recently in this session. */
export async function reopenLastClosed(): Promise<void> {
  const last = useWorkspace.getState().recentlyClosed[0];
  if (last) await openRepository(last.path);
}

export async function forgetRepository(id: number): Promise<void> {
  try {
    await useWorkspace.getState().forget(id);
    void queryClient.invalidateQueries({ queryKey: ["recent"] });
  } catch (e) {
    useUi.getState().showNotice({ kind: "error", message: errorMessage(e) });
  }
}

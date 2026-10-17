import { useState } from "react";
import i18n from "i18next";
import { relativeTime } from "@/features/blame/blame";
import { useRepoRefs } from "@/features/repos/queries";
import { useWorktreeStatus } from "@/features/worktree/queries";
import type { Stash, WorkingTreeStatus } from "@/ipc";
import { useActiveRepo } from "@/stores/workspace";
import { applyStash, stashName } from "./actions";
import { useStashUi } from "./store";

/** The working tree has something `git stash` could save. */
export function hasLocalChanges(status: WorkingTreeStatus | undefined): boolean {
  return !!status && status.staged.length + status.unstaged.length + status.conflicted.length > 0;
}

/** `stash@{0}`, the newest entry. */
export function topStash(stashes: Stash[] | undefined): Stash | null {
  return stashes?.find((s) => s.index === 0) ?? null;
}

/**
 * What the toolbar "Pop" button does:
 * - `none`: nothing to pop (disabled);
 * - `single`: one stash, made on this branch → one click pops it (green);
 * - `otherBranch`: one stash, made on another branch → opens the picker so the branch
 *   difference is seen first (amber);
 * - `many`: several stashes → opens the picker to choose which one (blue, with the count).
 */
export type PopMode = "none" | "single" | "otherBranch" | "many";

export function popMode(stashes: Stash[] | undefined, currentBranch: string | null): PopMode {
  const count = stashes?.length ?? 0;
  if (count === 0) return "none";
  if (count > 1) return "many";
  const only = stashes?.[0];
  return only?.branch && currentBranch && only.branch !== currentBranch ? "otherBranch" : "single";
}

export const POP_TONE = {
  single: "success",
  otherBranch: "warning",
  many: "info",
} as const;

/** Entry selected when the picker opens: the newest one made on this branch, else the newest. */
export function defaultStash(stashes: Stash[], currentBranch: string | null): number | null {
  const sorted = [...stashes].sort((a, b) => a.index - b.index);
  return (sorted.find((s) => s.branch === currentBranch) ?? sorted[0])?.index ?? null;
}

/** Stash message without git's `On <branch>: ` / `WIP on <branch>: ` prefix. */
export function stashTitle(stash: Pick<Stash, "message" | "branch">): string {
  const m = /^(WIP on|On) [^:]+: (.*)$/s.exec(stash.message);
  const [, prefix, rest = ""] = m ?? [];
  if (!m) return stash.message;
  return prefix === "WIP on" ? `WIP · ${rest}` : rest;
}

/** "main · 5 phút trước" (what is known of the stash). */
export function stashMeta(stash: Stash, nowSeconds = Date.now() / 1000): string {
  const parts: string[] = [];
  if (stash.branch) parts.push(stash.branch);
  if (stash.time > 0) parts.push(relativeTime(stash.time, nowSeconds, i18n.language));
  return parts.join(" · ");
}

/** Tooltip of the Pop button. */
export function popTooltip(
  mode: PopMode,
  stashes: Stash[] | undefined,
  currentBranch: string | null,
): string {
  const top = topStash(stashes) ?? stashes?.[0] ?? null;
  switch (mode) {
    case "none":
      return i18n.t("stash.noStashes");
    case "single":
      return i18n.t("stash.popSingle", {
        name: stashName(top!.index),
        message: stashTitle(top!),
        meta: stashMeta(top!),
      });
    case "otherBranch":
      return i18n.t("stash.popOtherBranch", {
        branch: top!.branch,
        current: currentBranch,
        message: stashTitle(top!),
      });
    case "many":
      return i18n.t("stash.popMany", { count: stashes!.length });
  }
}

/** State and handlers of the toolbar Stash / Pop buttons for the active repository. */
export function useStashToolbar() {
  const repo = useActiveRepo();
  const available = !!repo && !repo.missing;
  const repoId = repo?.id ?? null;
  const status = useWorktreeStatus(repoId ?? -1, available);
  const refs = useRepoRefs(repoId, available);
  const openCreate = useStashUi((s) => s.openCreate);
  const openPicker = useStashUi((s) => s.openPicker);
  const [popping, setPopping] = useState(false);
  const stashes = available ? refs.data?.stashes : undefined;
  const branch = refs.data?.head.branch ?? null;
  const top = topStash(stashes);
  const mode = popMode(stashes, branch);

  return {
    top,
    stashes: stashes ?? [],
    count: stashes?.length ?? 0,
    mode,
    tone: mode === "none" ? null : POP_TONE[mode],
    tooltip: popTooltip(mode, stashes, branch),
    canStash: available && hasLocalChanges(status.data),
    canPop: available && mode !== "none" && !popping,
    stash: () => {
      if (repoId !== null) openCreate(repoId);
    },
    /** One stash on this branch: pop it. Otherwise: let the user choose (picker). */
    pop: async () => {
      if (repoId === null || mode === "none") return;
      if (mode !== "single" || !top) {
        openPicker(repoId);
        return;
      }
      setPopping(true);
      try {
        await applyStash(repoId, top.index, true);
      } finally {
        setPopping(false);
      }
    },
    /** Pop a given entry (the ▾ menu). */
    popIndex: async (index: number) => {
      if (repoId === null) return;
      setPopping(true);
      try {
        await applyStash(repoId, index, true);
      } finally {
        setPopping(false);
      }
    },
    openPicker: () => {
      if (repoId !== null) openPicker(repoId);
    },
  };
}

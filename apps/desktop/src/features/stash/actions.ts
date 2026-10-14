import i18n from "i18next";
import { commands, errorMessage, type StashApplyOutcome, type StashOptions } from "@/ipc";
import { invalidateRepo } from "@/features/worktree/queries";
import { useUi } from "@/stores/ui";

/** `stash@{n}` as git and the stash list show it. */
export const stashName = (index: number) => `stash@{${index}}`;

/** Values of the "Stash changes" dialog. */
export interface StashForm {
  message: string;
  includeUntracked: boolean;
  keepIndex: boolean;
}

export const DEFAULT_STASH_FORM: StashForm = {
  message: "",
  includeUntracked: true,
  keepIndex: false,
};

export function buildStashOptions(form: StashForm): StashOptions {
  const message = form.message.trim();
  return {
    message: message || null,
    includeUntracked: form.includeUntracked,
    keepIndex: form.keepIndex,
  };
}

/** `a, b, c, …` — the first `max` paths of a list, for a one-line notice. */
export function shortList(paths: string[], max = 3): string {
  const head = paths.slice(0, max).join(", ");
  return paths.length > max ? `${head}, …` : head;
}

/** Notice text for the result of an apply / pop. */
export function applyOutcomeText(index: number, outcome: StashApplyOutcome, pop: boolean): string {
  const name = stashName(index);
  if (outcome.conflicts.length > 0) {
    return i18n.t("stash.conflicts", {
      name,
      count: outcome.conflicts.length,
      files: shortList(outcome.conflicts),
    });
  }
  return i18n.t(pop && outcome.dropped ? "stash.popped" : "stash.applied", { name });
}

function notify(kind: "info" | "error", message: string) {
  useUi.getState().showNotice({ kind, message });
}

/** `git stash push`. "nothing" = no local changes to save (git still exits 0). */
export async function createStash(
  repoId: number,
  form: StashForm,
): Promise<"created" | "nothing" | "failed"> {
  try {
    const result = await commands.stashCreate(repoId, buildStashOptions(form));
    notify(
      "info",
      result.created ? i18n.t("stash.created", { name: stashName(0) }) : i18n.t("stash.nothing"),
    );
    return result.created ? "created" : "nothing";
  } catch (e) {
    notify("error", errorMessage(e));
    return "failed";
  } finally {
    invalidateRepo(repoId);
  }
}

/** Apply (or pop) `stash@{index}`, restoring the index too when possible. */
export async function applyStash(repoId: number, index: number, pop: boolean): Promise<void> {
  try {
    const outcome = pop
      ? await commands.stashPop(repoId, index, true)
      : await commands.stashApply(repoId, index, true);
    notify(outcome.conflicts.length > 0 ? "error" : "info", applyOutcomeText(index, outcome, pop));
  } catch (e) {
    notify("error", errorMessage(e));
  } finally {
    invalidateRepo(repoId);
  }
}

export async function dropStash(repoId: number, index: number): Promise<void> {
  try {
    await commands.stashDrop(repoId, index);
    notify("info", i18n.t("stash.dropped", { name: stashName(index) }));
  } catch (e) {
    notify("error", errorMessage(e));
  } finally {
    invalidateRepo(repoId);
  }
}

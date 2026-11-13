/**
 * "Hỏi AI" can do more than explain: the app lists what it can run for the error (the notice's
 * own fixes plus a few safe everyday actions), the model names the ones that fit on a last
 * `ACTIONS: id1, id2` line, and the chat shows them as buttons. The model only picks ids from
 * the list; what each button does is decided here, never by the model.
 */
import i18n from "i18next";
import { useAccountDialogs } from "@/features/accounts/dialogs";
import { fetchAll, pull, push } from "@/features/command-palette/actions";
import { useLfsManager } from "@/features/lfs/managerStore";
import { repoKeys } from "@/features/repos/queries";
import { useStashUi } from "@/features/stash/store";
import type { RepoRefs } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import type { Notice, NoticeAction } from "@/stores/ui";

export interface HelpAction {
  /** Short id the model answers with. */
  id: string;
  /** Button text (app language). */
  label: string;
  /** What it does, for the model (English). */
  does: string;
  run: () => void;
}

/** The last line of an answer naming actions (see the Rust `ACTIONS_PREFIX`). */
const ACTIONS_LINE = /^[\s>*`_-]*actions[\s*`_]*:\s*(.*?)[\s*`_.]*$/i;
export const MAX_SUGGESTED = 3;

/**
 * Splits the `ACTIONS:` line off an answer: the text to show and the ids it names (unique, at
 * most {@link MAX_SUGGESTED}). Only the last few lines are looked at, so a line in the middle
 * of the explanation is left alone.
 */
export function splitActions(answer: string): { text: string; ids: string[] } {
  const lines = answer.replace(/\s+$/, "").split("\n");
  for (let i = lines.length - 1; i >= Math.max(0, lines.length - 3); i--) {
    const m = ACTIONS_LINE.exec(lines[i]!);
    if (!m) continue;
    const ids = [
      ...new Set(
        m[1]!
          .split(/[\s,;]+/)
          .map((s) => s.replace(/[[\]`*"'()]/g, "").trim())
          .filter(Boolean),
      ),
    ].slice(0, MAX_SUGGESTED);
    lines.splice(i, 1);
    return { text: lines.join("\n").replace(/\s+$/, ""), ids };
  }
  return { text: answer, ids: [] };
}

const cachedRefs = (repoId: number) => queryClient.getQueryData<RepoRefs>(repoKeys.refs(repoId));

/**
 * Actions offered to the model for `notice`: its own buttons first (`fix1`…), Retry, then
 * everyday actions that never lose work. `skip`: labels not to repeat (the "Hỏi AI" button).
 */
export function helpActions(notice: Notice, skip: readonly string[] = []): HelpAction[] {
  const out: HelpAction[] = [];
  const own = (notice.actions ?? []).filter((a: NoticeAction) => !skip.includes(a.label));
  own.forEach((a, i) =>
    out.push({
      id: `fix${i + 1}`,
      label: a.label,
      does: `the app's own fix for this error: "${a.label}"`,
      run: a.run,
    }),
  );
  if (notice.retry) {
    out.push({
      id: "retry",
      label: i18n.t("aiHelp.action.retry"),
      does: "run the operation that failed again, unchanged",
      run: notice.retry,
    });
  }
  out.push(...everydayActions(notice.repoId));
  return out;
}

/**
 * Actions that fit any error and never lose work (fetch, pull, push, stash, LFS settings,
 * accounts). They need nothing but the repository, so an old chat can offer them again.
 */
export function everydayActions(repoId: number | undefined): HelpAction[] {
  const accounts: HelpAction = {
    id: "accounts",
    label: i18n.t("aiHelp.action.accounts"),
    does: "open the account manager (sign in again, add or switch accounts)",
    run: () => useAccountDialogs.getState().openManager(),
  };
  if (repoId === undefined) return [accounts];
  return [
    {
      id: "fetch",
      label: i18n.t("aiHelp.action.fetch"),
      does: "fetch: download new commits from the server, changes nothing in the user's files",
      run: () => void fetchAll(repoId),
    },
    {
      id: "pull",
      label: i18n.t("aiHelp.action.pull"),
      does: "pull: merge the server's new commits into the current branch",
      run: () => void pull(repoId, "merge"),
    },
    {
      id: "push",
      label: i18n.t("aiHelp.action.push"),
      does: "push the current branch (checks big files and the account first)",
      run: () => {
        const refs = cachedRefs(repoId);
        if (refs) void push(repoId, refs, false);
      },
    },
    {
      id: "stash",
      label: i18n.t("aiHelp.action.stash"),
      does: "stash: put uncommitted changes aside (can be brought back later)",
      run: () => useStashUi.getState().openCreate(repoId),
    },
    {
      id: "lfs",
      label: i18n.t("aiHelp.action.lfs"),
      does: "open the Git LFS settings (choose which file types are stored with LFS)",
      run: () => useLfsManager.getState().open(repoId, "patterns"),
    },
    accounts,
  ];
}

/** The lines added to the facts sent to the model. */
export function actionsContext(actions: readonly HelpAction[]): string {
  if (!actions.length) return "";
  return [
    "Actions the app can run (one click under your answer):",
    ...actions.map((a) => `[${a.id}] ${a.does}`),
  ].join("\n");
}

/** Buttons of the chats started in this session (their handlers live in memory). */
const byChat = new Map<string, HelpAction[]>();

const CHAT_REPOS_KEY = "nanomgg.aiHelp.chatRepos";
/** Chats whose repository is remembered (oldest dropped first). */
const MAX_CHAT_REPOS = 200;

function loadChatRepos(): Record<string, string> {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(CHAT_REPOS_KEY) ?? "{}");
    return parsed && typeof parsed === "object" ? (parsed as Record<string, string>) : {};
  } catch {
    return {};
  }
}

/**
 * Keeps the buttons of a chat. `repoPath` (folder of the repository the error was in, ""
 * for an error outside a repository) is stored on this computer, so the chat still offers
 * the everyday actions after a restart; the error's own fixes only live as long as the app.
 */
export function rememberChatActions(
  chatId: string,
  actions: readonly HelpAction[],
  repoPath?: string | null,
) {
  if (actions.length) byChat.set(chatId, [...actions]);
  if (repoPath === undefined || repoPath === null) return;
  try {
    const entries = Object.entries({ ...loadChatRepos(), [chatId]: repoPath });
    localStorage.setItem(
      CHAT_REPOS_KEY,
      JSON.stringify(Object.fromEntries(entries.slice(-MAX_CHAT_REPOS))),
    );
  } catch {
    // Storage off: the buttons simply stay for this session only.
  }
}

/**
 * Buttons a chat can offer: the ones it was started with, or, for a chat of an earlier
 * session, the everyday actions when its repository is open (`tabs`: the open tabs).
 */
export function chatActions(
  chatId: string | null | undefined,
  tabs: readonly { id: number; path: string; missing: boolean }[] = [],
): HelpAction[] {
  if (!chatId) return [];
  const live = byChat.get(chatId);
  if (live) return live;
  const path = loadChatRepos()[chatId];
  if (path === undefined) return [];
  if (path === "") return everydayActions(undefined);
  const tab = tabs.find((t) => t.path === path && !t.missing);
  return tab ? everydayActions(tab.id) : [];
}

/** The actions an answer suggests, in the model's order, known ids only. */
export function suggested(actions: readonly HelpAction[], ids: readonly string[]): HelpAction[] {
  return ids.flatMap((id) => actions.find((a) => a.id === id) ?? []);
}

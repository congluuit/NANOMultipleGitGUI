/**
 * "What's new" after a Fetch: compares the refs before and after and says, in plain words,
 * which remote branches moved, appeared or disappeared, and how far behind the local branches
 * now are (with a Pull button when the current branch is behind).
 */
import i18n from "i18next";
import { repoKeys } from "@/features/repos/queries";
import { commands, type RepoRefs } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useUi, type Notice } from "@/stores/ui";

export interface FetchChanges {
  /** `remote/branch` that got new commits (or were rewritten). */
  updated: string[];
  added: string[];
  removed: string[];
  /** Local branches that are now further behind their upstream. */
  behind: { branch: string; count: number; isHead: boolean }[];
}

/** Lines of the hint at most; the rest is summed up as "… and N more". */
const MAX_LINES = 6;

function remoteOids(refs: RepoRefs): Map<string, string> {
  const out = new Map<string, string>();
  for (const remote of refs.remotes) {
    for (const b of remote.branches) {
      if (b.name === "HEAD") continue;
      out.set(`${remote.name}/${b.name}`, b.oid);
    }
  }
  return out;
}

export function diffRefs(before: RepoRefs, after: RepoRefs): FetchChanges {
  const old = remoteOids(before);
  const now = remoteOids(after);
  const updated: string[] = [];
  const added: string[] = [];
  const removed: string[] = [];
  for (const [name, oid] of now) {
    const prev = old.get(name);
    if (prev === undefined) added.push(name);
    else if (prev !== oid) updated.push(name);
  }
  for (const name of old.keys()) if (!now.has(name)) removed.push(name);

  const behindBefore = new Map(before.local.map((b) => [b.name, b.behind]));
  const behind = after.local
    .filter((b) => b.upstream && !b.upstreamGone && b.behind > (behindBefore.get(b.name) ?? 0))
    .map((b) => ({ branch: b.name, count: b.behind, isHead: b.isHead }))
    // The current branch first: that is the one a Pull acts on.
    .sort((a, b) => Number(b.isHead) - Number(a.isHead));
  return { updated, added, removed, behind };
}

export function hasChanges(c: FetchChanges): boolean {
  return c.updated.length + c.added.length + c.removed.length + c.behind.length > 0;
}

/** `null` when the fetch brought nothing new (the plain "nothing changed" message stays). */
export function fetchChangesNotice(c: FetchChanges, onPull?: () => void): Notice | null {
  if (!hasChanges(c)) return null;
  const t = i18n.t.bind(i18n);
  const lines = [
    ...c.behind.map((b) =>
      t(b.isHead ? "remote.whatsNew.behindHead" : "remote.whatsNew.behind", {
        branch: b.branch,
        count: b.count,
      }),
    ),
    ...c.updated.map((name) => t("remote.whatsNew.updated", { name })),
    ...c.added.map((name) => t("remote.whatsNew.added", { name })),
    ...c.removed.map((name) => t("remote.whatsNew.removed", { name })),
  ];
  const shown = lines.slice(0, MAX_LINES);
  if (lines.length > MAX_LINES) {
    shown.push(t("remote.whatsNew.more", { count: lines.length - MAX_LINES }));
  }
  const head = c.behind.find((b) => b.isHead);
  return {
    kind: "info",
    message: t("remote.whatsNew.title", {
      count: c.updated.length + c.added.length + c.removed.length,
    }),
    hint: shown.join("\n"),
    actions:
      head && onPull
        ? [{ label: t("remote.whatsNew.pull", { branch: head.branch }), run: onPull }]
        : [],
  };
}

async function readRefs(repoId: number): Promise<RepoRefs | null> {
  try {
    return await queryClient.fetchQuery({
      queryKey: repoKeys.refs(repoId),
      queryFn: () => commands.repoRefs(repoId),
      staleTime: 0,
    });
  } catch {
    return null;
  }
}

/**
 * Runs a fetch and, when it went through and changed something, replaces its short "Fetched"
 * notice with the summary of what is new.
 */
export async function fetchWithSummary<T>(
  repoId: number,
  run: () => Promise<T | undefined>,
  onPull?: () => void,
): Promise<T | undefined> {
  const before =
    queryClient.getQueryData<RepoRefs>(repoKeys.refs(repoId)) ?? (await readRefs(repoId));
  const value = await run();
  if (value === undefined || !before) return value;
  const after = await readRefs(repoId);
  const notice = after ? fetchChangesNotice(diffRefs(before, after), onPull) : null;
  if (notice) useUi.getState().showNotice(notice);
  return value;
}

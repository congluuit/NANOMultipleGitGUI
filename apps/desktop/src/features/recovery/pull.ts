import i18n from "i18next";
import { conflictText } from "@/features/commits/outcome";
import { push } from "@/features/command-palette/actions";
import { startTransfer } from "@/features/remote/transfers";
import { commands, errorMessage, type PullMode, type RepoRefs } from "@/ipc";
import { useUi, type Notice } from "@/stores/ui";

/** Remote to pull from when the branch has no upstream yet: `origin`, else the only one. */
export function pullSource(refs: RepoRefs): { remote: string; branch: string } | null {
  const branch = refs.head.branch;
  if (!branch || refs.head.detached) return null;
  const remote = refs.remotes.find((r) => r.name === "origin") ?? refs.remotes[0];
  return remote ? { remote: remote.name, branch } : null;
}

async function pushCurrent(repoId: number) {
  try {
    await push(repoId, await commands.repoRefs(repoId), false);
  } catch (e) {
    useUi.getState().showNotice({ kind: "error", message: errorMessage(e), repoId });
  }
}

/**
 * Pull so that a rejected push can go through: from the upstream, or — for a branch that has
 * none yet (a remote added by URL) — from the same-named branch of `origin`. When it worked,
 * the notice offers the Push.
 */
export async function pullBeforePush(repoId: number, mode: PullMode): Promise<void> {
  let refs: RepoRefs;
  try {
    refs = await commands.repoRefs(repoId);
  } catch (e) {
    useUi.getState().showNotice({ kind: "error", message: errorMessage(e), repoId });
    return;
  }
  const head = refs.local.find((b) => b.isHead);
  const tracked = !!head?.upstream && !head.upstreamGone;
  const source = tracked ? null : pullSource(refs);
  if (!tracked && !source) {
    useUi.getState().showNotice({ kind: "error", message: i18n.t("recovery.pull.noSource") });
    return;
  }
  await startTransfer(
    repoId,
    "pull",
    (op) =>
      commands.remotePull(repoId, op, source?.remote ?? null, source?.branch ?? null, mode, null),
    (o): Notice | string =>
      conflictText(o) || {
        kind: "success",
        message: i18n.t(
          mode === "mergeUnrelated" ? "recovery.pull.mergedUnrelated" : "recovery.pull.done",
        ),
        hint: i18n.t("recovery.pull.pushHint"),
        actions: [{ label: i18n.t("toolbar.push"), run: () => void pushCurrent(repoId) }],
        durationMs: 15_000,
      },
    source ? `${source.remote}/${source.branch}` : null,
  );
}

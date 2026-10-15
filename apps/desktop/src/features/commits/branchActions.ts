/**
 * Actions of the branch context menu (graph label, sidebar row): pull / push a branch that may
 * not be checked out, upstream, links to the hosted project.
 */
import i18n from "i18next";
import { pull, pushBranch } from "@/features/command-palette/actions";
import { splitUpstream } from "@/features/remote/plan";
import { startTransfer } from "@/features/remote/transfers";
import { commands, type HostedRemote, type RepoRefs } from "@/ipc";
import { useUi } from "@/stores/ui";
import { runOutcome } from "./outcome";

/**
 * "Pull (fast-forward if possible)". The checked-out branch pulls (merge, which fast-forwards
 * when it can); another branch is fetched and fast-forwarded to its upstream without checking
 * it out — when it has diverged the user is told to check it out and pull.
 */
export async function pullBranch(repoId: number, refs: RepoRefs, name: string) {
  const local = refs.local.find((b) => b.name === name);
  if (!local) return;
  if (local.isHead) {
    await pull(repoId, "merge");
    return;
  }
  const notify = useUi.getState().showNotice;
  const up =
    local.upstream && !local.upstreamGone
      ? splitUpstream(
          local.upstream,
          refs.remotes.map((r) => r.name),
        )
      : null;
  if (!local.upstream || !up) {
    notify({ kind: "info", message: i18n.t("branchMenu.noUpstream", { name }) });
    return;
  }
  const upstream = local.upstream;
  const fetched = await startTransfer(
    repoId,
    "fetch",
    (op) => commands.remoteFetch(repoId, op, up.remote, false, false, null),
    () => null,
    up.remote,
  );
  if (fetched === undefined) return;
  const rel = await runOutcome(repoId, () => commands.branchRelation(repoId, upstream, name));
  if (!rel) return;
  if (rel.canFastForward) {
    await runOutcome(
      repoId,
      () => commands.branchFastForward(repoId, name, upstream),
      () => ({
        kind: "success",
        message: i18n.t("branchMenu.fastForwarded", { name, upstream, count: rel.ahead }),
      }),
    );
  } else if (rel.ahead === 0) {
    notify({ kind: "info", message: i18n.t("branchMenu.upToDate", { name, upstream }) });
  } else {
    notify({
      kind: "info",
      message: i18n.t("branchMenu.diverged", { name, upstream }),
      hint: i18n.t("branchMenu.divergedHint", { name }),
    });
  }
}

export { pushBranch };

/** Sets (`origin/main`) or removes (`null`) the upstream of a local branch. */
export function setUpstream(repoId: number, name: string, upstream: string | null) {
  return runOutcome(
    repoId,
    () => commands.branchSetUpstream(repoId, name, upstream),
    () =>
      upstream
        ? i18n.t("branchMenu.upstreamSet", { name, upstream })
        : i18n.t("branchMenu.upstreamUnset", { name }),
  );
}

/** Deletes `branch` on `remote` (a push), reported in the transfer queue. */
export function deleteRemoteBranch(repoId: number, remote: string, branch: string) {
  return startTransfer(
    repoId,
    "push",
    (op) => commands.remoteDeleteBranch(repoId, op, remote, branch),
    () => ({
      kind: "success",
      message: i18n.t("branchMenu.remoteDeleted", { name: `${remote}/${branch}` }),
    }),
    `${remote}/${branch}`,
  );
}

const enc = (path: string) => path.split("/").map(encodeURIComponent).join("/");

function isGitLab(h: HostedRemote): boolean {
  return h.family ? h.family === "gitlab" : h.host.includes("gitlab");
}

/** Web page of a branch of a hosted project (GitHub `/tree/x`, GitLab `/-/tree/x`). */
export function webBranchUrl(h: HostedRemote, branch: string): string {
  const base = `https://${h.host}/${h.fullName}`;
  return isGitLab(h) ? `${base}/-/tree/${enc(branch)}` : `${base}/tree/${enc(branch)}`;
}

/** Web page of a commit of a hosted project. */
export function webCommitUrl(h: HostedRemote, sha: string): string {
  const base = `https://${h.host}/${h.fullName}`;
  return isGitLab(h) ? `${base}/-/commit/${sha}` : `${base}/commit/${sha}`;
}

export function copyText(text: string) {
  void navigator.clipboard.writeText(text).catch(() => {});
}

export { startAmend } from "./amend";

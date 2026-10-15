/**
 * Publishing a repository without a remote: create an empty repository on GitHub / GitLab (or
 * take a URL) → add it as a remote bound to the account → push the current branch with
 * `--set-upstream`. Each step reports its phase so the dialog can show progress.
 */
import i18n from "i18next";
import { invalidateAccounts } from "@/features/accounts/queries";
import type { ForkFailure } from "@/features/forks/actions";
import { startTransfer } from "@/features/remote/transfers";
import { repoKeys } from "@/features/repos/queries";
import {
  commands,
  errorMessage,
  type ForkRemoteChange,
  type ForkRepo,
  type NewRepoOptions,
} from "@/ipc";
import { queryClient } from "@/lib/queryClient";

export type PublishPhase = "creating" | "linking" | "pushing";

export type PublishStep =
  | { status: "done"; repo: ForkRepo | null; change: ForkRemoteChange; pushed: boolean }
  | { status: "createFailed"; failure: ForkFailure }
  /** The remote is in place but the push failed (details are in the notice bar). */
  | { status: "pushFailed"; repo: ForkRepo | null; change: ForkRemoteChange }
  | { status: "failed"; message: string };

export interface PublishTarget {
  repoId: number;
  /** Branch pushed after linking; `null` = do not push (no commit yet). */
  branch: string | null;
  remoteName: string;
  /** Account bound to the remote (and that creates the repository). */
  accountId: string | null;
}

/** Pushes `branch` to `remote` with `--set-upstream` as a transfer-queue item. */
export async function pushNewBranch(repoId: number, remote: string, branch: string) {
  const out = await startTransfer(
    repoId,
    "push",
    (op) =>
      commands.remotePush(
        repoId,
        op,
        { remote, branch, remoteBranch: null, setUpstream: true, forceWithLease: false },
        null,
      ),
    () => ({
      kind: "success" as const,
      message: i18n.t("publish.pushed", { branch, remote }),
    }),
    `${remote}/${branch}`,
  );
  return out !== undefined;
}

async function linkAndPush(
  target: PublishTarget,
  url: string,
  repo: ForkRepo | null,
  onPhase: (phase: PublishPhase) => void,
): Promise<PublishStep> {
  onPhase("linking");
  let change: ForkRemoteChange;
  try {
    change = await commands.publishAddRemote(
      target.repoId,
      target.remoteName,
      url,
      target.accountId,
    );
  } catch (e) {
    return { status: "failed", message: errorMessage(e) };
  }
  void queryClient.invalidateQueries({ queryKey: repoKeys.refs(target.repoId) });
  void queryClient.invalidateQueries({ queryKey: ["hosting"] });
  invalidateAccounts();
  if (!target.branch) return { status: "done", repo, change, pushed: false };
  onPhase("pushing");
  const pushed = await pushNewBranch(target.repoId, change.remote, target.branch);
  void queryClient.invalidateQueries({ queryKey: repoKeys.refs(target.repoId) });
  return pushed
    ? { status: "done", repo, change, pushed: true }
    : { status: "pushFailed", repo, change };
}

/** Creates an empty repository with `target.accountId`, links it and pushes. */
export async function publishNewRepo(
  target: PublishTarget & { accountId: string },
  options: NewRepoOptions,
  protocol: "https" | "ssh",
  onPhase: (phase: PublishPhase) => void,
): Promise<PublishStep> {
  onPhase("creating");
  let repo: ForkRepo;
  try {
    const created = await commands.publishCreateRepo(target.accountId, options);
    if (created.status !== "done") return { status: "createFailed", failure: created };
    repo = created.value;
  } catch (e) {
    return { status: "failed", message: errorMessage(e) };
  }
  const url = protocol === "ssh" && repo.cloneSsh ? repo.cloneSsh : repo.cloneHttps;
  return linkAndPush(target, url, repo, onPhase);
}

/** Adds an existing repository by URL, then pushes. */
export function publishToUrl(
  target: PublishTarget,
  url: string,
  onPhase: (phase: PublishPhase) => void,
): Promise<PublishStep> {
  return linkAndPush(target, url.trim(), null, onPhase);
}

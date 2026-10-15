/** Write actions of the pull request feature, with notices. */
import i18n from "i18next";
import { runOutcome } from "@/features/commits/outcome";
import {
  commands,
  type ProviderFamily,
  type PullCommentTarget,
  type PullDetail,
  type PullMergeRequest,
  type PullReviewEvent,
} from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import type { HostedItemRef } from "@/stores/hosting";
import { useUi } from "@/stores/ui";
import { notifyPullError } from "./errors";
import { pullNumber, shortTerm } from "./model";
import { invalidatePull, invalidatePullLists, pullKeys } from "./queries";
import { openCreatePull } from "./store";

type Item = Pick<HostedItemRef, "accountId" | "fullName" | "number">;

const info = (message: string) => useUi.getState().showNotice({ kind: "info", message });

/** Runs a provider write as `actor`; reports failures; `true` when it went through. */
async function write(r: Item, call: () => Promise<unknown>, done?: string): Promise<boolean> {
  try {
    await call();
    if (done) info(done);
    return true;
  } catch (e) {
    notifyPullError(e);
    return false;
  } finally {
    invalidatePull(r);
  }
}

export const postComment = (r: Item, actor: string, target: PullCommentTarget, body: string) =>
  write(r, () => commands.pullCommentCreate(actor, r.fullName, r.number, target, body));

export const submitReview = (r: Item, actor: string, event: PullReviewEvent, body: string) =>
  write(
    r,
    () => commands.pullReviewSubmit(actor, r.fullName, r.number, event, body),
    i18n.t(`pulls.review.done.${event}`),
  );

export async function mergePull(
  r: Item,
  actor: string,
  family: ProviderFamily | null,
  request: PullMergeRequest,
): Promise<boolean> {
  try {
    const merged = await commands.pullMerge(actor, r.fullName, r.number, request);
    const base = i18n.t("pulls.merge.done", { number: pullNumber(family, r.number) });
    useUi.getState().showNotice({
      kind: merged.deleteError ? "error" : "info",
      message: base,
      hint: merged.deleteError
        ? i18n.t("pulls.merge.deleteFailed", { message: merged.deleteError })
        : i18n.t("pulls.merge.fetchHint"),
    });
    return true;
  } catch (e) {
    notifyPullError(e);
    return false;
  } finally {
    invalidatePull(r);
    invalidatePullLists();
  }
}

export async function setPullOpen(r: Item, actor: string, open: boolean) {
  const ok = await write(
    r,
    () => commands.pullSetOpen(actor, r.fullName, r.number, open),
    i18n.t(open ? "pulls.reopened" : "pulls.closed"),
  );
  invalidatePullLists();
  return ok;
}

/** Checks out the pull request's branch in the repo (fetching its head first). */
export function checkoutPull(repoId: number, detail: PullDetail, family: ProviderFamily) {
  return runOutcome(
    repoId,
    () =>
      commands.pullCheckout(
        repoId,
        {
          fullName: detail.targetRepo,
          number: detail.number,
          sourceBranch: detail.sourceBranch,
          fromFork: detail.fromFork,
          family,
        },
        null,
      ),
    (v) =>
      i18n.t(v.created ? "pulls.checkout.created" : "pulls.checkout.switched", {
        branch: v.branch,
      }),
  );
}

/**
 * After a push that created branch `branch` on `remote`: when that remote is on GitHub/GitLab,
 * suggests opening a pull request from it (the notice replaces the plain "pushed" message).
 */
export async function offerCreatePull(repoId: number, remote: string, branch: string) {
  const remotes = await queryClient
    .fetchQuery({
      queryKey: pullKeys.remotes(repoId),
      queryFn: () => commands.hostedRemoteList(repoId),
      staleTime: 30_000,
    })
    .catch(() => null);
  const hosted = remotes?.find((r) => r.remote === remote);
  if (!hosted) return;
  const term = shortTerm(hosted.family);
  useUi.getState().showNotice({
    kind: "info",
    message: i18n.t("pulls.create.afterPush", { branch, term }),
    actions: [
      { label: i18n.t("pulls.create.open", { term }), run: () => openCreatePull(repoId, branch) },
    ],
  });
}

/**
 * "All repositories": fetch or pull every open repository in one go. Each repository runs as
 * its own queue item (progress, cancel) but stays quiet; one message sums the round up.
 */
import i18n from "i18next";
import { failureText } from "@/features/commits/outcome";
import { newOpId } from "@/features/remote/plan";
import { useLastFetch } from "@/features/remote/autoFetch";
import { selectRepoRunning, useTransferQueue } from "@/features/transfers/store";
import { invalidateRepo } from "@/features/worktree/queries";
import { commands, errorMessage, type Outcome, type RepoOverview } from "@/ipc";
import { useUi } from "@/stores/ui";
import { refreshOverview } from "./queries";

/** Repositories worked on at the same time (each one is a git process and a connection). */
const PARALLEL = 3;

/** Can be fetched: the folder is there and its state could be read. */
export const canFetch = (r: RepoOverview) => !r.missing && !r.error;

/**
 * Safe to pull without asking: behind its upstream, nothing uncommitted, no merge or rebase
 * under way. The pull is fast-forward only, so a branch with its own commits is left alone
 * when it cannot simply move forward.
 */
export const canPull = (r: RepoOverview) =>
  canFetch(r) &&
  !!r.upstream &&
  r.behind > 0 &&
  r.changes === 0 &&
  r.conflicts === 0 &&
  !r.operation;

export interface RoundResult {
  done: string[];
  failed: { name: string; error: string }[];
  skipped: string[];
}

async function runQuiet<T>(
  row: RepoOverview,
  kind: "fetch" | "pull",
  call: (opId: string) => Promise<Outcome<T>>,
  result: RoundResult,
) {
  const queue = useTransferQueue.getState();
  if (selectRepoRunning(queue, row.repoId)) {
    result.skipped.push(row.name);
    return;
  }
  const opId = newOpId(kind);
  queue.start({ opId, repoId: row.repoId, kind, subject: null });
  const finish = useTransferQueue.getState().finish;
  try {
    const out = await call(opId);
    if (out.status === "done") {
      finish(opId, "done", { text: i18n.t(`overview.done.${kind}`) });
      result.done.push(row.name);
    } else if (out.status === "rejected") {
      const error = failureText(out.reason, out.message);
      finish(opId, "failed", { error });
      result.failed.push({ name: row.name, error: i18n.t(`failure.${out.reason}`) });
    } else {
      finish(opId, "cancelled", { text: i18n.t("ops.cancelled") });
      result.skipped.push(row.name);
    }
  } catch (e) {
    const error = errorMessage(e);
    finish(opId, "failed", { error });
    result.failed.push({ name: row.name, error });
  }
  if (kind === "fetch") useLastFetch.getState().mark(row.repoId);
  invalidateRepo(row.repoId);
}

async function round(
  rows: RepoOverview[],
  work: (row: RepoOverview, result: RoundResult) => Promise<void>,
): Promise<RoundResult> {
  const result: RoundResult = { done: [], failed: [], skipped: [] };
  const todo = [...rows];
  const worker = async () => {
    for (let row = todo.shift(); row; row = todo.shift()) await work(row, result);
  };
  await Promise.all(Array.from({ length: Math.min(PARALLEL, rows.length) }, worker));
  await refreshOverview();
  return result;
}

function report(kind: "fetch" | "pull", r: RoundResult) {
  const t = i18n.t.bind(i18n);
  const failed = r.failed.length;
  useUi.getState().showNotice({
    kind: failed ? "error" : "success",
    message: failed
      ? t(`overview.result.${kind}Failed`, { done: r.done.length, failed })
      : t(`overview.result.${kind}`, { count: r.done.length }),
    detail: failed ? r.failed.map((f) => `${f.name}: ${f.error}`).join("\n") : undefined,
  });
}

/** Fetches every remote of every repository. */
export async function fetchAllRepos(rows: RepoOverview[]): Promise<RoundResult> {
  const result = await round(rows.filter(canFetch), (row, res) =>
    runQuiet(
      row,
      "fetch",
      (op) => commands.remoteFetch(row.repoId, op, null, true, true, null),
      res,
    ),
  );
  report("fetch", result);
  return result;
}

/** Fast-forwards every repository that is behind and has nothing uncommitted. */
export async function pullAllRepos(rows: RepoOverview[]): Promise<RoundResult> {
  const result = await round(rows.filter(canPull), (row, res) =>
    runQuiet(
      row,
      "pull",
      (op) => commands.remotePull(row.repoId, op, null, null, "fastForwardOnly", null),
      res,
    ),
  );
  report("pull", result);
  return result;
}

/** Counts for the header: how many repositories need attention. */
export function overviewTotals(rows: RepoOverview[]) {
  return {
    behind: rows.filter((r) => r.behind > 0).length,
    ahead: rows.filter((r) => r.ahead > 0).length,
    dirty: rows.filter((r) => r.changes > 0 || r.conflicts > 0).length,
    pullable: rows.filter(canPull).length,
  };
}

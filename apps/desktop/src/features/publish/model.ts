/**
 * Pure logic of publishing a repository that has no remote yet: the suggested repository name,
 * name checks, the URL of an existing repository and the "first steps" checklist.
 */
import type { RepoRefs } from "@/ipc";

/** Same rule as the backend: letters, digits, `.`, `_`, `-`; not `.`/`..`, at most 100. */
export function validRepoName(name: string): boolean {
  return (
    /^[A-Za-z0-9._-]{1,100}$/.test(name) &&
    name !== "." &&
    name !== ".." &&
    !name.startsWith("-") &&
    !name.endsWith(".git")
  );
}

/** Repository name suggested from a folder path: `D:\Code\Dự án mới` → `Du-an-moi`. */
export function suggestRepoName(path: string): string {
  const folder =
    path
      .replace(/[\\/]+$/, "")
      .split(/[\\/]/)
      .pop() ?? "";
  const ascii = folder
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/đ/g, "d")
    .replace(/Đ/g, "D");
  const name = ascii
    .replace(/[^A-Za-z0-9._-]+/g, "-")
    .replace(/-{2,}/g, "-")
    .replace(/^[-.]+|[-.]+$/g, "")
    .replace(/\.git$/i, "")
    .slice(0, 100);
  return validRepoName(name) ? name : "my-project";
}

/** HTTPS clone URL of `owner/name` on a provider web root (`https://github.com`). */
export function repoUrl(baseUrl: string, owner: string, name: string): string {
  return `${baseUrl.replace(/\/+$/, "")}/${owner.replace(/^\/+|\/+$/g, "")}/${name}.git`;
}

/** Lower-cased host of a remote URL (`https://…`, `ssh://…`, `git@host:path`). */
export function remoteHost(url: string): string | null {
  const u = url.trim();
  const scp = /^[\w.-]+@([\w.-]+):/.exec(u);
  if (scp) return scp[1]!.toLowerCase();
  try {
    return new URL(u).hostname.toLowerCase() || null;
  } catch {
    return null;
  }
}

/** Plausible remote URL: http(s), ssh, git, file URLs or `user@host:path`. */
export function plausibleRemoteUrl(url: string): boolean {
  const u = url.trim();
  if (!u || u.startsWith("-") || /\s/.test(u)) return false;
  return /^(https?|ssh|git|file):\/\/.+/i.test(u) || /^[\w.-]+@[\w.-]+:.+/.test(u);
}

export type FirstStep = "commit" | "publish" | "push";

export interface FirstStepsState {
  /** Steps already done. */
  done: Record<FirstStep, boolean>;
  /** The step to do next; `null` = all done. */
  next: FirstStep | null;
  /** Current branch (`main` in a fresh repository). */
  branch: string | null;
}

/**
 * Checklist of a new project: make the first commit → publish it (a remote exists) → the
 * current branch is pushed (has an upstream).
 */
export function firstSteps(refs: RepoRefs): FirstStepsState {
  const head = refs.head;
  const current = refs.local.find((b) => b.isHead);
  const done: Record<FirstStep, boolean> = {
    commit: !head.unborn,
    publish: refs.remotes.length > 0,
    push: !!current?.upstream && !current.upstreamGone,
  };
  const order: FirstStep[] = ["commit", "publish", "push"];
  return { done, next: order.find((s) => !done[s]) ?? null, branch: head.branch };
}

/** Show the checklist: a repository that is not on a server yet. */
export const needsFirstSteps = (refs: RepoRefs) =>
  !refs.head.detached && (refs.head.unborn || refs.remotes.length === 0);

/** A remote was just added but nothing is on it yet: the push step is still to do. */
const awaitingFirstPush = (refs: RepoRefs) =>
  refs.remotes.length > 0 &&
  refs.remotes.every((r) => r.branches.length === 0) &&
  !refs.local.some((b) => b.upstream && !b.upstreamGone);

/** Whether the "first steps" checklist applies to a repository. */
export const showFirstSteps = (refs: RepoRefs) =>
  !refs.head.detached && (needsFirstSteps(refs) || awaitingFirstPush(refs));

import type { HostedRemote } from "@/ipc";
import { primaryRemote } from "./model";
import { useHostedRemotes } from "./queries";

export type IssueContextState =
  | { status: "loading" }
  | { status: "error"; message: string }
  /** No GitHub / GitLab remote. */
  | { status: "noRemote" }
  /** The remote is not bound to an account. */
  | { status: "unbound"; remote: HostedRemote }
  /** The account has no token (signed in with SSH only, or the token was removed). */
  | { status: "noToken"; remote: HostedRemote; accountId: string }
  | { status: "ready"; remote: HostedRemote; accountId: string };

/** Whether issues of a repository can be loaded, and with which remote / account. */
export function useIssueContext(repoId: number | null | undefined): IssueContextState {
  const remotes = useHostedRemotes(repoId);
  if (remotes.isPending) return { status: "loading" };
  if (remotes.isError) return { status: "error", message: String(remotes.error) };
  const remote = primaryRemote(remotes.data);
  if (!remote) return { status: "noRemote" };
  if (!remote.accountId) return { status: "unbound", remote };
  if (!remote.hasToken) return { status: "noToken", remote, accountId: remote.accountId };
  return { status: "ready", remote, accountId: remote.accountId };
}

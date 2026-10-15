import { create } from "zustand";

/**
 * A pull request (GitHub) / merge request (GitLab) or an issue on a hosted project. Opened from
 * anywhere (sidebar, "My work" panel across accounts, commit details…) and shown by the feature
 * that owns it (`features/pulls`, `features/issues`).
 */
export type HostedItemRef = {
  /** Account whose token is used for the API calls. */
  accountId: string;
  /** `owner/repo` or `group/sub/project`. */
  fullName: string;
  /** PR/MR number (GitLab: the project-scoped `iid`) or issue number. */
  number: number;
  /** Open repository tab it belongs to, when known (enables checkout / local actions). */
  repoId?: number | null;
};

type HostingNav = {
  pullRequest: HostedItemRef | null;
  issue: HostedItemRef | null;
  openPullRequest: (ref: HostedItemRef) => void;
  openIssue: (ref: HostedItemRef) => void;
  closePullRequest: () => void;
  closeIssue: () => void;
};

export const useHostingNav = create<HostingNav>((set) => ({
  pullRequest: null,
  issue: null,
  openPullRequest: (ref) => set({ pullRequest: ref, issue: null }),
  openIssue: (ref) => set({ issue: ref, pullRequest: null }),
  closePullRequest: () => set({ pullRequest: null }),
  closeIssue: () => set({ issue: null }),
}));

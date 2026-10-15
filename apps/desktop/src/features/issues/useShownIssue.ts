import { useHostingNav, type HostedItemRef } from "@/stores/hosting";

/**
 * The issue the center area of repository tab `repoId` shows: one opened from that tab, or one
 * opened without a tab ("My work" for a repository that is not open).
 */
export function useShownIssue(repoId: number | null | undefined): HostedItemRef | null {
  return useHostingNav((s) =>
    s.issue && (s.issue.repoId == null || s.issue.repoId === repoId) ? s.issue : null,
  );
}

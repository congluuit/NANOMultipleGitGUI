import { useHostingNav, type HostedItemRef } from "@/stores/hosting";

/**
 * The pull request the center area of repository tab `repoId` shows: one opened from that tab,
 * or one opened without a tab ("My work" for a repository that is not open).
 */
export function useShownPull(repoId: number | null | undefined): HostedItemRef | null {
  return useHostingNav((s) =>
    s.pullRequest && (s.pullRequest.repoId == null || s.pullRequest.repoId === repoId)
      ? s.pullRequest
      : null,
  );
}

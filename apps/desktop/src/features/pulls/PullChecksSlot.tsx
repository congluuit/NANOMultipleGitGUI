import { CiSummary } from "@/features/ci";

export interface PullChecksSlotProps {
  repoId: number | null;
  accountId: string;
  fullName: string;
  sha: string;
}

/** CI checks of the pull request head commit (renders nothing when the project has no CI). */
export function PullChecksSlot({ accountId, fullName, sha }: PullChecksSlotProps) {
  return <CiSummary accountId={accountId} fullName={fullName} sha={sha} />;
}

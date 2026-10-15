/**
 * CI status of commits (PLAN §5.6, Phase 4.3).
 *
 * - Graph: `useCiStatuses(repoId, ciCandidateHashes(refs, rows))` → `bySha.get(hash)?.status.state`,
 *   drawn with `drawCiBadge(ctx, x, y, state, readCiColors())`.
 * - Commit details: `<CommitCiRow repoId hash />`; status bar: `<HeadCiIndicator />`.
 * - PR / MR pages: `<CiSummary accountId fullName sha />`.
 */
export { CiBadge } from "./CiBadge";
export { jobIcon, useAccountLabelOf, useCiTooltip } from "./hooks";
export { CiChecksList } from "./CiChecksList";
export { CiEntryView, CiProblemHint, CiSummary } from "./CiSummary";
export { CommitCiRow, HeadCiIndicator } from "./CommitCi";
export { CI_BADGE_SIZE, ciColor, drawCiBadge, readCiColors, type CiColors } from "./draw";
export { describeCiProblem, describeProviderError } from "./errors";
export { ciCandidateHashes, ciGlyph, entriesBySha, hasPending } from "./model";
export {
  ciKeys,
  useCiEnabled,
  useCiStatus,
  useCiStatusFor,
  useCiStatuses,
  type CiStatuses,
} from "./queries";

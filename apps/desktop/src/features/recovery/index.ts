import { setFailureFixer } from "@/features/transfers/store";
import { setNoticeEnhancer } from "@/stores/ui";
import { largeFileActions } from "./largeFiles";
import { enhanceNotice } from "./notice";

export { detectProblem, type Problem, type ProblemKind } from "./detect";
export { enhanceNotice } from "./notice";
export { pullBeforePush } from "./pull";

// Every error notice gets a plain-words explanation and one-click fixes when one applies.
setNoticeEnhancer(enhanceNotice);

// A push refused for a file over the server's limit: the queue offers the fix, not Retry.
setFailureFixer((repoId, reason, message, retry) =>
  reason === "fileTooLarge"
    ? (largeFileActions(repoId, { kind: "error", repoId, reason, message, retry })[0] ?? null)
    : null,
);

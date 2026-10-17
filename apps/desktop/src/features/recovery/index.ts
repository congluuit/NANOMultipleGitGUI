import { setNoticeEnhancer } from "@/stores/ui";
import { enhanceNotice } from "./notice";

export { detectProblem, type Problem, type ProblemKind } from "./detect";
export { enhanceNotice } from "./notice";
export { pullBeforePush } from "./pull";

// Every error notice gets a plain-words explanation and one-click fixes when one applies.
setNoticeEnhancer(enhanceNotice);

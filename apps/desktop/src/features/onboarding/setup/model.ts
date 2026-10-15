import type {
  SetupInstallOutcome,
  SetupProgress,
  SetupStep,
  SetupTool,
  SetupToolState,
} from "@/ipc";

export const STEPS: SetupStep[] = ["welcome", "tools", "accounts", "finish"];

export const emptyProgress = (): SetupProgress => ({
  completed: false,
  skipped: false,
  stepsDone: [],
});

export function withStepDone(progress: SetupProgress, step: SetupStep): SetupProgress {
  if (progress.stepsDone.includes(step)) return progress;
  return { ...progress, stepsDone: [...progress.stepsDone, step] };
}

/** Steps the user has not finished yet (the reminder in Settings). */
export function pendingSteps(progress: SetupProgress): SetupStep[] {
  if (progress.completed) return [];
  return STEPS.filter((s) => s !== "finish" && !progress.stepsDone.includes(s));
}

/** Icon / colour of a tool row: ✅ ok, ⚠️ warn, ❌ error, ℹ️ info (optional and absent). */
export type ToolLevel = "ok" | "warn" | "error" | "info";

export function toolLevel(tool: SetupTool): ToolLevel {
  if (tool.state === "ok") return "ok";
  if (tool.state === "outsidePath") return "warn";
  if (tool.importance === "required") return "error";
  if (tool.importance === "recommended") return "warn";
  return "info";
}

/** "Cài giúp tôi" is offered for a tool that is missing or too old and has an installer. */
export function canInstall(tool: SetupTool): boolean {
  const states: SetupToolState[] = ["missing", "outdated"];
  return tool.installer !== null && states.includes(tool.state);
}

/** The tools probe reports the tool installed (the result needs a re-check). */
export function installSucceeded(outcome: SetupInstallOutcome): boolean {
  return outcome === "installed" || outcome === "alreadyInstalled";
}

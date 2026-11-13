import { AiReviewDialog } from "@/features/ai/review/AiReviewDialog";
import { BisectStartDialog } from "./Bisect";
import { OverviewDialog } from "./OverviewDialog";
import "./palette";
import { PatchApplyDialog } from "./PatchApplyDialog";
import { ReflogDialog } from "./ReflogDialog";
import { SparseDialog } from "./SparseDialog";
import { StatsDialog } from "./StatsDialog";
import { closeTool, useTools } from "./store";

/** Mounted once in the shell: the dialog of the tool that is open, if any. */
export function ToolsHost() {
  const dialog = useTools((s) => s.dialog);
  if (!dialog) return null;
  switch (dialog.kind) {
    case "overview":
      return <OverviewDialog onClose={closeTool} />;
    case "reflog":
      return <ReflogDialog repoId={dialog.repoId} onClose={closeTool} />;
    case "stats":
      return <StatsDialog repoId={dialog.repoId} onClose={closeTool} />;
    case "sparse":
      return <SparseDialog repoId={dialog.repoId} onClose={closeTool} />;
    case "patchApply":
      return (
        <PatchApplyDialog
          key={dialog.file}
          repoId={dialog.repoId}
          file={dialog.file}
          onClose={closeTool}
        />
      );
    case "bisectStart":
      return (
        <BisectStartDialog
          repoId={dialog.repoId}
          good={dialog.good}
          bad={dialog.bad}
          onClose={closeTool}
        />
      );
    case "aiReview":
      return <AiReviewDialog repoId={dialog.repoId} onClose={closeTool} />;
  }
}

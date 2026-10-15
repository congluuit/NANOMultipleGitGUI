import { BlameView } from "@/features/blame/BlameView";
import { ConflictView } from "@/features/conflicts/ConflictView";
import type { FileView } from "@/stores/fileView";
import { FileAtCommitView } from "./FileAtCommitView";
import { FileHistoryView } from "./FileHistoryView";

/** Center-area file view: conflict editor, file history, blame or a file at a commit. */
export function FileViewHost({ repoId, view }: { repoId: number; view: FileView }) {
  switch (view.kind) {
    case "conflict":
      return <ConflictView key={`c:${view.path}`} repoId={repoId} path={view.path} />;
    case "history":
      return <FileHistoryView key={`h:${view.path}`} repoId={repoId} path={view.path} />;
    case "blame":
      return (
        <BlameView
          key={`b:${view.path}:${view.rev ?? ""}`}
          repoId={repoId}
          path={view.path}
          rev={view.rev}
        />
      );
    case "file":
      return (
        <FileAtCommitView
          key={`f:${view.path}:${view.commit}`}
          repoId={repoId}
          path={view.path}
          commit={view.commit}
        />
      );
  }
}

import { CommitDetailsPanel } from "@/features/commit/CommitDetailsPanel";
import { OperationBanner } from "@/features/commits/OperationBanner";
import { RefDialogs } from "@/features/commits/RefDialogs";
import { DiffView } from "@/features/diff/DiffView";
import { GraphView } from "@/features/graph/GraphView";
import { MissingRepo } from "@/features/repos/MissingRepo";
import { WelcomeScreen } from "@/features/repos/WelcomeScreen";
import { TerminalHost } from "@/features/terminal/TerminalHost";
import { WorkingTreePanel } from "@/features/worktree/WorkingTreePanel";
import {
  UnboundBanner,
  useAccountShortcuts,
  useActiveRepoAccount,
} from "@/features/accounts/switcher";
import { useSelection } from "@/stores/graph";
import { useActiveRepo, useWorkspace } from "@/stores/workspace";
import { useOpenDiff } from "@/stores/worktree";
import { NoticeBar, ToolBanner } from "./Banners";
import { Sidebar } from "./Sidebar";
import { StatusBar } from "./StatusBar";
import { Toolbar } from "./Toolbar";
import { TopBar } from "./TopBar";

export function AppShell() {
  const repo = useActiveRepo();
  const loaded = useWorkspace((s) => s.loaded);
  // Window top border in the color of the active repo's account (neutral when unbound).
  const { color } = useActiveRepoAccount();
  const selection = useSelection(repo?.id);
  const diff = useOpenDiff(repo?.id);
  useAccountShortcuts();

  return (
    <div
      className="flex h-full flex-col border-t-[3px]"
      data-testid="app-shell"
      style={{ borderTopColor: color ?? "var(--border)" }}
    >
      <TopBar />
      <ToolBanner />
      <NoticeBar />
      {repo && !repo.missing && <UnboundBanner key={repo.id} repoId={repo.id} />}
      {repo ? (
        <>
          <Toolbar />
          {!repo.missing && <OperationBanner key={repo.id} repoId={repo.id} />}
          <div className="flex min-h-0 flex-1">
            <Sidebar />
            {repo.missing ? (
              <MissingRepo tab={repo} />
            ) : (
              <>
                {/* The graph stays mounted under an open diff so its scroll position survives. */}
                <div className={diff ? "hidden" : "flex min-w-0 flex-1"}>
                  <GraphView key={repo.id} repoId={repo.id} name={repo.name} />
                </div>
                {diff && <DiffView key={repo.id} repoId={repo.id} request={diff} />}
                {selection ? (
                  <CommitDetailsPanel key={selection.hash} repoId={repo.id} hash={selection.hash} />
                ) : (
                  <WorkingTreePanel key={repo.id} repoId={repo.id} />
                )}
              </>
            )}
          </div>
        </>
      ) : (
        loaded && <WelcomeScreen />
      )}
      {/* Outside the repo branch so shells survive switching repos (hidden when not shown). */}
      <TerminalHost />
      <StatusBar />
      <RefDialogs />
    </div>
  );
}

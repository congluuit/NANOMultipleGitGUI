import { CommitDetailsPanel } from "@/features/commit/CommitDetailsPanel";
import { OperationBanner } from "@/features/commits/OperationBanner";
import { RefDialogs } from "@/features/commits/RefDialogs";
import { DiffView } from "@/features/diff/DiffView";
import { FileViewHost } from "@/features/file-history/FileViewHost";
import { GraphView } from "@/features/graph/GraphView";
import { FirstStepsBanner } from "@/features/publish/FirstStepsBanner";
import { MissingRepo } from "@/features/repos/MissingRepo";
import { WelcomeScreen } from "@/features/repos/WelcomeScreen";
import { TerminalHost } from "@/features/terminal/TerminalHost";
import { WorkingTreePanel } from "@/features/worktree/WorkingTreePanel";
import {
  UnboundBanner,
  useAccountShortcuts,
  useActiveRepoAccount,
} from "@/features/accounts/switcher";
import { ViewErrorBoundary } from "@/components/ViewErrorBoundary";
import { closeFileView, useFileView } from "@/stores/fileView";
import { useSelection } from "@/stores/graph";
import { useActiveRepo, useWorkspace } from "@/stores/workspace";
import { useOpenDiff, useWorktreeStore } from "@/stores/worktree";
import { LfsManagerHost } from "@/features/lfs/LfsManagerDialog";
import { IssueView } from "@/features/issues/IssueView";
import { useShownIssue } from "@/features/issues/useShownIssue";
import { PullDetailView } from "@/features/pulls/PullDetailView";
import { useShownPull } from "@/features/pulls/useShownPull";
import { useHostingNav } from "@/stores/hosting";
import { NoticeBar, ToolBanner } from "./Banners";
import { useLayoutShortcuts } from "./layoutShortcuts";
import { PanelRow, SidePanel } from "./PanelLayout";
import { RightRail } from "./RightRail";
import "./palette";
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
  const fileView = useFileView(repo?.id);
  // Phase 4.2: an issue opened from this tab, or from "My work" (no tab).
  const issue = useShownIssue(repo?.id);
  // Phase 4.1: a pull / merge request opened from this tab, or from "My work" (no tab).
  const pull = useShownPull(repo?.id);
  useAccountShortcuts();
  useLayoutShortcuts();

  return (
    <div
      className="flex h-full flex-col border-t-[3px]"
      data-testid="app-shell"
      style={{ borderTopColor: color ?? "var(--border)" }}
    >
      <TopBar />
      <ToolBanner />
      <NoticeBar />
      {repo && !repo.missing && <UnboundBanner key={`unboundBanner-${repo.id}`} repoId={repo.id} />}
      {repo ? (
        <>
          <Toolbar />
          {!repo.missing && <OperationBanner key={`operationBanner-${repo.id}`} repoId={repo.id} />}
          {!repo.missing && <FirstStepsBanner key={`firstSteps-${repo.id}`} repoId={repo.id} />}
          <PanelRow hasRight={!repo.missing}>
            <SidePanel id="sidebar" rail={<Sidebar rail />}>
              <Sidebar />
            </SidePanel>
            {repo.missing ? (
              <MissingRepo tab={repo} />
            ) : (
              <>
                {/* The graph stays mounted under an open diff so its scroll position survives. */}
                <div
                  className={diff || fileView || issue || pull ? "hidden" : "flex min-w-0 flex-1"}
                >
                  <GraphView key={`graphView-${repo.id}`} repoId={repo.id} name={repo.name} />
                </div>
                {pull ? (
                  <ViewErrorBoundary
                    key={`pullView-${repo.id}`}
                    onClose={() => useHostingNav.getState().closePullRequest()}
                    resetKey={JSON.stringify(pull)}
                  >
                    <PullDetailView
                      key={`pull-${pull.accountId}-${pull.fullName}-${pull.number}`}
                      pullRef={pull}
                    />
                  </ViewErrorBoundary>
                ) : issue ? (
                  <ViewErrorBoundary
                    key={`issueView-${repo.id}`}
                    onClose={() => useHostingNav.getState().closeIssue()}
                    resetKey={JSON.stringify(issue)}
                  >
                    <IssueView
                      key={`issue-${issue.accountId}-${issue.fullName}-${issue.number}`}
                      issueRef={issue}
                    />
                  </ViewErrorBoundary>
                ) : fileView ? (
                  <ViewErrorBoundary
                    key={`fileViewHost-${repo.id}`}
                    onClose={() => closeFileView(repo.id)}
                    resetKey={JSON.stringify(fileView)}
                  >
                    <FileViewHost repoId={repo.id} view={fileView} />
                  </ViewErrorBoundary>
                ) : (
                  diff && (
                    <ViewErrorBoundary
                      key={`diffView-${repo.id}`}
                      onClose={() => useWorktreeStore.getState().openDiff(repo.id, null)}
                      resetKey={JSON.stringify(diff)}
                    >
                      <DiffView repoId={repo.id} request={diff} />
                    </ViewErrorBoundary>
                  )
                )}
                <SidePanel id="right" rail={<RightRail details={!!selection} />}>
                  {selection ? (
                    <CommitDetailsPanel
                      key={selection.hash}
                      repoId={repo.id}
                      hash={selection.hash}
                    />
                  ) : (
                    <WorkingTreePanel key={`workingTreePanel-${repo.id}`} repoId={repo.id} />
                  )}
                </SidePanel>
              </>
            )}
          </PanelRow>
          <LfsManagerHost />
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
